"""Shared XDG RemoteDesktop input and PipeWire capture for a dedicated KDE output."""
from contextlib import ExitStack, contextmanager
import os
import time
import uuid


class Unavailable(RuntimeError):
    pass


class DisplayTimeout(RuntimeError):
    def __init__(self, message, received_frames=False):
        super().__init__(message)
        self.received_frames = received_frames


DEST = "org.freedesktop.portal.Desktop"
PATH = "/org/freedesktop/portal/desktop"
PORTAL = "org.freedesktop.portal."


class Wayland:
    def __init__(self):
        try:
            import gi
            gi.require_version("Gst", "1.0")
            gi.require_version("GstVideo", "1.0")
            from gi.repository import Gio, GLib, Gst, GstVideo
        except (ImportError, ValueError) as error:
            raise Unavailable(f"PyGObject and GStreamer introspection are required: {error}") from error
        self.gio, self.glib, self.gst, self.video = Gio, GLib, Gst, GstVideo
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION)
        self.v = GLib.Variant
        Gst.init(None)
        if not Gst.ElementFactory.find("pipewiresrc"):
            raise Unavailable("GStreamer pipewiresrc is unavailable")

    def call(self, interface, method, signature, values, path=PATH):
        return self.bus.call_sync(
            DEST, path, PORTAL + interface, method, self.v(signature, values), None,
            self.gio.DBusCallFlags.NONE, 5_000, None,
        ).unpack()

    def preflight(self):
        def prop(interface, name):
            return self.bus.call_sync(
                DEST, PATH, "org.freedesktop.DBus.Properties", "Get",
                self.v("(ss)", (PORTAL + interface, name)), None,
                self.gio.DBusCallFlags.NONE, 5_000, None,
            ).unpack()[0]

        if prop("RemoteDesktop", "AvailableDeviceTypes") & 3 != 3:
            raise Unavailable("portal does not offer both keyboard and pointer input")
        if not prop("ScreenCast", "AvailableSourceTypes") & 1:
            raise Unavailable("portal does not offer output capture")

    def request(self, interface, method, signature, *values, **options):
        token = "retro_" + uuid.uuid4().hex
        options["handle_token"] = self.v("s", token)
        sender = self.bus.get_unique_name()[1:].replace(".", "_")
        expected = f"{PATH}/request/{sender}/{token}"
        loop = self.glib.MainLoop()
        responses = []

        def receive(_bus, _sender, _path, _interface, _signal, params, _data):
            responses.append(params.unpack())
            loop.quit()

        subscription = self.bus.signal_subscribe(
            DEST, PORTAL + "Request", "Response", expected, None,
            self.gio.DBusSignalFlags.NONE, receive, None,
        )

        def expire():
            loop.quit()
            return self.glib.SOURCE_CONTINUE

        timer = self.glib.timeout_add_seconds(90, expire)
        try:
            (handle,) = self.call(interface, method, signature, (*values, options))
            if handle != expected:
                raise Unavailable("portal did not honor the request token")
            if not responses:
                loop.run()
            if not responses or responses[0][0]:
                raise Unavailable(f"portal {method} timed out, was cancelled, or was denied")
            return responses[0][1]
        finally:
            self.glib.source_remove(timer)
            self.bus.signal_unsubscribe(subscription)

    @contextmanager
    def open(self):
        session_token = self.v("s", "retro_" + uuid.uuid4().hex)
        self.session = self.request(
            "RemoteDesktop", "CreateSession", "(a{sv})", session_handle_token=session_token
        )["session_handle"]
        with ExitStack() as cleanup:
            cleanup.callback(self.call, "Session", "Close", "()", (), self.session)
            self.request("RemoteDesktop", "SelectDevices", "(oa{sv})", self.session,
                         types=self.v("u", 3))
            self.request("ScreenCast", "SelectSources", "(oa{sv})", self.session,
                         types=self.v("u", 1), multiple=self.v("b", False),
                         cursor_mode=self.v("u", 1))
            response = self.request("RemoteDesktop", "Start", "(osa{sv})", self.session, "")
            streams = response.get("streams", [])
            if response.get("devices", 0) & 3 != 3 or len(streams) != 1:
                raise Unavailable("select one output and allow keyboard and pointer access")
            self.node, properties = streams[0]
            self.size = tuple(properties.get("size", ()))
            reply, descriptors = self.bus.call_with_unix_fd_list_sync(
                DEST, PATH, PORTAL + "ScreenCast", "OpenPipeWireRemote",
                self.v("(oa{sv})", (self.session, {})), None, self.gio.DBusCallFlags.NONE,
                5_000, None, None,
            )
            fd = descriptors.get(reply.unpack()[0])
            cleanup.callback(os.close, fd)
            self.pipeline = self.gst.parse_launch(
                f"pipewiresrc fd={fd} path={int(self.node)} ! videoconvert ! "
                "video/x-raw,format=BGRx ! appsink name=frames max-buffers=1 drop=true sync=false"
            )
            cleanup.callback(self.pipeline.set_state, self.gst.State.NULL)
            self.sink = self.pipeline.get_by_name("frames")
            self.pipeline.set_state(self.gst.State.PLAYING)
            frame = self.frame(5)
            if frame is None or self.size != frame[:2]:
                raise Unavailable("capture needs an unscaled output with a confirmed pixel size")
            yield self

    def frame(self, timeout):
        sample = self.sink.emit("try-pull-sample", int(timeout * self.gst.SECOND))
        if sample is None:
            return None
        info = self.video.VideoInfo.new_from_caps(sample.get_caps())
        buffer = sample.get_buffer()
        return info.width, info.height, info.stride[0], info.offset[0], buffer.extract_dup(0, buffer.get_size())

    def observe(self, color, grid, timeout, typed=None):
        end = time.monotonic() + timeout
        received = False
        while time.monotonic() < end:
            frame = self.frame(max(0, end - time.monotonic()))
            if frame:
                received = True
                evidence = visible(frame, color, grid)
                if evidence is not None and (typed is None or typed_cell(frame, evidence, grid, typed)):
                    return evidence
        raise DisplayTimeout(f"missing visible marker {color} for grid {grid}", received)

    def observe_partial(self, color, evidence, timeout):
        x, y, width, height = evidence["area"]
        expected = bytes((color[2], color[1], color[0]))
        end = time.monotonic() + timeout
        received = False
        while (remain := end - time.monotonic()) > 0:
            frame = self.frame(remain)
            if frame is None:
                continue
            received = True
            if any(
                pixel(frame, x + width // 2, row) == expected
                and pixel(frame, x + width - 2, row) == expected
                for row in range(y, y + height)
            ):
                return
        raise DisplayTimeout("scrollback marker was not visible", received)

    def keys(self, *keys):
        for state, sequence in [(1, keys), (0, reversed(keys))]:
            for key in sequence:
                self.call("RemoteDesktop", "NotifyKeyboardKeycode", "(oa{sv}iu)",
                          (self.session, {}, key, state))

    def move(self, x, y):
        self.call("RemoteDesktop", "NotifyPointerMotionAbsolute", "(oa{sv}udd)",
                  (self.session, {}, self.node, float(x), float(y)))

    def button(self, button, state):
        self.call("RemoteDesktop", "NotifyPointerButton", "(oa{sv}iu)",
                  (self.session, {}, button, state))

    def focus(self, evidence):
        x, y, width, height = evidence["area"]
        self.move(x + width // 2, y + height // 2)
        self.button(272, 1)
        self.button(272, 0)

    def resize(self, evidence, dx, dy):
        # KDE's Meta + right-button resize gesture.
        x, y, width, height = evidence["area"]
        x, y = x + width - 12, y + height - 12
        self.move(x, y)
        try:
            self.call("RemoteDesktop", "NotifyKeyboardKeycode", "(oa{sv}iu)",
                      (self.session, {}, 125, 1))
            self.button(273, 1)
            try:
                self.move(x + dx, y + dy)
            finally:
                self.button(273, 0)
        finally:
            self.call("RemoteDesktop", "NotifyKeyboardKeycode", "(oa{sv}iu)",
                      (self.session, {}, 125, 0))

    def close_window(self, evidence):
        self.focus(evidence)
        self.keys(56, 62)  # Alt+F4

    def scroll(self, evidence, pixels):
        x, y, width, height = evidence["area"]
        self.move(x + width // 2, y + height // 2)
        # Negative pixels scroll up. KWin reads positive portal steps as up.
        self.call("RemoteDesktop", "NotifyPointerAxisDiscrete", "(oa{sv}ui)",
                  (self.session, {}, 0, -pixels // 80))
        self.call("RemoteDesktop", "NotifyPointerAxis", "(oa{sv}dd)",
                  (self.session, {"finish": self.v("b", True)}, 0.0, float(-pixels)))


def visible(frame, color, grid):
    """Find the colored grid perimeter and probe the cursor cell inside it."""
    width, height, stride, offset, pixels = frame
    rows, cols, grid_width, grid_height = grid
    if min(rows, cols) < 6 or not grid_width or not grid_height:
        return None
    if grid_width > width or grid_height > height:
        return None
    cell_w, cell_h = grid_width // cols, grid_height // rows
    if not cell_w or not cell_h or cell_w * cols != grid_width or cell_h * rows != grid_height:
        return None
    pattern = bytes((color[2], color[1], color[0]))
    # The X byte is padding. Use repeated RGB samples at aligned pixel offsets.
    count = 0
    position = pixels.find(pattern, offset)
    while position != -1:
        y, x_bytes = divmod(position - offset, stride)
        if x_bytes % 4 == 0:
            x = x_bytes // 4
            if x + grid_width <= width and y + grid_height <= height:
                checks = [
                    (x + grid_width - 1, y),
                    (x, y + grid_height - 1),
                    (x + grid_width - 1, y + grid_height - 1),
                    (x + grid_width // 2, y + grid_height // 2),
                ]
                if all(pixel(frame, px, py) == pattern for px, py in checks):
                    if (x == 0 or pixel(frame, x - 1, y) != pattern) and (
                        y == 0 or pixel(frame, x, y - 1) != pattern
                    ) and text_visible(frame, x, y + 2 * cell_h, cell_w, cell_h, pattern):
                        cursor = pixel(frame, x + cell_w + cell_w // 2, y + cell_h + cell_h // 2)
                        return {"area": [x, y, grid_width, grid_height], "cursor": cursor != pattern}
        position = pixels.find(pattern, position + 1)
        count += 1
        if count > 2 * width:
            break
    return None


def pixel(frame, x, y):
    _, _, stride, offset, pixels = frame
    index = offset + y * stride + x * 4
    return pixels[index:index + 3]


def text_visible(frame, x, y, cell_w, cell_h, background):
    # The fixture starts its text row with five non-space glyphs.
    return all(any(pixel(frame, x + col * cell_w + dx, y + dy) != background
                   for dy in range(cell_h) for dx in range(cell_w))
               for col in range(5))


def typed_cell(frame, evidence, grid, typed):
    column, color = typed
    x, y, width, height = evidence["area"]
    rows, cols, _, _ = grid
    cell_w, cell_h = width // cols, height // rows
    x += (column - 1) * cell_w
    y += 3 * cell_h
    background = bytes((color[2], color[1], color[0]))
    if pixel(frame, x, y) != background or pixel(frame, x + cell_w - 1, y + cell_h - 1) != background:
        return False
    return any(pixel(frame, x + dx, y + dy) != background
               for dy in range(cell_h) for dx in range(cell_w))
