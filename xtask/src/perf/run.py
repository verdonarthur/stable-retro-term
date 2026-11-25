"""Drive both terminals through the same Wayland and PTY paths."""
from contextlib import contextmanager
from hashlib import file_digest
from pathlib import Path
from xml.sax.saxutils import escape
import json
import os
import platform
import re
import socket
import subprocess
import sys
import tempfile
import threading
import time
import tomllib

from metrics import METRICS, compare, process_stat, sample, summarize
from wayland import DisplayTimeout, Unavailable, Wayland, visible


HERE = Path(__file__).resolve().parent
START = [37, 83, 121]
READY = [33, 81, 117]
NEXT = [51, 91, 129]
HISTORY = [71, 103, 137]


class WorkloadError(RuntimeError):
    pass


def sha(path):
    with open(path, "rb") as file:
        return file_digest(file, "sha256").hexdigest()


def command(*args, env=None):
    return subprocess.run(args, env=env, check=True, capture_output=True, text=True,
                          timeout=5).stdout.strip()


def terminal_env(fontconfig):
    env = os.environ.copy()
    env.pop("DISPLAY", None)
    env["FONTCONFIG_FILE"] = str(fontconfig)
    return env


def optional_command(*args):
    try:
        return command(*args)
    except (OSError, subprocess.SubprocessError):
        return None


def drm_devices():
    cards = {}
    for node in Path("/dev/dri").glob("renderD*"):
        device = Path("/sys/class/drm") / node.name / "device"
        try:
            cards[str(node)] = {
                "pci": device.resolve(strict=True).name,
                "driver": (device / "driver").resolve(strict=True).name,
                "vendor_id": (device / "vendor").read_text().strip(),
                "device_id": (device / "device").read_text().strip(),
                "accessible": os.access(node, os.R_OK | os.W_OK),
            }
        except OSError:
            cards[str(node)] = {"accessible": False}
    return cards


def prepare(request, directory, report):
    config = Path(request["config"])
    suite = request["suite"]
    if not os.access(request["candidate"], os.X_OK):
        raise Unavailable("candidate lacks execute permission")
    version = command(request["baseline"], "--version")
    if re.fullmatch(rf"alacritty {re.escape(suite['baseline_version'])}(?: \([^()]+\))?", version) is None:
        raise Unavailable(f"expected Alacritty {suite['baseline_version']}, got {version}")
    report["identity"] = {
        "candidate_sha256": sha(request["candidate"]), "baseline_sha256": sha(request["baseline"]),
        "baseline_version": version,
        "baseline_build": os.environ.get("RETRO_PERF_ALACRITTY_BUILD"),
        "suite_sha256": sha(request["suite_path"]), "alacritty_config_sha256": sha(config),
        "harness_sha256": {file.name: sha(file) for file in sorted(HERE.glob("*.py"))
                           if not file.name.startswith("test_")},
    }
    report["host"] = {
        "kernel": platform.release(), "machine": platform.machine(),
        "desktop": os.environ.get("XDG_CURRENT_DESKTOP"), "session": os.environ.get("XDG_SESSION_TYPE"),
        "cpu": next((row.partition(":")[2].strip() for row in Path("/proc/cpuinfo").read_text().splitlines()
                     if row.startswith("model name")), None),
        "power": os.environ.get("RETRO_PERF_POWER") or optional_command("powerprofilesctl", "get"),
        "refresh_hz": os.environ.get("RETRO_PERF_REFRESH_HZ"),
        "compositor": optional_command("kwin_wayland", "--version")
        if "KDE" in os.environ.get("XDG_CURRENT_DESKTOP", "") else None,
        "render_nodes": drm_devices(),
        "graphics_packages": optional_command("rpm", "-q", "mesa-dri-drivers", "mesa-vulkan-drivers"),
    }
    if report["host"]["session"] != "wayland" or not report["host"]["compositor"]:
        raise Unavailable("a native KDE Wayland session is required")
    if not any(node["accessible"] for node in report["host"]["render_nodes"].values()):
        raise Unavailable("no hardware DRM render node is accessible")
    if not os.environ.get("WAYLAND_DISPLAY"):
        raise Unavailable("WAYLAND_DISPLAY is unavailable")
    settings = tomllib.loads(config.read_text())
    if settings.get("general", {}).get("import") or settings.get("import"):
        raise Unavailable("the baseline config must be self-contained")
    if settings.get("scrolling", {}).get("history") != 10_000:
        raise Unavailable("the candidate requires 10000 lines of baseline scrollback")
    font = settings["font"]["normal"]
    fontconfig = directory / "fonts.conf"
    fontconfig.write_text(
        '<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">'
        '<fontconfig><include>/etc/fonts/fonts.conf</include>'
        '<alias binding="strong"><family>monospace</family><prefer><family>'
        + escape(font["family"]) + '</family></prefer></alias></fontconfig>'
    )
    env = terminal_env(fontconfig)
    fmt = "%{family}\n%{style}\n%{file}\n"
    name, style, file = command("fc-match", "-f", fmt, "monospace", env=env).splitlines()
    if name != font["family"] or style != font["style"]:
        raise Unavailable(f"configured font is unavailable (matched {name} {style})")
    report["identity"]["font"] = {"family": name, "style": style, "sha256": sha(file)}
    report["identity"]["fontconfig_sha256"] = sha(fontconfig)
    return env


