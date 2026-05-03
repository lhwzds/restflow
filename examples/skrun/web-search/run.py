#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
import urllib.parse
import urllib.request
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import int_input, ok, read_input, require_env, string_input


def main() -> None:
    payload = read_input()
    query = string_input(payload, "query")
    count = int_input(payload, "count", default=5, maximum=20)
    api_key = require_env("BRAVE_SEARCH_API_KEY")
    params = urllib.parse.urlencode({"q": query, "count": count})
    request = urllib.request.Request(
        f"https://api.search.brave.com/res/v1/web/search?{params}",
        headers={
            "accept": "application/json",
            "x-subscription-token": api_key,
            "user-agent": "restflow-skrun-web-search/0.1",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        data = json.loads(response.read().decode("utf-8"))
    results = [
        {
            "title": item.get("title"),
            "url": item.get("url"),
            "description": item.get("description"),
        }
        for item in data.get("web", {}).get("results", [])
    ]
    ok(query=query, results=results)


if __name__ == "__main__":
    main()
