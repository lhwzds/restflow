#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
import urllib.request
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import ok, read_input, require_env, string_input


def main() -> None:
    payload = read_input()
    bot_token = require_env("TELEGRAM_BOT_TOKEN")
    chat_id = string_input(payload, "chat_id")
    text = string_input(payload, "text")
    data = json.dumps({"chat_id": chat_id, "text": text}).encode("utf-8")
    request = urllib.request.Request(
        f"https://api.telegram.org/bot{bot_token}/sendMessage",
        data=data,
        headers={"content-type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        result = json.loads(response.read().decode("utf-8"))
    ok(result=result)


if __name__ == "__main__":
    main()
