#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
import urllib.request
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import ok, read_input, string_input


def main() -> None:
    payload = read_input()
    webhook_url = payload.get("webhook_url") or os.environ.get("DISCORD_WEBHOOK_URL")
    if not isinstance(webhook_url, str) or not webhook_url:
        from common import fail

        fail("missing 'webhook_url' or DISCORD_WEBHOOK_URL")
    content = string_input(payload, "content")
    data = json.dumps({"content": content}).encode("utf-8")
    request = urllib.request.Request(
        webhook_url,
        data=data,
        headers={"content-type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        ok(status=response.status)


if __name__ == "__main__":
    main()
