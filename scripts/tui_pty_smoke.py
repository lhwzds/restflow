#!/usr/bin/env python3
"""PTY smoke for RestFlow's terminal-stateful TUI path.

This intentionally avoids model execution. It verifies that the current checkout
can build, enter the TUI on a pseudo terminal, render the composer, run a local
slash-command error through the real redraw path, and exit cleanly.
"""

from __future__ import annotations

import os
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

if os.name == "nt":
    print("Skipping TUI PTY smoke on Windows: pseudo-terminal APIs are Unix-only.")
    raise SystemExit(0)

import fcntl
import pty
import termios


LOCAL_MESSAGE = "/help"
LOCAL_OVERLAY = "RestFlow terminal shell"
PROMPT = "Type your message or use /help"
SCROLL_REGION_MARKER = "\x1b[1;"
RESET_SCROLL_REGION_MARKER = "\x1b[r"
CURSOR_MOVE_RE = re.compile(r"\x1b\[(\d+);(\d+)H")
SCROLL_REGION_RE = re.compile(r"\x1b\[(\d+);(\d+)r")
ANSI_CSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


def read_until(fd: int, deadline: float, marker: str) -> str:
    chunks: list[bytes] = []
    while time.time() < deadline:
        readable, _, _ = select.select([fd], [], [], 0.05)
        if not readable:
            continue
        try:
            data = os.read(fd, 65536)
        except OSError:
            break
        if not data:
            break
        chunks.append(data)
        text = b"".join(chunks).decode("utf-8", errors="replace")
        if marker in text:
            return text
    return b"".join(chunks).decode("utf-8", errors="replace")


def read_available(fd: int, quiet_for: float = 0.15, timeout: float = 1.0) -> str:
    chunks: list[bytes] = []
    deadline = time.time() + timeout
    quiet_deadline = time.time() + quiet_for
    while time.time() < deadline:
        readable, _, _ = select.select([fd], [], [], 0.05)
        if not readable:
            if time.time() >= quiet_deadline:
                break
            continue
        try:
            data = os.read(fd, 65536)
        except OSError:
            break
        if not data:
            break
        chunks.append(data)
        quiet_deadline = time.time() + quiet_for
    return b"".join(chunks).decode("utf-8", errors="replace")


def write_slow(fd: int, text: str, delay: float = 0.005) -> None:
    for ch in text:
        os.write(fd, ch.encode())
        time.sleep(delay)


def resolve_binary(repo: Path) -> Path:
    explicit_binary = os.environ.get("RESTFLOW_BIN")
    if explicit_binary:
        binary = Path(explicit_binary)
        if (
            "debug" not in binary.parts
            and os.environ.get("RESTFLOW_ALLOW_RELEASE_FIXTURE_SMOKE") != "1"
        ):
            sys.stderr.write(
                "RESTFLOW_BIN fixture smoke requires a debug binary; "
                "set RESTFLOW_ALLOW_RELEASE_FIXTURE_SMOKE=1 to override.\n"
            )
            raise SystemExit(1)
        return binary
    subprocess.run(
        ["cargo", "build", "--package", "cli", "--bin", "restflow"],
        cwd=repo,
        check=True,
    )
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", repo / "target"))
    if not target_dir.is_absolute():
        target_dir = repo / target_dir
    return target_dir / "debug" / "restflow"


def launch_tui(
    binary: Path,
    repo: Path,
    fixture: str | None = None,
    rows: int = 24,
    cols: int = 100,
):
    restflow_dir = tempfile.TemporaryDirectory(prefix="restflow-tui-pty-")
    env = os.environ.copy()
    env["RESTFLOW_DIR"] = restflow_dir.name
    env.setdefault("TERM", "xterm-256color")
    env.setdefault("NO_COLOR", "1")
    if fixture:
        env["RESTFLOW_TUI_PTY_FIXTURE"] = fixture

    master = None
    slave = None
    try:
        master, slave = pty.openpty()
        size = struct.pack("HHHH", rows, cols, 0, 0)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, size)
        proc = subprocess.Popen(
            [str(binary)],
            cwd=repo,
            env=env,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            start_new_session=True,
        )
        os.close(slave)
        return proc, master, restflow_dir
    except Exception:
        for fd in (slave, master):
            if fd is not None:
                try:
                    os.close(fd)
                except OSError:
                    pass
        restflow_dir.cleanup()
        raise


