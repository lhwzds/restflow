"""Event API placeholders."""

from dataclasses import dataclass
from typing import Any


@dataclass
class Event:
    type: str
    value: Any = None

    def is_terminal(self) -> bool:
        return self.type in {"done", "error"}
