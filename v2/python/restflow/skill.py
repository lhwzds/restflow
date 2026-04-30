"""Skill catalog and AI context API placeholders."""

from dataclasses import dataclass, field


@dataclass
class Skill:
    id: str
    name: str
    source: str = "user"
    read_only: bool = False
    description: str | None = None
    content: str = ""
    suggested_tools: list[str] = field(default_factory=list)


@dataclass
class SkillContext:
    assigned: list[Skill] = field(default_factory=list)
    mentioned: list[Skill] = field(default_factory=list)
    issues: list[str] = field(default_factory=list)
