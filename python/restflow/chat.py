"""Chat session and message data models."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Message:
    role: str
    text: str


@dataclass
class Session:
    id: str
    name: str | None = None
    agent_id: str | None = None
    provider: str | None = None
    model: str | None = None
    source: str | None = None
    created_at: str | None = None
    updated_at: str | None = None
    archived_at: str | None = None
    messages: list[Message] = field(default_factory=list)

    def push(self, message: Message) -> None:
        self.messages.append(message)