def terminate_tui(proc: subprocess.Popen, master: int, restflow_dir) -> None:
    try:
        if proc.poll() is None:
            try:
                os.killpg(proc.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                proc.wait(timeout=5)
    finally:
        try:
            os.close(master)
        except OSError:
            pass
        restflow_dir.cleanup()


def wait_for_clean_exit(proc: subprocess.Popen) -> int:
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(timeout=5)
        sys.stderr.write("TUI did not exit after Ctrl-C\n")
        return 1
    return 0 if proc.returncode in (0, -signal.SIGINT) else proc.returncode


def has_native_scrollback_insert(output: str) -> bool:
    return SCROLL_REGION_RE.search(output) is not None and RESET_SCROLL_REGION_MARKER in output


def native_scrollback_insert_segments(output: str) -> list[str]:
    segments: list[str] = []
    offset = 0
    while True:
        match = SCROLL_REGION_RE.search(output, offset)
        if match is None:
            break
        start = match.start()
        end = output.find(RESET_SCROLL_REGION_MARKER, start)
        if end < 0:
            break
        end += len(RESET_SCROLL_REGION_MARKER)
        segments.append(output[start:end])
        offset = end
    return segments


def native_scrollback_insert_text(output: str) -> str:
    return "\n".join(native_scrollback_insert_segments(output))


def latest_native_scrollback_insert_text(output: str) -> str:
    segments = native_scrollback_insert_segments(output)
    return segments[-1] if segments else ""


def require_native_scrollback_markers(
    output: str, markers: list[str], failure_context: str
) -> bool:
    segment_text = native_scrollback_insert_text(output)
    missing = [marker for marker in markers if marker not in segment_text]
    if not missing:
        return True
    sys.stderr.write(output[-4000:])
    sys.stderr.write(
        f"\n{failure_context}: missing from native scrollback insert segments: {', '.join(missing)}\n"
    )
    return False


def require_ordered_markers(text: str, markers: list[str], failure_context: str) -> bool:
    offset = 0
    for marker in markers:
        index = text.find(marker, offset)
        if index < 0:
            sys.stderr.write(text[-4000:])
            sys.stderr.write(f"\n{failure_context}: missing ordered marker: {marker}\n")
            return False
        offset = index + len(marker)
    return True


def require_unique_ordered_markers(
    text: str, markers: list[str], failure_context: str
) -> bool:
    if not require_unique_markers(text, markers, failure_context):
        return False
    return require_ordered_markers(text, markers, failure_context)


def require_unique_markers(text: str, markers: list[str], failure_context: str) -> bool:
    duplicated = [marker for marker in markers if text.count(marker) > 1]
    if not duplicated:
        return True
    sys.stderr.write(text[-4000:])
    sys.stderr.write(
        f"\n{failure_context}: duplicated marker(s): {', '.join(duplicated)}\n"
    )
    return False


def prompt_entered_native_scrollback(output: str) -> bool:
    return any(PROMPT in segment for segment in native_scrollback_insert_segments(output))


def ui_chrome_entered_native_scrollback(output: str) -> bool:
    fragments = [
        PROMPT,
        "Type your message",
        "use /help",
        "┌",
        "└",
        "Plan ·",
        "Default Assistant ·",
    ]
    return any(
        fragment in segment
        for segment in native_scrollback_insert_segments(output)
        for fragment in fragments
    )


def prompt_rows(output: str) -> list[int]:
    rows: list[int] = []
    for prompt_match in re.finditer(re.escape(PROMPT), output):
        moves = list(CURSOR_MOVE_RE.finditer(output[: prompt_match.start()]))
        if moves:
            rows.append(int(moves[-1].group(1)))
    return rows


def visible_prompt_count(screen: str) -> int:
    return screen.count(PROMPT)


def scroll_region_end_rows(output: str) -> list[int]:
    return [int(match.group(2)) for match in SCROLL_REGION_RE.finditer(output)]


def scroll_region_reaches_composer(output: str) -> bool:
    rows = prompt_rows(output)
    if not rows:
        return False
    composer_top = max(1, min(rows) - 1)
    return any(end_row >= composer_top for end_row in scroll_region_end_rows(output))


def final_visible_screen(output: str, rows: int, cols: int) -> str:
    screen = [[" " for _ in range(cols)] for _ in range(rows)]
    row = 0
    col = 0
    scroll_top = 0
    scroll_bottom = rows - 1
    index = 0

    def scroll_region_up() -> None:
        nonlocal screen
        if scroll_top > scroll_bottom:
            return
        for region_row in range(scroll_top, scroll_bottom):
            screen[region_row] = screen[region_row + 1]
        screen[scroll_bottom] = [" " for _ in range(cols)]

    def line_feed() -> None:
        nonlocal row
        if scroll_top <= row <= scroll_bottom and row == scroll_bottom:
            scroll_region_up()
        else:
            row = min(rows - 1, row + 1)

    while index < len(output):
        char = output[index]
        if char == "\x1b" and index + 1 < len(output) and output[index + 1] == "[":
            end = index + 2
            while end < len(output) and not ("@" <= output[end] <= "~"):
                end += 1
            if end >= len(output):
                break
            command = output[end]
            params = output[index + 2 : end]
            parts = [part for part in re.split(r"[;?]", params) if part.isdigit()]
            if command in ("H", "f"):
                row = max(0, min(rows - 1, (int(parts[0]) if parts else 1) - 1))
                col = max(0, min(cols - 1, (int(parts[1]) if len(parts) > 1 else 1) - 1))
            elif command == "G":
                col = max(0, min(cols - 1, (int(parts[0]) if parts else 1) - 1))
            elif command == "K":
                screen[row][col:] = [" " for _ in range(cols - col)]
            elif command == "J" and (not parts or parts[-1] in ("2", "3")):
                screen = [[" " for _ in range(cols)] for _ in range(rows)]
                row = 0
                col = 0
            elif command == "r":
                if len(parts) >= 2:
                    top = int(parts[0]) - 1
                    bottom = int(parts[1]) - 1
                    scroll_top = max(0, min(rows - 1, top))
                    scroll_bottom = max(scroll_top, min(rows - 1, bottom))
                else:
                    scroll_top = 0
                    scroll_bottom = rows - 1
            index = end + 1
            continue
        if char == "\r":
            col = 0
        elif char == "\n":
            line_feed()
        elif char >= " ":
            if 0 <= row < rows and 0 <= col < cols:
                screen[row][col] = char
            col += 1
            if col >= cols:
                col = 0
                line_feed()
        index += 1
    return "\n".join("".join(line).rstrip() for line in screen)


def compact_screen_text(screen: str) -> str:
    return "".join(screen.split())


def compact_output_text(output: str) -> str:
    return "".join(ANSI_CSI_RE.sub("", output).split())


def read_until_prompt(fd: int, deadline: float) -> str:
    output = read_until(fd, deadline, PROMPT)
    if PROMPT not in output:
        output += read_available(fd, quiet_for=0.3, timeout=2)
    return output


def run_help_smoke(binary: Path, repo: Path) -> int:
    proc, master, restflow_dir = launch_tui(binary, repo)
    try:
        output = read_until_prompt(master, time.time() + 15)
        if PROMPT not in output:
            sys.stderr.write(output[-2000:])
            sys.stderr.write("\ncomposer prompt not rendered\n")
            return 1

        write_slow(master, LOCAL_MESSAGE)
        os.write(master, b"\r")
        command_output = read_until(master, time.time() + 8, LOCAL_OVERLAY)
        if LOCAL_OVERLAY not in command_output:
            sys.stderr.write((output + command_output)[-3000:])
            sys.stderr.write("\nlocal help overlay did not render\n")
            return 1
        if PROMPT not in command_output:
            redraw_output = read_until(master, time.time() + 3, PROMPT)
            if PROMPT not in redraw_output:
                sys.stderr.write((output + command_output + redraw_output)[-3000:])
                sys.stderr.write("\ncomposer prompt not restored after redraw\n")
                return 1

        read_available(master)
        os.write(master, b"\x1b")
        close_output = read_until(master, time.time() + 5, PROMPT)
        if PROMPT not in close_output:
            sys.stderr.write(close_output[-3000:])
            sys.stderr.write("\ncomposer prompt not restored after closing overlay\n")
            return 1
        read_available(master)
        os.write(master, b"\x03")
        return wait_for_clean_exit(proc)
    finally:
        terminate_tui(proc, master, restflow_dir)


def run_long_active_smoke(
    binary: Path,
    repo: Path,
    rows: int = 24,
    cols: int = 100,
    expect_native_scrollback: bool = True,
) -> int:
    proc, master, restflow_dir = launch_tui(
        binary, repo, "long-active-turn", rows=rows, cols=cols
    )
    try:
        output = read_until(master, time.time() + 8, "assistant-tail-final")
        if "assistant-line-00" not in output or "tool-smoke-output" not in output:
            output += read_available(master, timeout=2)
        if "assistant-line-00" not in output:
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nlong active assistant prefix did not render\n")
            return 1
        if "tool-smoke-output" not in output:
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted tool output did not render\n")
            return 1
        if "assistant-tail-final" not in output:
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nactive assistant tail did not render\n")
            return 1
        output += read_available(master)
        if expect_native_scrollback and not has_native_scrollback_insert(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nlong active fixture did not use native scrollback insertion\n")
            return 1
        if expect_native_scrollback and not require_native_scrollback_markers(
            output,
            ["assistant-line-00", "tool-smoke-output"],
            "long active fixture did not commit expected content",
        ):
            return 1
        if expect_native_scrollback and not require_ordered_markers(
            native_scrollback_insert_text(output),
            ["assistant-line-00", "tool-smoke-output"],
            "long active fixture committed content out of order",
        ):
            return 1
        if not prompt_rows(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncomposer prompt row was not observed during active fixture\n")
            return 1
        final_screen = final_visible_screen(output, rows, cols)
        if "assistant-tail-final" not in compact_screen_text(final_screen):
            sys.stderr.write(final_screen)
            sys.stderr.write("\nactive assistant tail was not present in final visible screen\n")
            return 1
        if PROMPT not in final_screen:
            sys.stderr.write(final_screen)
            sys.stderr.write("\ncomposer prompt was not present in final visible screen\n")
            return 1
        if visible_prompt_count(final_screen) != 1:
            sys.stderr.write(final_screen)
            sys.stderr.write("\nexpected exactly one composer prompt in final visible screen\n")
            return 1
        if ui_chrome_entered_native_scrollback(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncomposer or footer chrome entered native scrollback during fixture\n")
            return 1
        if scroll_region_reaches_composer(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nscroll region reached the composer during fixture\n")
            return 1
        os.write(master, b"\x03")
        canceled_output = read_until(master, time.time() + 5, "Canceled current response")
        if "Canceled current response" not in canceled_output:
            sys.stderr.write(canceled_output[-3000:])
            sys.stderr.write("\nactive fixture did not cancel after first Ctrl-C\n")
            return 1
        read_available(master)
        os.write(master, b"\x03")
        return wait_for_clean_exit(proc)
    finally:
        terminate_tui(proc, master, restflow_dir)


def run_long_single_line_smoke(
    binary: Path,
    repo: Path,
    rows: int = 12,
    cols: int = 40,
    expect_native_scrollback: bool = True,
) -> int:
    proc, master, restflow_dir = launch_tui(
        binary, repo, "long-active-single-line", rows=rows, cols=cols
    )
    try:
        output = read_until(master, time.time() + 8, "assistant-tail-final")
        output += read_available(master, timeout=1)
        if expect_native_scrollback and "assistant-single-line-prefix" not in output:
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nlong single-line assistant prefix did not render\n")
            return 1
        if "assistant-tail-final" not in compact_output_text(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nlong single-line assistant tail did not render\n")
            return 1
        if expect_native_scrollback and not has_native_scrollback_insert(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nlong single-line fixture did not use native scrollback insertion\n")
            return 1
        if expect_native_scrollback and not require_native_scrollback_markers(
            output,
            ["assistant-single-line-prefix"],
            "long single-line fixture did not commit assistant prefix",
        ):
            return 1
        final_screen = final_visible_screen(output, rows, cols)
        if "tail-final" not in compact_screen_text(final_screen):
            sys.stderr.write(final_screen)
            sys.stderr.write("\nlong single-line tail was not present in final visible screen\n")
            return 1
        if PROMPT not in final_screen:
            sys.stderr.write(final_screen)
            sys.stderr.write("\ncomposer prompt was not present after long single-line fixture\n")
            return 1
        if visible_prompt_count(final_screen) != 1:
            sys.stderr.write(final_screen)
            sys.stderr.write(
                "\nexpected exactly one composer prompt after long single-line fixture\n"
            )
            return 1
        if ui_chrome_entered_native_scrollback(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write(
                "\ncomposer or footer chrome entered native scrollback during long single-line fixture\n"
            )
            return 1
        if scroll_region_reaches_composer(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nscroll region reached the composer during long single-line fixture\n")
            return 1
        os.write(master, b"\x03")
        canceled_output = read_until(master, time.time() + 5, "Canceled current response")
        if "Canceled current response" not in canceled_output:
            sys.stderr.write(canceled_output[-3000:])
            sys.stderr.write("\nlong single-line fixture did not cancel after first Ctrl-C\n")
            return 1
        read_available(master)
        os.write(master, b"\x03")
        return wait_for_clean_exit(proc)
    finally:
        terminate_tui(proc, master, restflow_dir)


def run_completed_turn_smoke(
    binary: Path,
    repo: Path,
    rows: int = 24,
    cols: int = 100,
    fixture: str = "completed-turn",
) -> int:
    proc, master, restflow_dir = launch_tui(
        binary, repo, fixture, rows=rows, cols=cols
    )
    try:
        output = read_until(master, time.time() + 8, "assistant-tail-final")
        output += read_available(master, timeout=1)
        if "assistant-line-00" not in output:
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted fixture assistant prefix did not reach history\n")
            return 1
        if "tool-smoke-output" not in output:
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted fixture tool output did not reach history\n")
            return 1
        if "assistant-tail-final" not in output:
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted fixture assistant tail did not reach history\n")
            return 1
        if not has_native_scrollback_insert(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted fixture did not use native scrollback insertion\n")
            return 1
        if not require_native_scrollback_markers(
            output,
            ["assistant-line-00", "tool-smoke-output", "assistant-tail-final"],
            "completed fixture did not commit expected content",
        ):
            return 1
        if not require_unique_ordered_markers(
            native_scrollback_insert_text(output),
            ["assistant-line-00", "tool-smoke-output", "assistant-tail-final"],
            "completed fixture committed content out of order",
        ):
            return 1
        if latest_native_scrollback_insert_text(output).count("assistant-tail-final") > 1:
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted fixture duplicated assistant tail in latest native history\n")
            return 1
        if "typing" in compact_screen_text(native_scrollback_insert_text(output)):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted fixture leaked active typing chrome into native history\n")
            return 1
        if not prompt_rows(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncomposer prompt row was not observed after completed fixture\n")
            return 1
        final_screen = final_visible_screen(output, rows, cols)
        final_text = compact_screen_text(final_screen)
        if "assistant-tail-final" not in final_text:
            sys.stderr.write(final_screen)
            sys.stderr.write("\ncompleted fixture assistant tail disappeared from final screen\n")
            return 1
        if "typing" in final_text:
            sys.stderr.write(final_screen)
            sys.stderr.write("\ncompleted fixture leaked active typing chrome into final screen\n")
            return 1
        if PROMPT not in final_screen:
            sys.stderr.write(final_screen)
            sys.stderr.write("\ncomposer prompt was not present after completed fixture\n")
            return 1
        if visible_prompt_count(final_screen) != 1:
            sys.stderr.write(final_screen)
            sys.stderr.write("\nexpected exactly one composer prompt after completed fixture\n")
            return 1
        if ui_chrome_entered_native_scrollback(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncomposer or footer chrome entered native scrollback after completed fixture\n")
            return 1
        if scroll_region_reaches_composer(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nscroll region reached the composer after completed fixture\n")
            return 1
        os.write(master, b"\x03")
        return wait_for_clean_exit(proc)
    finally:
        terminate_tui(proc, master, restflow_dir)


def run_completed_long_assistant_smoke(
    binary: Path,
    repo: Path,
    rows: int = 24,
    cols: int = 100,
    fixture: str = "completed-long-assistant",
) -> int:
    proc, master, restflow_dir = launch_tui(binary, repo, fixture, rows=rows, cols=cols)
    try:
        output = read_until(master, time.time() + 8, "assistant-tail-final")
        output += read_available(master, timeout=1)
        if "assistant-line-00" not in output or "assistant-tail-final" not in output:
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted long assistant fixture did not render expected markers\n")
            return 1
        if not has_native_scrollback_insert(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted long assistant fixture did not use native scrollback insertion\n")
            return 1
        if not require_unique_ordered_markers(
            native_scrollback_insert_text(output),
            ["assistant-line-00", "assistant-tail-final"],
            "completed long assistant committed content out of order",
        ):
            return 1
        latest_history = latest_native_scrollback_insert_text(output)
        if latest_history.count("Default Assistant") > 1 or not require_unique_markers(
            latest_history,
            ["assistant-line-00", "assistant-tail-final"],
            "completed long assistant duplicated content in latest native history",
        ):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted long assistant rendered as duplicate assistant blocks\n")
            return 1
        if "typing" in compact_screen_text(native_scrollback_insert_text(output)):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncompleted long assistant leaked active typing chrome into native history\n")
            return 1
        final_screen = final_visible_screen(output, rows, cols)
        final_text = compact_screen_text(final_screen)
        if "assistant-tail-final" not in final_text:
            sys.stderr.write(final_screen)
            sys.stderr.write("\ncompleted long assistant tail disappeared from final screen\n")
            return 1
        if "typing" in final_text:
            sys.stderr.write(final_screen)
            sys.stderr.write("\ncompleted long assistant leaked active typing chrome into final screen\n")
            return 1
        if final_text.count("Default Assistant") > 1:
            sys.stderr.write(final_screen)
            sys.stderr.write(
                "\ncompleted long assistant rendered duplicate assistant blocks in final screen\n"
            )
            return 1
        if visible_prompt_count(final_screen) != 1:
            sys.stderr.write(final_screen)
            sys.stderr.write("\nexpected exactly one composer prompt after completed long assistant fixture\n")
            return 1
        if ui_chrome_entered_native_scrollback(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\ncomposer or footer chrome entered native scrollback after completed long assistant fixture\n")
            return 1
        if scroll_region_reaches_composer(output):
            sys.stderr.write(output[-4000:])
            sys.stderr.write("\nscroll region reached the composer after completed long assistant fixture\n")
            return 1
        os.write(master, b"\x03")
        return wait_for_clean_exit(proc)
    finally:
        terminate_tui(proc, master, restflow_dir)


def main() -> int:
    repo = Path(__file__).resolve().parents[1]
    binary = resolve_binary(repo)
    if not binary.exists():
        sys.stderr.write(f"restflow binary not found: {binary}\n")
        return 1

    help_status = run_help_smoke(binary, repo)
    if help_status != 0:
        return help_status
    active_status = run_long_active_smoke(binary, repo)
    if active_status != 0:
        return active_status
    short_active_status = run_long_active_smoke(binary, repo, rows=12)
    if short_active_status != 0:
        return short_active_status
    tiny_active_status = run_long_active_smoke(
        binary, repo, rows=6, expect_native_scrollback=False
    )
    if tiny_active_status != 0:
        return tiny_active_status
    single_line_status = run_long_single_line_smoke(binary, repo)
    if single_line_status != 0:
        return single_line_status
    short_single_line_status = run_long_single_line_smoke(
        binary, repo, rows=6, expect_native_scrollback=False
    )
    if short_single_line_status != 0:
        return short_single_line_status
    completed_status = run_completed_turn_smoke(binary, repo)
    if completed_status != 0:
        return completed_status
    completed_refresh_status = run_completed_turn_smoke(
        binary, repo, fixture="completed-tool-turn-refresh"
    )
    if completed_refresh_status != 0:
        return completed_refresh_status
    tall_completed_refresh_status = run_completed_turn_smoke(
        binary,
        repo,
        rows=50,
        fixture="completed-tool-turn-refresh",
    )
    if tall_completed_refresh_status != 0:
        return tall_completed_refresh_status
    short_completed_status = run_completed_turn_smoke(binary, repo, rows=12)
    if short_completed_status != 0:
        return short_completed_status
    long_completed_status = run_completed_long_assistant_smoke(binary, repo)
    if long_completed_status != 0:
        return long_completed_status
    long_completed_refresh_status = run_completed_long_assistant_smoke(
        binary, repo, fixture="completed-long-assistant-refresh"
    )
    if long_completed_refresh_status != 0:
        return long_completed_refresh_status
    return run_completed_long_assistant_smoke(
        binary,
        repo,
        rows=50,
        fixture="completed-long-assistant-refresh",
    )


if __name__ == "__main__":
    raise SystemExit(main())
