"""Deterministic child attached to either emulator's real PTY."""
import fcntl
import json
import os
import re
import select
import socket
import struct
import sys
import termios
import tty


def grid():
    rows, cols, width, height = struct.unpack(
        "HHHH", fcntl.ioctl(0, termios.TIOCGWINSZ, bytes(8))
    )
    return [rows, cols, width, height]


def background(color):
    return "\x1b[48;2;" + ";".join(map(str, color)) + "m"


def draw(color, blink):
    rows, cols, _, _ = grid()
    fill = background(color)
    lines = []
    for row in range(1, rows + 1):
        text = "Retro benchmark 0123456789 ".ljust(cols)[:cols] if row == 3 else " " * cols
        lines.append(f"\x1b[{row};1H{fill}{text}")
    style = 1 if blink else 2
    return "".join(["\x1b[?25l", *lines, f"\x1b[2;2H\x1b[{style} q\x1b[?25h\x1b[0m"])


def main():
    tty.setraw(sys.stdin.fileno())
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(sys.argv[1])
        with client.makefile("r", encoding="utf-8") as commands:
            for line in commands:
                request = json.loads(line)
                kind = request["action"]
                if kind == "exit":
                    break
                if kind == "draw":
                    sys.stdout.write(draw(request["color"], request.get("blink", False)))
                if kind == "enable_mouse":
                    sys.stdout.write("\x1b[?1000h\x1b[?1006h")
                if kind == "read_wheel":
                    data = bytearray()
                    while len(data) < 1024 and not re.search(rb"\x1b\[<(64|65);\d+;\d+M", data):
                        if not select.select([sys.stdin], [], [], request["timeout"])[0]:
                            break
                        data.extend(os.read(0, 256))
                    sys.stdout.write("\x1b[?1000l\x1b[?1006l")
                if kind == "output":
                    cols = grid()[1]
                    for index in range(request["lines"]):
                        color = request["history"] if index >= request["lines"] - 120 else request["color"]
                        text = f"output line {index:04}".ljust(cols)[:cols]
                        sys.stdout.write(f"\r{background(color)}{text}\x1b[0m\r\n")
                    sys.stdout.write(draw(request["color"], False))
                if kind == "key":
                    if not select.select([sys.stdin], [], [], request["timeout"])[0]:
                        raise TimeoutError("fixture did not receive a key")
                    key = os.read(0, 1).decode("ascii")
                    if key != request["key"]:
                        raise ValueError(f"fixture got unexpected input {key!r}")
                    if request.get("color") is not None:
                        column = request["column"]
                        sys.stdout.write(
                            f"\x1b[4;{column}H{background(request['color'])}{key}"
                            "\x1b[0m\x1b[2;2H"
                        )
                sys.stdout.flush()
                reply = {"grid": grid(), "pid": os.getpid()}
                if kind == "read_wheel":
                    match = re.search(rb"\x1b\[<(64|65);\d+;\d+M", data)
                    reply["wheel"] = int(match[1]) if match else None
                client.sendall((json.dumps(reply) + "\n").encode())


if __name__ == "__main__":
    main()
