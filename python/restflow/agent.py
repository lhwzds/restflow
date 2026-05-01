"""Agent-facing data models for skill-aware execution planning."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Agent:
    model: str
    skills: list[str] = field(default_factory=list)
