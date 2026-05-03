#!/usr/bin/env python3
from __future__ import annotations

import os
import shlex
import subprocess
import sys
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import fail, int_input, ok, read_input, string_input


def main() -> None:
    payload = read_input()
    audio_path = string_input(payload, "path")
    command_template = payload.get("command") or os.environ.get("TRANSCRIBE_COMMAND")
    if not isinstance(command_template, str) or not command_template:
        fail("missing 'command' or TRANSCRIBE_COMMAND. Use {path} as the audio path placeholder.")
    timeout = int_input(payload, "timeout_seconds", default=120, maximum=600)
    command = command_template.format(path=shlex.quote(audio_path))
    result = subprocess.run(
        command,
        shell=True,
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    ok(exit_code=result.returncode, text=result.stdout, stderr=result.stderr)


if __name__ == "__main__":
    main()