def check_run_metadata(report):
    missing = []
    if not report["identity"]["baseline_build"]:
        missing.append("RETRO_PERF_ALACRITTY_BUILD")
    if not report["host"]["power"]:
        missing.append("RETRO_PERF_POWER")
    try:
        if not 1 <= float(report["host"]["refresh_hz"] or 0) <= 1000:
            raise ValueError()
    except ValueError:
        missing.append("RETRO_PERF_REFRESH_HZ")
    if missing:
        raise Unavailable("missing measurement metadata: " + ", ".join(missing))


class Recorder:
    def __init__(self, pid, period):
        self.pid = pid
        self.period = period
        self.samples = []
        self.stop = threading.Event()
        self.child_pid = None
        self.thread = threading.Thread(target=self.collect)

    def collect(self):
        while not self.stop.is_set():
            try:
                self.samples.append(sample(self.pid, self.child_pid))
            except (FileNotFoundError, ProcessLookupError):
                break
            except (OSError, ValueError):
                break
            self.stop.wait(self.period)

    def boundary(self):
        return sample(self.pid, self.child_pid)


class Fixture:
    def __init__(self, connection):
        self.connection = connection
        self.replies = connection.makefile("r", encoding="utf-8")
        self.pid = None
        self.grid = None

    def send(self, action, **args):
        self.connection.sendall((json.dumps({"action": action, **args}) + "\n").encode())

    def receive(self):
        line = self.replies.readline()
        if not line:
            raise WorkloadError("fixture exited before the workload finished")
        reply = self.reply = json.loads(line)
        self.pid, self.grid = reply["pid"], reply["grid"]
        return self.grid

    def close(self):
        self.replies.close()
        self.connection.close()


@contextmanager
def terminal(request, env, directory, kind, blink=False):
    socket_path = directory / "pty.sock"
    socket_path.unlink(missing_ok=True)
    period = request["suite"]["sample_ms"] / 1000
    timeout = request["suite"]["timeout_ms"] / 1000
    fixture_args = [sys.executable, str(HERE / "fixture.py"), str(socket_path)]
    if kind == "candidate":
        argv = [request[kind], "--", *fixture_args]
    else:
        argv = [request[kind], "--config-file", request["config"], "-o",
                f'cursor.style.blinking={"Always" if blink else "Never"!r}', "-e", *fixture_args]
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
        server.bind(str(socket_path))
        server.listen(1)
        server.settimeout(timeout)
        with tempfile.TemporaryFile() as log:
            start = time.monotonic_ns()
            child = subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL,
                                     stdout=log, stderr=log)
            recorder = Recorder(child.pid, period)
            recorder.thread.start()
            fixture = None
            try:
                conn, _ = server.accept()
                conn.settimeout(timeout)
                fixture = Fixture(conn)
                yield start, child, recorder, fixture
            finally:
                recorder.stop.set()
                recorder.thread.join()
                if fixture:
                    fixture.close()
                if child.poll() is None:
                    child.terminate()
                    try:
                        child.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait()


