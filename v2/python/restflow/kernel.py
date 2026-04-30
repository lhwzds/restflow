"""Composable in-memory kernel placeholder."""

from dataclasses import dataclass, field
from typing import Any

from .auth import Profile
from .chat import Session
from .event import Event
from .model import Model, ModelSpec
from .run import Run, Task
from .skill import Skill
from .tool import Registry, ToolCall, ToolSpec


@dataclass
class KernelCommand:
    type: str
    payload: dict[str, Any] = field(default_factory=dict)


@dataclass
class KernelResponse:
    type: str
    payload: dict[str, Any] = field(default_factory=dict)


@dataclass
class KernelSnapshot:
    current_model: Model
    models: list[ModelSpec] = field(default_factory=list)
    skills: list[Skill] = field(default_factory=list)
    sessions: list[Session] = field(default_factory=list)
    tasks: list[Task] = field(default_factory=list)
    runs: list[Run] = field(default_factory=list)
    profiles: list[Profile] = field(default_factory=list)
    tool_specs: list[ToolSpec] = field(default_factory=list)


@dataclass
class Kernel:
    model: Model
    tools: Registry = field(default_factory=Registry)
    models: list[ModelSpec] = field(default_factory=list)
    skills: dict[str, Skill] = field(default_factory=dict)
    sessions: dict[str, Session] = field(default_factory=dict)
    tasks: dict[str, Task] = field(default_factory=dict)
    runs: dict[str, Run] = field(default_factory=dict)
    profiles: dict[str, Profile] = field(default_factory=dict)

    def save_skill(self, skill: Skill) -> None:
        self.skills[skill.id] = skill

    def save_profile(self, profile: Profile) -> None:
        self.profiles[profile.provider.id] = profile

    def switch_model(self, model: Model) -> None:
        self.model = model

    @classmethod
    def from_snapshot(cls, snapshot: KernelSnapshot) -> "Kernel":
        kernel = cls(model=snapshot.current_model)
        kernel.models.extend(snapshot.models)
        kernel.skills.update({skill.id: skill for skill in snapshot.skills})
        kernel.sessions.update({session.id: session for session in snapshot.sessions})
        kernel.tasks.update({task.id: task for task in snapshot.tasks})
        kernel.runs.update({run.id: run for run in snapshot.runs})
        kernel.profiles.update({profile.provider.id: profile for profile in snapshot.profiles})
        return kernel

    def snapshot(self) -> KernelSnapshot:
        return KernelSnapshot(
            current_model=self.model,
            models=list(self.models),
            skills=list(self.skills.values()),
            sessions=list(self.sessions.values()),
            tasks=list(self.tasks.values()),
            runs=list(self.runs.values()),
            profiles=list(self.profiles.values()),
            tool_specs=[ToolSpec(name=name) for name in self.tools.names()],
        )

    def start_run(self, task: Task, run_id: str, session_id: str) -> Run:
        self.tasks[task.id] = task
        run = Run(id=run_id, task_id=task.id, status="running", session_id=session_id)
        self.runs[run_id] = run
        return run

    def run_task(self, run_id: str, task: Task) -> Run:
        self.tasks[task.id] = task
        run = self.runs.get(run_id, Run(id=run_id, task_id=task.id, status="pending"))
        run.status = "done"
        self.runs[run_id] = run
        return run

    def call_tool_events(self, call: ToolCall) -> list[Event]:
        events = [Event(type="tool_call", value=call)]
        try:
            events.append(Event(type="tool_result", value=self.tools.call(call)))
        except KeyError as exc:
            events.append(Event(type="error", value=str(exc)))
        return events

    def handle(self, command: KernelCommand) -> KernelResponse:
        if command.type == "save_skill":
            self.save_skill(command.payload["skill"])
            return KernelResponse(type="saved")
        if command.type == "save_profile":
            self.save_profile(command.payload["profile"])
            return KernelResponse(type="saved")
        if command.type == "switch_model":
            model = command.payload["model"]
            self.switch_model(model)
            return KernelResponse(type="model_switched", payload={"model": model})
        if command.type == "start_run":
            run = self.start_run(
                command.payload["task"],
                command.payload["run_id"],
                command.payload["session_id"],
            )
            return KernelResponse(type="run_started", payload={"run": run})
        if command.type == "run_task":
            run = self.run_task(command.payload["run_id"], command.payload["task"])
            return KernelResponse(type="run_task", payload={"run": run})
        if command.type == "call_tool":
            events = self.call_tool_events(command.payload["call"])
            return KernelResponse(type="tool_events", payload={"events": events})
        raise ValueError(f"unknown kernel command: {command.type}")
