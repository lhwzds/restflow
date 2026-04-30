"""Bridge DTOs for moving legacy boundary data into the V2 core."""

from dataclasses import dataclass, field
from typing import Any

from .auth import Profile, SecretRef
from .chat import Message, Session
from .core import CoreCommand, CoreSnapshot
from .model import Model, ModelSpec, Provider
from .run import Run, Task
from .skill import Skill
from .tool import ToolCall, ToolSpec


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

    def to_model_spec(self) -> ModelSpec:
        return ModelSpec(
            model=Model(provider=Provider(id=self.provider), id=self.model),
            name=self.name,
            description=self.description,
        )


@dataclass
class BridgeSkill:
    id: str
    name: str
    source: str = "user"
    read_only: bool = False
    description: str | None = None
    content: str = ""
    suggested_tools: list[str] = field(default_factory=list)
    source_ref: str | None = None

    def to_skill(self) -> Skill:
        return Skill(
            id=self.id,
            name=self.name,
            source=self.source,
            read_only=self.read_only,
            description=self.description,
            content=self.content,
            suggested_tools=list(self.suggested_tools),
        )


@dataclass
class BridgeMessage:
    role: str
    text: str

    def to_message(self) -> Message:
        return Message(role=self.role, text=self.text)


@dataclass
class BridgeSession:
    id: str
    messages: list[BridgeMessage] = field(default_factory=list)

    def to_session(self) -> Session:
        return Session(id=self.id, messages=[message.to_message() for message in self.messages])


@dataclass
class BridgeTask:
    id: str
    title: str

    def to_task(self) -> Task:
        return Task(id=self.id, title=self.title)


@dataclass
class BridgeRun:
    id: str
    task_id: str
    status: str
    session_id: str | None = None

    def to_run(self) -> Run:
        return Run(id=self.id, task_id=self.task_id, status=self.status, session_id=self.session_id)


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
class BridgeSnapshot:
    current_model: BridgeModelRef
    models: list[BridgeModelSpec] = field(default_factory=list)
    skills: list[BridgeSkill] = field(default_factory=list)
    sessions: list[BridgeSession] = field(default_factory=list)
    tasks: list[BridgeTask] = field(default_factory=list)
    runs: list[BridgeRun] = field(default_factory=list)
    profiles: list[BridgeProfile] = field(default_factory=list)
    tool_specs: list[BridgeToolSpec] = field(default_factory=list)

    def to_core_snapshot(self) -> CoreSnapshot:
        return CoreSnapshot(
            current_model=self.current_model.to_model(),
            models=[model.to_model_spec() for model in self.models],
            skills=[skill.to_skill() for skill in self.skills],
            sessions=[session.to_session() for session in self.sessions],
            tasks=[task.to_task() for task in self.tasks],
            runs=[run.to_run() for run in self.runs],
            profiles=[profile.to_profile() for profile in self.profiles],
            tool_specs=[tool_spec.to_tool_spec() for tool_spec in self.tool_specs],
        )