def observe_action(driver, fixture, color, timeout, action="draw", **args):
    begin = time.monotonic_ns()
    fixture.send(action, color=color, **args)
    grid = fixture.receive()
    return assert_frame(driver, color, grid, timeout, begin)


def assert_frame(driver, color, grid, timeout, begin, **assertions):
    remain = timeout - (time.monotonic_ns() - begin) / 1e9
    if remain <= 0:
        raise WorkloadError("workload exceeded its deadline before display confirmation")
    evidence = driver.observe(color, grid, remain, **assertions)
    evidence["elapsed_ns"] = time.monotonic_ns() - begin
    evidence["grid"] = grid
    if evidence["elapsed_ns"] > timeout * 1e9:
        raise WorkloadError("visible output exceeded its deadline")
    return evidence


def cursor_run(driver, frame, duration, blink):
    states = [frame["cursor"]]
    transitions = []
    color, grid = (START, frame["grid"])
    start = time.monotonic()
    end = start + duration
    first_missing = None
    while (remain := end - time.monotonic()) > 0:
        timeout = remain if first_missing is None else min(remain, 0.1)
        shot = driver.frame(timeout)
        if shot is not None:
            state = visible(shot, color, grid)
            if state is None:
                if first_missing is None:
                    first_missing = time.monotonic()
                if time.monotonic() - first_missing >= 0.1:
                    raise WorkloadError("terminal content changed during idle")
                continue
            first_missing = None
            if state["cursor"] != states[-1]:
                states.append(state["cursor"])
                transitions.append(time.monotonic())
        elif first_missing is not None:
            raise WorkloadError("terminal content changed during idle")
    if first_missing is not None:
        raise WorkloadError("terminal content changed at the end of idle")
    if (blink and len(states) < 3) or (not blink and states != [True]):
        raise WorkloadError("visible cursor behavior did not match the scenario")
    if blink and (transitions[0] - start > 0.65 or end - transitions[-1] > 0.65):
        raise WorkloadError("cursor stopped blinking during the workload")
    if any(not 0.35 <= later - earlier <= 0.65
           for earlier, later in zip(transitions, transitions[1:])):
        raise WorkloadError("visible cursor blink interval did not match 500 ms")
    return {"states": states, "transitions": transitions}


def window_churn(driver, frame, child, fixture, timeout):
    driver.close_window(frame)
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        exited = os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
        if exited is not None:
            # Read the emulator's own CPU time while it is still a zombie.
            stat = process_stat(Path(f"/proc/{child.pid}/stat"))
            child.wait()
            break
        time.sleep(0.01)
    else:
        raise WorkloadError("emulator did not close in time")
    if child.returncode != 0:
        raise WorkloadError("emulator exited with an error after close")
    end = time.monotonic() + timeout
    while Path(f"/proc/{fixture.pid}").exists():
        if time.monotonic() >= end:
            raise WorkloadError("PTY fixture survived window close")
        time.sleep(0.01)
    return stat


