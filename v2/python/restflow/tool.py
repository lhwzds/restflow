"""Tool registry API placeholders."""

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any


ToolFn = Callable[[dict[str, Any]], dict[str, Any]]


@dataclass
class Registry:
    tools: dict[str, ToolFn] = field(default_factory=dict)

    def add(self, name: str, tool: ToolFn) -> None:
        self.tools[name] = tool

