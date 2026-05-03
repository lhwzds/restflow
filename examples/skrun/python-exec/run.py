#!/usr/bin/env python3
from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import int_input, ok, read_input, string_input


def main() -> None:
    payload = read_input()
    code = string_input(payload, "code")
    timeout = int_input(payload, "timeout_seconds", default=30, maximum=120)
    cwd = payload.get("cwd")
    if cwd is not None and not isinstance(cwd, str):
        from common import fail

        fail("'cwd' must be a string when provided")

    with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False, encoding="utf-8") as handle:
        handle.write(code)
        script_path = handle.name

    try:
        result = subprocess.run(
            [sys.executable, script_path],
            cwd=cwd,
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
        ok(
            exit_code=result.returncode,
            stdout=result.stdout,
            stderr=result.stderr,
        )
    finally:
        Path(script_path).unlink(missing_ok=True)


if __name__ == "__main__":
    main()
