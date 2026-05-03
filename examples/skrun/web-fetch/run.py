#!/usr/bin/env python3
from __future__ import annotations

import sys
import urllib.request
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import int_input, ok, read_input, string_input


def main() -> None:
    payload = read_input()
    url = string_input(payload, "url")
    timeout = int_input(payload, "timeout_seconds", default=30)
    max_bytes = int_input(payload, "max_bytes", default=1_000_000, maximum=10_000_000)
    request = urllib.request.Request(url, headers={"user-agent": "restflow-skrun-web-fetch/0.1"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        body = response.read(max_bytes).decode("utf-8", errors="replace")
    ok(url=url, text=body, truncated=len(body.encode("utf-8")) >= max_bytes)


if __name__ == "__main__":
    main()
