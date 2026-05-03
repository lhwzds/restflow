#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
import urllib.error
import urllib.request
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import fail, int_input, ok, read_input, string_input


def main() -> None:
    payload = read_input()
    url = string_input(payload, "url")
    method = str(payload.get("method", "GET")).upper()
    timeout = int_input(payload, "timeout_seconds", default=30)
    headers = payload.get("headers", {})
    if not isinstance(headers, dict):
        fail("'headers' must be an object")

    body = payload.get("body")
    if isinstance(body, (dict, list)):
        data = json.dumps(body).encode("utf-8")
        headers = {"content-type": "application/json", **headers}
    elif isinstance(body, str):
        data = body.encode("utf-8")
    elif body is None:
        data = None
    else:
        fail("'body' must be a string, object, array, or null")

    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            ok(
                status=response.status,
                headers=dict(response.headers.items()),
                body=response.read().decode("utf-8", errors="replace"),
            )
    except urllib.error.HTTPError as exc:
        ok(
            status=exc.code,
            headers=dict(exc.headers.items()),
            body=exc.read().decode("utf-8", errors="replace"),
        )


if __name__ == "__main__":
    main()
