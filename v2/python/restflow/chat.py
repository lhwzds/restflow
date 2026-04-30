"""Chat session API placeholders."""

from dataclasses import dataclass, field


@dataclass
class Message:
    role: str
    text: str


@dataclass
class Session:
    id: str
    messages: list[Message] = field(default_factory=list)

    def push(self, message: Message) -> None:
        self.messages.append(message)