def leg(request, driver, env, directory, kind, scenario):
    suite = request["suite"]
    timeout = suite["timeout_ms"] / 1000
    result = {"valid": False, "metrics": dict.fromkeys(METRICS), "engines": None, "evidence": []}
    recorder = None
    try:
        with terminal(request, env, directory, kind, scenario == "blink") as (
            launch, child, recorder, fixture
        ):
            frame = observe_action(driver, fixture, READY, timeout, blink=scenario == "blink")
            recorder.child_pid = fixture.pid
            result["evidence"].append(frame)
            first = None
            exited = None
            if scenario == "startup":
                frame["elapsed_ns"] = time.monotonic_ns() - launch
                if frame["elapsed_ns"] > timeout * 1e9:
                    raise WorkloadError("first visible output exceeded the startup deadline")
                begin = launch
            else:
                driver.focus(frame)
                fixture.send("key", key=" ", timeout=timeout)
                driver.keys(57)
                fixture.receive()
                if request["scenario"] == "output" and kind == "baseline":
                    fixture.send("enable_mouse")
                    fixture.receive()
                    driver.scroll(frame, -240)
                    fixture.send("read_wheel", timeout=timeout / 2)
                    fixture.receive()
                    result["wheel_probe"] = fixture.reply["wheel"]
                time.sleep(0.25)
                frame = observe_action(driver, fixture, START, timeout, blink=scenario == "blink")
                result["evidence"].append(frame)
                first = recorder.boundary()
                begin = launch if scenario == "lifecycle" else first["time_ns"]
            if scenario in ("idle", "blink"):
                result["cursor_states"] = cursor_run(driver, frame, suite["idle_ms"] / 1000,
                                                     scenario == "blink")
            if scenario == "typing":
                for index, key in enumerate("asdfjkl"):
                    color = [81, 97, 150 + index]
                    sent = time.monotonic_ns()
                    fixture.send("key", key=key, color=color, column=index + 1, timeout=timeout)
                    driver.keys({"a": 30, "s": 31, "d": 32, "f": 33, "j": 36, "k": 37, "l": 38}[key])
                    result["evidence"].append(assert_frame(
                        driver, START, fixture.receive(), timeout, sent,
                        typed=(index + 1, color)
                    ))
                    time.sleep(0.08)
            if scenario == "output":
                for index in range(4):
                    last = observe_action(driver, fixture, [51, 91, 129 + index], timeout, "output",
                                          history=HISTORY, lines=300)
                    result["evidence"].append(last)
                driver.scroll(last, -240)
                # Verify a separate visible marker in the scrolled text.
                driver.observe_partial(HISTORY, last, timeout)
            if scenario == "redraw":
                for index in range(20):
                    result["evidence"].append(
                        observe_action(driver, fixture, [51, 91, 130 + index], timeout)
                    )
                    time.sleep(0.05)
            if scenario == "resize":
                for index, (cols, rows) in enumerate(((10, 3), (-20, -6), (10, 3))):
                    sent = time.monotonic_ns()
                    old_rows, old_cols, width, height = frame["grid"]
                    dx, dy = cols * (width // old_cols), rows * (height // old_rows)
                    expected = [old_rows + rows, old_cols + cols, width + dx, height + dy]
                    driver.resize(frame, dx, dy)
                    while True:
                        fixture.send("size")
                        grid = fixture.receive()
                        if grid == expected:
                            break
                        if time.monotonic_ns() - sent > timeout * 1e9:
                            raise WorkloadError("resize was not delivered to the PTY")
                        time.sleep(0.01)
                    color = [NEXT[0], NEXT[1], NEXT[2] + index]
                    fixture.send("draw", color=color)
                    frame = assert_frame(driver, color, fixture.receive(), timeout, sent)
                    result["evidence"].append(frame)
            if scenario == "lifecycle":
                exited = window_churn(driver, frame, child, fixture, timeout)
                end = time.monotonic_ns()
            else:
                last = recorder.boundary()
                end = last["time_ns"]
            result["valid"] = True
        result["raw_samples"] = recorder.samples
        window = [entry for entry in recorder.samples if begin < entry["time_ns"] < end]
        if first is not None and scenario != "lifecycle":
            window.insert(0, first)
        if exited is None:
            window.append(last)
        result["interval_ns"] = [begin, end]
        # A final live /proc sample cannot cover work between it and process exit.
        result["metrics"], result["engines"] = summarize(
            window, from_launch=scenario in ("startup", "lifecycle")
        )
        result["render_nodes"] = sorted({node for entry in window for node in entry["render_nodes"]})
        if exited is not None:
            if window and exited["identity"] == window[0]["identity"]:
                result["metrics"]["cpu_ns"] = exited["cpu_ns"]
            result["metrics"]["peak_rss_bytes"] = None
            result["metrics"]["gpu_ns"], result["engines"] = None, None
            result["reason"] = "full-window peak RSS and final DRM counters are unavailable at exit"
    except (WorkloadError, DisplayTimeout, TimeoutError, OSError) as error:
        result["reason"] = str(error)
        mismatch = isinstance(error, WorkloadError) or (
            isinstance(error, DisplayTimeout) and error.received_frames
        )
        result["workload_status"] = "failed" if mismatch else "unverified"
        if recorder:
            result["raw_samples"] = recorder.samples
    return result


def matching(pair):
    a, b = pair["candidate"], pair["baseline"]
    if not a["valid"] or not b["valid"]:
        return
    if [frame["grid"] for frame in a["evidence"]] != [frame["grid"] for frame in b["evidence"]]:
        reason = "candidate and baseline had different visible grids"
    elif len(a.get("render_nodes", [])) != 1 or a["render_nodes"] != b.get("render_nodes"):
        reason = "could not confirm both emulators used the same hardware render node"
    else:
        reason = None
    if reason:
        for result in (a, b):
            result["valid"] = False
            result["reason"] = reason


def run_suite(request, driver, env, directory, report):
    results = report["scenarios"]
    gpu_resolution = report["host"]["drm_resolution_ns"]
    scenarios = [request["scenario"]] if request["scenario"] else request["suite"]["scenarios"]
    for scenario in scenarios:
        pairs = []
        for index in range(request["suite"]["repetitions"]):
            order = ["baseline", "candidate"] if index % 2 == 0 else ["candidate", "baseline"]
            pair = {"order": order}
            for kind in order:
                pair[kind] = leg(request, driver, env, directory, kind, scenario)
            matching(pair)
            pairs.append(pair)
        metrics = {metric: compare(pairs, metric, gpu_resolution) for metric in METRICS}
        failed = any(pair["baseline"]["valid"] and pair["candidate"].get("workload_status") == "failed"
                     for pair in pairs)
        statuses = {entry["status"] for entry in metrics.values()}
        status = "failed" if failed or "failed" in statuses else (
            "passed" if statuses == {"passed"} else "unverified"
        )
        results.append({"scenario": scenario, "status": status, "metrics": metrics, "pairs": pairs})


def run(request):
    report = {"status": "unverified", "reasons": [], "scenarios": []}
    with tempfile.TemporaryDirectory(prefix="retro-perf-") as temp:
        directory = Path(temp)
        try:
            env = prepare(request, directory, report)
            resolution = os.environ.get("RETRO_PERF_DRM_RESOLUTION_NS")
            resolution_source = os.environ.get("RETRO_PERF_DRM_RESOLUTION_SOURCE")
            if resolution is not None and (not resolution.isdecimal() or
                                           int(resolution) <= 0 or not resolution_source):
                raise Unavailable("DRM resolution requires a positive ns bound and its evidence source")
            report["host"]["drm_resolution_ns"] = int(resolution) if resolution else None
            report["host"]["drm_resolution_source"] = resolution_source
            driver = Wayland()
            driver.preflight()
            report["host"]["portal_capabilities"] = ["keyboard", "pointer", "output capture"]
            check_run_metadata(report)
            if request["preflight"]:
                report["status"] = "passed"
                return report
            with driver.open():
                report["host"]["output_pixels"] = driver.size
                report["host"]["output_scale"] = 1
                # Give KDE's selection dialog and recording notification time to clear.
                time.sleep(1)
                run_suite(request, driver, env, directory, report)
            states = {row["status"] for row in report["scenarios"]}
            if "failed" in states:
                report["status"] = "failed"
            elif states == {"passed"} and not request["scenario"]:
                report["status"] = "passed"
        except Exception as error:
            report["reasons"].append(f"{type(error).__name__}: {error}")
    return report


if __name__ == "__main__":
    print(json.dumps(run(json.load(sys.stdin)), allow_nan=False))
