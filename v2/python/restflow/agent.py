"""Agent kernel API placeholders."""

from dataclasses import dataclass, field


@dataclass
class Agent:
    model: str
    skills: list[str] = field(default_factory=list)

