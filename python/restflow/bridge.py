"""Bridge DTOs for moving legacy boundary data into the core."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any

from .auth import Profile, SecretRef
from .chat import Message, Session
from .core import CoreCommand, CoreSnapshot
from .model import Model, ModelSpec, Provider
from .run import Run, Task
from .skill import Skill
from .tool import ToolCall, ToolSpec


class BridgeSkillSource(str, Enum):
    SYSTEM = "system"
    USER = "user"
    EXTERNAL = "external"


class BridgeRole(str, Enum):
    USER = "user"
    ASSISTANT = "assistant"
    TOOL = "tool"
    SYSTEM = "system"


class BridgeStatus(str, Enum):
    PENDING = "pending"
    RUNNING = "running"
    DONE = "done"
    FAILED = "failed"
    CANCELED = "canceled"


@dataclass
class BridgeModelRef:
    provider: str
    model: str

    def to_model(self) -> Model:
        return Model(provider=Provider(id=self.provider), id=self.model)


@dataclass
class BridgeModelSpec:
    provider: str
    model: str
    name: str
    description: str | None = None
    client_model: str | None = None
    client_kind: str | None = None
    base_url: str | None = None

    def to_model_spec(self) -> ModelSpec:
        return ModelSpec(
            model=Model(provider=Provider(id=self.provider), id=self.model),
            name=self.name,
            description=self.description,
            client_model=self.client_model,
            client_kind=self.client_kind,
            base_url=self.base_url,
        )


@dataclass
class BridgeSkill:
    id: str
    name: str
    source: str | BridgeSkillSource = BridgeSkillSource.USER
    read_only: bool = False
    description: str | None = None
    content: str = ""
    suggested_tools: list[str] = field(default_factory=list)
    source_ref: str | None = None

    def to_skill(self) -> Skill:
        return Skill(
            id=self.id,
            name=self.name,
            source=_enum_value(self.source),
            source_ref=self.source_ref,
            read_only=self.read_only,
            description=self.description,
            content=self.content,
            suggested_tools=list(self.suggested_tools),
        )


@dataclass
class BridgeMessage:
    role: str | BridgeRole
    text: str

    def to_message(self) -> Message:
        return Message(role=_enum_value(self.role), text=self.text)


@dataclass
class BridgeSession:
    id: str
    name: str | None = None
    agent_id: str | None = None
    provider: str | None = None
    model: str | None = None
    source: str | None = None
    created_at: str | None = None
    updated_at: str | None = None
    archived_at: str | None = None
    messages: list[BridgeMessage] = field(default_factory=list)

    def to_session(self) -> Session:
        return Session(
            id=self.id,
            name=self.name,
            agent_id=self.agent_id,
            provider=self.provider,
            model=self.model,
            source=self.source,
            created_at=self.created_at,
            updated_at=self.updated_at,
            archived_at=self.archived_at,
            messages=[message.to_message() for message in self.messages],
        )


@dataclass
class BridgeTask:
    id: str
    title: str
    input: str | None = None
    agent_id: str | None = None
    session_id: str | None = None
    status: str | None = None
    schedule: str | None = None
    created_at: str | None = None
    updated_at: str | None = None
    error: str | None = None

    def to_task(self) -> Task:
        return Task(
            id=self.id,
            title=self.title,
            input=self.input,
            agent_id=self.agent_id,
            session_id=self.session_id,
            status=self.status,
            schedule=self.schedule,
            created_at=self.created_at,
            updated_at=self.updated_at,
            error=self.error,
        )


@dataclass
class BridgeRun:
    id: str
    task_id: str
    status: str | BridgeStatus
    raw_status: str | None = None
    session_id: str | None = None
    execution_id: str | None = None
    checkpoint_id: str | None = None
    error: str | None = None
    started_at: str | None = None
    updated_at: str | None = None
    ended_at: str | None = None

    def to_run(self) -> Run:
        return Run(
            id=self.id,
            task_id=self.task_id,
            status=_enum_value(self.status),
            raw_status=self.raw_status,
            session_id=self.session_id,
            execution_id=self.execution_id,
            checkpoint_id=self.checkpoint_id,
            error=self.error,
            started_at=self.started_at,
            updated_at=self.updated_at,
            ended_at=self.ended_at,
        )


def _enum_value(value: str | Enum) -> str:
    if isinstance(value, Enum):
        return str(value.value)
    return value


@dataclass
class BridgeProfile:
    provider: str
    secret_key: str

    def to_profile(self) -> Profile:
        return Profile(provider=Provider(id=self.provider), secret=SecretRef(key=self.secret_key))


@dataclass
class BridgeToolSpec:
    name: str
    description: str | None = None
    input_schema: dict[str, Any] = field(default_factory=lambda: {"type": "object"})

    def to_tool_spec(self) -> ToolSpec:
        return ToolSpec(
            name=self.name,
            description=self.description,
            input_schema=dict(self.input_schema),
        )


@dataclass
class BridgeToolCall:
    id: str
    name: str
    input: dict[str, Any] = field(default_factory=dict)

    def to_tool_call(self) -> ToolCall:
        return ToolCall(id=self.id, name=self.name, input=dict(self.input))


@dataclass
class BridgeChatTurn:
    session_id: str
    message: str
    assigned_skills: list[str] = field(default_factory=list)

    def to_core_command(self) -> CoreCommand:
        return CoreCommand(
            type="chat_turn",
            payload={
                "session_id": self.session_id,
                "message": self.message,
                "assigned_skills": list(self.assigned_skills),
            },
        )


@dataclass
class BridgeRunTask:
    run_id: str
    task: BridgeTask
    message: str
    assigned_skills: list[str] = field(default_factory=list)

    def to_core_command(self) -> CoreCommand:
        return CoreCommand(
            type="run_task",
            payload={
                "run_id": self.run_id,
                "task": self.task.to_task(),
                "message": self.message,
                "assigned_skills": list(self.assigned_skills),
            },
        )


@dataclass
class BridgeSnapshot:
    current_model: BridgeModelRef
    models: list[BridgeModelSpec] = field(default_factory=list)
    skills: list[BridgeSkill] = field(default_factory=list)
    sessions: list[BridgeSession] = field(default_factory=list)
    tasks: list[BridgeTask] = field(default_factory=list)
    runs: list[BridgeRun] = field(default_factory=list)
    profiles: list[BridgeProfile] = field(default_factory=list)
    observed_tool_specs: list[BridgeToolSpec] = field(default_factory=list)

    def to_core_snapshot(self) -> CoreSnapshot:
        return CoreSnapshot(
            current_model=self.current_model.to_model(),
            models=[model.to_model_spec() for model in self.models],
            skills=[skill.to_skill() for skill in self.skills],
            sessions=[session.to_session() for session in self.sessions],
            tasks=[task.to_task() for task in self.tasks],
            runs=[run.to_run() for run in self.runs],
            profiles=[profile.to_profile() for profile in self.profiles],
            tool_specs=[],
        )
