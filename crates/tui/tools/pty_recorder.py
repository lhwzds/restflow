#!/usr/bin/env python3
"""Record a PTY session into replayable text and SVG frames.

This is a dependency-free diagnostic harness for RestFlow's TUI. It runs an
arbitrary command inside a real pseudo terminal, injects scripted input events,
and writes a frame every time the terminal screen changes.

Example:
    python3 crates/tui/tools/pty_recorder.py \
        --out target/tui-pty-recordings/manual \
        --cols 100 --rows 30 \
        --events /tmp/restflow-events.json \
        -- target/release/restflow

Event file format:
    [
      {"after_ms": 500, "text": "hello"},
      {"after_ms": 100, "key": "enter"},
      {"after_ms": 1000, "key": "esc"}
    ]
"""

from __future__ import annotations

import argparse
import codecs
import html
import json
import os
import pty
import select
import signal
import struct
import sys
import termios
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


KEY_BYTES = {
    "enter": b"\r",
    "return": b"\r",
    "esc": b"\x1b",
    "escape": b"\x1b",
    "tab": b"\t",
    "backspace": b"\x7f",
    "delete": b"\x1b[3~",
    "ctrl-c": b"\x03",
    "ctrl-d": b"\x04",
    "up": b"\x1b[A",
    "down": b"\x1b[B",
    "right": b"\x1b[C",
    "left": b"\x1b[D",
}


@dataclass
class InputEvent:
    at: float
    label: str
    payload: bytes


