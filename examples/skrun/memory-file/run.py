#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
import time
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import fail, int_input, ok, read_input, string_input


def memory_path(payload: dict[str, object]) -> Path:
    raw_path = payload.get("path") or os.environ.get("RESTFLOW_MEMORY_FILE")
    if not isinstance(raw_path, str) or not raw_path:
        raw_path = str(Path.home() / ".restflow" / "skrun-memory.jsonl")
    path = Path(raw_path).expanduser()
    path.parent.mkdir(parents=True, exist_ok=True)
    return path


def read_entries(path: Path) -> list[dict[str, object]]:
    if not path.exists():
        return []
    entries: list[dict[str, object]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            entries.append(json.loads(line))
    return entries


def main() -> None:
    payload = read_input()
    action = string_input(payload, "action")
    path = memory_path(payload)

    if action == "add":
        text = string_input(payload, "text")
        entry = {
            "id": str(uuid.uuid4()),
            "created_at": int(time.time() * 1000),
            "text": text,
            "tags": payload.get("tags", []),
        }
        with path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(entry, ensure_ascii=False) + "\n")
        ok(entry=entry, path=str(path))
        return

    if action == "search":
        query = string_input(payload, "query").lower()
        limit = int_input(payload, "limit", default=10, maximum=100)
        matches = [
            entry
            for entry in read_entries(path)
            if query in str(entry.get("text", "")).lower()
        ][:limit]
        ok(matches=matches, count=len(matches), path=str(path))
        return

    if action == "list":
        limit = int_input(payload, "limit", default=20, maximum=100)
        entries = read_entries(path)[-limit:]
        ok(entries=entries, count=len(entries), path=str(path))
        return

    fail("unsupported action. Use add, search, or list.")


if __name__ == "__main__":
    main()
