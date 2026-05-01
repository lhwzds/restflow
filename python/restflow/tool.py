"""Callable tool registry data models."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any


ToolFn = Callable[[dict[str, Any]], dict[str, Any]]


@dataclass
class ToolSpec:
    name: str
    description: str | None = None
    input_schema: dict[str, Any] = field(default_factory=lambda: {"type": "object"})


@dataclass
class ToolCall:
    id: str
    name: str
    input: dict[str, Any]


@dataclass
class ToolOutput:
    call_id: str
    value: dict[str, Any]


@dataclass
class Registry:
    tools: dict[str, ToolFn] = field(default_factory=dict)

    def add(self, name: str, tool: ToolFn) -> None:
        self.tools[name] = tool

    def names(self) -> list[str]:
        return sorted(self.tools)

    def call(self, call: ToolCall) -> ToolOutput:
        return ToolOutput(call_id=call.id, value=self.tools[call.name](call.input))
