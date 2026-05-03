#!/usr/bin/env python3
"""Shared helpers for RestFlow skrun example skills."""

from __future__ import annotations

import json
import os
import sys
from typing import Any


def read_input() -> dict[str, Any]:
    if len(sys.argv) < 2 or not sys.argv[1].strip():
        return {}
    try:
        payload = json.loads(sys.argv[1])
    except json.JSONDecodeError as exc:
        fail(f"invalid JSON input: {exc}")
    if not isinstance(payload, dict):
        fail("skill input must be a JSON object")
    return payload


def emit(payload: dict[str, Any]) -> None:
    print(json.dumps(payload, ensure_ascii=False))


def ok(**payload: Any) -> None:
    emit({"ok": True, **payload})


def fail(message: str, **payload: Any) -> None:
    emit({"ok": False, "error": message, **payload})
    raise SystemExit(1)


def require_env(name: str) -> str:
    value = os.environ.get(name)
    if not value:
        fail(f"missing required environment variable: {name}")
    return value


def string_input(payload: dict[str, Any], name: str, *, default: str | None = None) -> str:
    value = payload.get(name, default)
    if not isinstance(value, str) or not value:
        fail(f"'{name}' must be a non-empty string")
    return value


def int_input(
    payload: dict[str, Any],
    name: str,
    *,
    default: int,
    minimum: int = 1,
    maximum: int = 300,
) -> int:
    value = payload.get(name, default)
    if isinstance(value, bool):
        fail(f"'{name}' must be an integer")
    try:
        number = int(value)
    except (TypeError, ValueError):
        fail(f"'{name}' must be an integer")
    if number < minimum or number > maximum:
        fail(f"'{name}' must be between {minimum} and {maximum}")
    return number
