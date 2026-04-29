"""Skill catalog and turn planning API placeholders."""

from dataclasses import dataclass, field


@dataclass
class Skill:
    id: str
    name: str
    suggested_tools: list[str] = field(default_factory=list)


@dataclass
class TurnPlan:
    mentioned: list[str] = field(default_factory=list)
    activated: list[str] = field(default_factory=list)
    tools: list[str] = field(default_factory=list)
    issues: list[str] = field(default_factory=list)