class MiniTerminal:
    """Small ANSI terminal model.

    It intentionally implements the subset emitted by common full-screen TUIs:
    cursor movement, clear screen/line, SGR styling ignores, and scroll regions.
    Raw bytes are still written to raw.log, so parser gaps can be debugged.
    """

    def __init__(self, rows: int, cols: int) -> None:
        self.rows = rows
        self.cols = cols
        self.screen = [[" " for _ in range(cols)] for _ in range(rows)]
        self.row = 0
        self.col = 0
        self.scroll_top = 0
        self.scroll_bottom = rows - 1
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.state = "normal"
        self.csi = ""
        self.osc = ""

    def feed(self, data: bytes) -> bool:
        before = self.text()
        for ch in self.decoder.decode(data):
            self._feed_char(ch)
        return self.text() != before

    def text(self) -> str:
        return "\n".join("".join(row).rstrip() for row in self.screen)

    def _feed_char(self, ch: str) -> None:
        if self.state == "normal":
            if ch == "\x1b":
                self.state = "esc"
            elif ch == "\r":
                self.col = 0
            elif ch == "\n":
                self._newline()
            elif ch == "\b":
                self.col = max(0, self.col - 1)
            elif ch == "\t":
                next_tab = min(self.cols, ((self.col // 8) + 1) * 8)
                while self.col < next_tab:
                    self._put(" ")
            elif ch >= " ":
                self._put(ch)
            return

        if self.state == "esc":
            if ch == "[":
                self.csi = ""
                self.state = "csi"
            elif ch == "]":
                self.osc = ""
                self.state = "osc"
            elif ch in "78=":
                self.state = "normal"
            else:
                self.state = "normal"
            return

        if self.state == "osc":
            if ch == "\x07":
                self.state = "normal"
            elif ch == "\x1b":
                self.state = "osc_esc"
            else:
                self.osc += ch
            return

        if self.state == "osc_esc":
            self.state = "normal" if ch == "\\" else "osc"
            return

        if self.state == "csi":
            if "@" <= ch <= "~":
                self._handle_csi(self.csi, ch)
                self.state = "normal"
            else:
                self.csi += ch

    def _put(self, ch: str) -> None:
        if self.col >= self.cols:
            self._newline()
        if 0 <= self.row < self.rows and 0 <= self.col < self.cols:
            self.screen[self.row][self.col] = ch
        self.col += 1

    def _newline(self) -> None:
        if self.row == self.scroll_bottom:
            self._scroll_up()
        else:
            self.row = min(self.rows - 1, self.row + 1)

    def _scroll_up(self) -> None:
        top = max(0, min(self.scroll_top, self.rows - 1))
        bottom = max(top, min(self.scroll_bottom, self.rows - 1))
        del self.screen[top]
        self.screen.insert(bottom, [" " for _ in range(self.cols)])

    def _scroll_down(self) -> None:
        top = max(0, min(self.scroll_top, self.rows - 1))
        bottom = max(top, min(self.scroll_bottom, self.rows - 1))
        del self.screen[bottom]
        self.screen.insert(top, [" " for _ in range(self.cols)])

    def _handle_csi(self, raw: str, final: str) -> None:
        private = raw.startswith("?")
        if private:
            raw = raw[1:]
        parts = [part for part in raw.split(";") if part != ""]
        nums = []
        for part in parts:
            try:
                nums.append(int(part))
            except ValueError:
                nums.append(0)

        def num(index: int, default: int) -> int:
            if index >= len(nums) or nums[index] == 0:
                return default
            return nums[index]

        if final in ("H", "f"):
            self.row = self._clamp(num(0, 1) - 1, 0, self.rows - 1)
            self.col = self._clamp(num(1, 1) - 1, 0, self.cols - 1)
        elif final == "A":
            self.row = self._clamp(self.row - num(0, 1), 0, self.rows - 1)
        elif final == "B":
            self.row = self._clamp(self.row + num(0, 1), 0, self.rows - 1)
        elif final == "C":
            self.col = self._clamp(self.col + num(0, 1), 0, self.cols - 1)
        elif final == "D":
            self.col = self._clamp(self.col - num(0, 1), 0, self.cols - 1)
        elif final == "G":
            self.col = self._clamp(num(0, 1) - 1, 0, self.cols - 1)
        elif final == "J":
            self._clear_screen(num(0, 0))
        elif final == "K":
            self._clear_line(num(0, 0))
        elif final == "S":
            for _ in range(num(0, 1)):
                self._scroll_up()
        elif final == "T":
            for _ in range(num(0, 1)):
                self._scroll_down()
        elif final == "r":
            self.scroll_top = self._clamp(num(0, 1) - 1, 0, self.rows - 1)
            self.scroll_bottom = self._clamp(num(1, self.rows) - 1, self.scroll_top, self.rows - 1)
            self.row = self.scroll_top
            self.col = 0
        elif final in ("h", "l", "m"):
            pass

    def _clear_screen(self, mode: int) -> None:
        if mode in (2, 3):
            self.screen = [[" " for _ in range(self.cols)] for _ in range(self.rows)]
            self.row = 0
            self.col = 0
        elif mode == 0:
            self._clear_line(0)
            for row in range(self.row + 1, self.rows):
                self.screen[row] = [" " for _ in range(self.cols)]
        elif mode == 1:
            for row in range(0, self.row):
                self.screen[row] = [" " for _ in range(self.cols)]
            self._clear_line(1)

    def _clear_line(self, mode: int) -> None:
        if not 0 <= self.row < self.rows:
            return
        if mode == 0:
            start, end = self.col, self.cols
        elif mode == 1:
            start, end = 0, self.col + 1
        else:
            start, end = 0, self.cols
        for col in range(self._clamp(start, 0, self.cols), self._clamp(end, 0, self.cols)):
            self.screen[self.row][col] = " "

    @staticmethod
    def _clamp(value: int, lower: int, upper: int) -> int:
        return max(lower, min(upper, value))


class Recorder:
    def __init__(self, out_dir: Path, rows: int, cols: int, frame_limit: int) -> None:
        self.out_dir = out_dir
        self.frames_dir = out_dir / "frames"
        self.frames_dir.mkdir(parents=True, exist_ok=True)
        self.events_file = (out_dir / "events.jsonl").open("w", encoding="utf-8")
        self.raw_file = (out_dir / "raw.log").open("wb")
        self.rows = rows
        self.cols = cols
        self.frame_limit = frame_limit
        self.frame_count = 0
        self.start = time.monotonic()
        self.last_text = ""

    def close(self) -> None:
        self.events_file.close()
        self.raw_file.close()

    def elapsed_ms(self) -> int:
        return int((time.monotonic() - self.start) * 1000)

    def write_event(self, event_type: str, **fields: Any) -> None:
        payload = {"time_ms": self.elapsed_ms(), "type": event_type, **fields}
        self.events_file.write(json.dumps(payload, ensure_ascii=False) + "\n")
        self.events_file.flush()

    def write_raw(self, data: bytes) -> None:
        self.raw_file.write(data)
        self.raw_file.flush()

    def write_frame(self, text: str, reason: str) -> None:
        if text == self.last_text:
            return
        if self.frame_count >= self.frame_limit:
            return
        self.last_text = text
        self.frame_count += 1
        stem = f"{self.frame_count:06d}_{self.elapsed_ms():08d}ms"
        txt_path = self.frames_dir / f"{stem}.txt"
        svg_path = self.frames_dir / f"{stem}.svg"
        txt_path.write_text(text + "\n", encoding="utf-8")
        svg_path.write_text(render_svg(text, self.rows, self.cols), encoding="utf-8")
        self.write_event(
            "frame",
            frame=self.frame_count,
            reason=reason,
            text=str(txt_path.relative_to(self.out_dir)),
            svg=str(svg_path.relative_to(self.out_dir)),
        )


def render_svg(text: str, rows: int, cols: int) -> str:
    char_width = 8
    line_height = 18
    padding_x = 10
    padding_y = 18
    width = max(320, cols * char_width + padding_x * 2)
    height = max(120, rows * line_height + padding_y)
    lines = text.splitlines()
    tspans = []
    for index in range(rows):
        line = html.escape(lines[index] if index < len(lines) else "")
        y = padding_y + index * line_height
        tspans.append(f'<tspan x="{padding_x}" y="{y}">{line}</tspan>')
    return "\n".join(
        [
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
            '<rect width="100%" height="100%" fill="#0b0f14"/>',
            f'<text font-family="Menlo, Consolas, monospace" font-size="14" fill="#d5dde5">{"".join(tspans)}</text>',
            "</svg>",
        ]
    )


def parse_events(path: Path | None, quick_inputs: list[str], quick_keys: list[str], delay_ms: int) -> list[InputEvent]:
    raw_events: list[dict[str, Any]] = []
    if path is not None:
        loaded = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(loaded, list):
            raise ValueError("event file must contain a JSON array")
        raw_events.extend(loaded)
    for value in quick_inputs:
        raw_events.append({"after_ms": delay_ms, "text": decode_text(value)})
    for value in quick_keys:
        raw_events.append({"after_ms": delay_ms, "key": value})

    events: list[InputEvent] = []
    current = 0.0
    for item in raw_events:
        if not isinstance(item, dict):
            raise ValueError(f"event must be an object: {item!r}")
        current += float(item.get("after_ms", 0)) / 1000.0
        if "text" in item:
            text = str(item["text"])
            events.append(InputEvent(current, f"text:{preview(text)}", text.encode("utf-8")))
        elif "key" in item:
            key = str(item["key"]).lower()
            if key not in KEY_BYTES:
                raise ValueError(f"unsupported key {key!r}; supported keys: {', '.join(sorted(KEY_BYTES))}")
            events.append(InputEvent(current, f"key:{key}", KEY_BYTES[key]))
        else:
            raise ValueError(f"event needs text or key: {item!r}")
    return events


def decode_text(value: str) -> str:
    return bytes(value, "utf-8").decode("unicode_escape")


def preview(value: str, limit: int = 40) -> str:
    value = value.replace("\n", "\\n").replace("\r", "\\r")
    return value if len(value) <= limit else value[: limit - 1] + "..."


def set_winsize(fd: int, rows: int, cols: int) -> None:
    size = struct.pack("HHHH", rows, cols, 0, 0)
    termios.tcsetwinsize(fd, (rows, cols)) if hasattr(termios, "tcsetwinsize") else None
    try:
        import fcntl

        fcntl.ioctl(fd, termios.TIOCSWINSZ, size)
    except Exception:
        pass


def build_env(args: argparse.Namespace, out_dir: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["TERM"] = args.term
    env["COLUMNS"] = str(args.cols)
    env["LINES"] = str(args.rows)
    if args.sandbox_home:
        home = out_dir / "home"
        home.mkdir(parents=True, exist_ok=True)
        env["HOME"] = str(home)
        env.setdefault("RESTFLOW_DIR", str(home / ".restflow"))
    return env


def run_session(args: argparse.Namespace) -> int:
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        raise SystemExit("missing command; pass it after --")

    out_dir = args.out or Path("target") / "tui-pty-recordings" / time.strftime("%Y%m%d-%H%M%S")
    out_dir = out_dir.resolve()
    if out_dir.exists() and any(out_dir.iterdir()):
        raise SystemExit(f"output directory already exists and is not empty: {out_dir}")
    out_dir.mkdir(parents=True, exist_ok=True)

    events = parse_events(args.events, args.send, args.key, args.send_delay_ms)
    terminal = MiniTerminal(args.rows, args.cols)
    recorder = Recorder(out_dir, args.rows, args.cols, args.frame_limit)
    cwd = Path(args.cwd).resolve()
    env = build_env(args, out_dir)
    start_time = time.time()
    exit_code: int | None = None

    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(cwd)
        os.environ.clear()
        os.environ.update(env)
        os.execvp(command[0], command)

    set_winsize(fd, args.rows, args.cols)
    os.set_blocking(fd, False)
    recorder.write_event("start", command=command, cwd=str(cwd), rows=args.rows, cols=args.cols)

    next_event = 0
    deadline = time.monotonic() + args.timeout
    try:
        while True:
            now = time.monotonic()
            while next_event < len(events) and events[next_event].at <= now - recorder.start:
                event = events[next_event]
                os.write(fd, event.payload)
                recorder.write_event("input", label=event.label)
                next_event += 1

            timeout = 0.05
            if next_event < len(events):
                due = recorder.start + events[next_event].at
                timeout = max(0.0, min(timeout, due - now))

            readable, _, _ = select.select([fd], [], [], timeout)
            if readable:
                try:
                    data = os.read(fd, 65536)
                except BlockingIOError:
                    data = b""
                except OSError:
                    data = b""
                if data:
                    recorder.write_raw(data)
                    if terminal.feed(data):
                        recorder.write_frame(terminal.text(), "pty-output")

            done_pid, status = os.waitpid(pid, os.WNOHANG)
            if done_pid == pid:
                if os.WIFEXITED(status):
                    exit_code = os.WEXITSTATUS(status)
                elif os.WIFSIGNALED(status):
                    exit_code = 128 + os.WTERMSIG(status)
                break

            if time.monotonic() > deadline:
                recorder.write_event("timeout", timeout_seconds=args.timeout)
                os.kill(pid, signal.SIGTERM)
                time.sleep(0.2)
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                exit_code = 124
                break
    finally:
        recorder.write_frame(terminal.text(), "final")
        duration_ms = int((time.time() - start_time) * 1000)
        manifest = {
            "command": command,
            "cwd": str(cwd),
            "rows": args.rows,
            "cols": args.cols,
            "exit_code": exit_code,
            "duration_ms": duration_ms,
            "frames": recorder.frame_count,
            "events": "events.jsonl",
            "raw": "raw.log",
        }
        (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
        recorder.write_event("finish", exit_code=exit_code, frames=recorder.frame_count)
        recorder.close()
        try:
            os.close(fd)
        except OSError:
            pass

    print(out_dir)
    return int(exit_code or 0)


def make_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, help="Output directory. Defaults to target/tui-pty-recordings/<timestamp>.")
    parser.add_argument("--cwd", default=os.getcwd(), help="Working directory for the child process.")
    parser.add_argument("--rows", type=int, default=30, help="PTY rows.")
    parser.add_argument("--cols", type=int, default=100, help="PTY columns.")
    parser.add_argument("--term", default="xterm-256color", help="TERM value for the child process.")
    parser.add_argument("--events", type=Path, help="JSON input event file.")
    parser.add_argument("--send", action="append", default=[], help="Quick text event. Repeatable. Escape sequences like \\n are decoded.")
    parser.add_argument("--key", action="append", default=[], help="Quick key event. Repeatable; e.g. enter, esc, ctrl-c.")
    parser.add_argument("--send-delay-ms", type=int, default=250, help="Delay before each --send/--key event.")
    parser.add_argument("--timeout", type=float, default=30.0, help="Maximum recording duration in seconds.")
    parser.add_argument("--frame-limit", type=int, default=1000, help="Maximum number of frames to write.")
    parser.add_argument("--sandbox-home", action="store_true", help="Run with HOME inside the output directory.")
    parser.add_argument("command", nargs=argparse.REMAINDER, help="Command to run, usually after --.")
    return parser


def main() -> int:
    args = make_parser().parse_args()
    return run_session(args)


if __name__ == "__main__":
    raise SystemExit(main())
