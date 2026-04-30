"""Core JSON ABI for PyO3-backed Python bindings."""

from __future__ import annotations

import importlib
import json
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field, fields as dataclass_fields, is_dataclass
from enum import Enum
from types import MappingProxyType
from typing import Any, Protocol

from .auth import Profile
from .chat import Session
from .model import Model, ModelSpec
from .run import Run, Task
from .skill import Skill
from .store import MemoryStore, Store
from .tool import ToolSpec


JsonObject = dict[str, Any]


def _json_value(value: Any) -> Any:
    if is_dataclass(value) and not isinstance(value, type):
        return {
            item.name: _json_value(getattr(value, item.name))
            for item in dataclass_fields(value)
        }
    if isinstance(value, Enum):
        return value.value
    if isinstance(value, Mapping):
        return {str(key): _json_value(item) for key, item in value.items()}
    if isinstance(value, (tuple, list)):
        return [_json_value(item) for item in value]
    return value


def _tagged_object(tag: str, values: Mapping[str, Any], kind: str) -> JsonObject:
    body = _json_value(values)
    if "type" in body:
        raise ValueError(f"core {kind} field `type` is reserved")
    return {"type": tag, **body}


@dataclass(init=False)
class CoreCommand:
    type: str
    fields: JsonObject = field(default_factory=dict)

    def __init__(
        self,
        type: str,
        fields: Mapping[str, Any] | None = None,
        payload: Mapping[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        if fields is not None and payload is not None:
            raise ValueError("use either fields or payload, not both")
        data = dict(fields if fields is not None else payload or {})
        data.update(kwargs)
        if "type" in data:
            raise ValueError("core command field `type` is reserved")
        self.type = type
        self.fields = data

    @property
    def payload(self) -> Mapping[str, Any]:
        return MappingProxyType(self.fields)

    def to_dict(self) -> JsonObject:
        return _tagged_object(self.type, self.fields, "command")

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), separators=(",", ":"), sort_keys=True)

    @classmethod
    def from_dict(cls, value: Mapping[str, Any]) -> "CoreCommand":
        data = dict(value)
        command_type = data.pop("type")
        if not isinstance(command_type, str):
            raise TypeError("core command type must be a string")
        return cls(type=command_type, fields=data)

    @classmethod
    def from_json(cls, value: str) -> "CoreCommand":
        decoded = json.loads(value)
        if not isinstance(decoded, dict):
            raise TypeError("core command JSON must decode to an object")
        return cls.from_dict(decoded)


@dataclass(init=False)
class CoreResponse:
    type: str
    fields: JsonObject = field(default_factory=dict)

    def __init__(
        self,
        type: str,
        fields: Mapping[str, Any] | None = None,
        payload: Mapping[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        if fields is not None and payload is not None:
            raise ValueError("use either fields or payload, not both")
        data = dict(fields if fields is not None else payload or {})
        data.update(kwargs)
        if "type" in data:
            raise ValueError("core response field `type` is reserved")
        self.type = type
        self.fields = data

    @property
    def payload(self) -> Mapping[str, Any]:
        return MappingProxyType(self.fields)

    def to_dict(self) -> JsonObject:
        return _tagged_object(self.type, self.fields, "response")

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), separators=(",", ":"), sort_keys=True)

    @classmethod
    def from_dict(cls, value: Mapping[str, Any]) -> "CoreResponse":
        data = dict(value)
        response_type = data.pop("type")
        if not isinstance(response_type, str):
            raise TypeError("core response type must be a string")
        return cls(type=response_type, fields=data)

    @classmethod
    def from_json(cls, value: str) -> "CoreResponse":
        decoded = json.loads(value)
        if not isinstance(decoded, dict):
            raise TypeError("core response JSON must decode to an object")
        return cls.from_dict(decoded)


class CoreTransport(Protocol):
    def send(self, command_json: str) -> str: ...


class NativeModule(Protocol):
    def handle_json(self, command_json: str) -> str: ...


@dataclass
class NativeTransport:
    """Adapter for the PyO3 module that owns Rust core execution."""

    module: NativeModule

    def send(self, command_json: str) -> str:
        return self.module.handle_json(command_json)


@dataclass
class CallableTransport:
    """Test adapter for injecting a Core JSON handler without PyO3."""

    handler: Callable[[str], str]

    def send(self, command_json: str) -> str:
        return self.handler(command_json)


def load_native_transport(module_name: str = "restflow_native") -> NativeTransport:
    module = importlib.import_module(module_name)
    core_factory = getattr(module, "Core", None)
    if core_factory is not None:
        core = core_factory()
        if not hasattr(core, "handle_json"):
            raise TypeError(f"{module_name}.Core must expose handle_json(command_json: str) -> str")
        return NativeTransport(core)
    if not hasattr(module, "handle_json"):
        raise TypeError(f"{module_name} must expose handle_json(command_json: str) -> str")
    return NativeTransport(module)


@dataclass
class CoreClient:
    """Thin Python wrapper over the Rust core JSON ABI."""

    transport: CoreTransport

    @classmethod
    def native(cls, module_name: str = "restflow_native") -> "CoreClient":
        return cls(load_native_transport(module_name))

    def handle(self, command: CoreCommand) -> CoreResponse:
        return CoreResponse.from_json(self.transport.send(command.to_json()))


@dataclass
class CoreSnapshot:
    current_model: Model
    models: list[ModelSpec] = field(default_factory=list)
    skills: list[Skill] = field(default_factory=list)
    sessions: list[Session] = field(default_factory=list)
    tasks: list[Task] = field(default_factory=list)
    runs: list[Run] = field(default_factory=list)
    profiles: list[Profile] = field(default_factory=list)
    tool_specs: list[ToolSpec] = field(default_factory=list)


@dataclass
class InMemoryCoreHarness:
    """In-memory harness kept for prototype migration helpers.

    Production Python integrations should use CoreClient so Rust remains the
    owner of CoreCommand behavior.
    """

    model: Model
    models: list[ModelSpec] = field(default_factory=list)
    skills: Store[Skill] = field(default_factory=MemoryStore)
    sessions: Store[Session] = field(default_factory=MemoryStore)
    tasks: Store[Task] = field(default_factory=MemoryStore)
    runs: Store[Run] = field(default_factory=MemoryStore)
    profiles: Store[Profile] = field(default_factory=MemoryStore)

    def save_skill(self, skill: Skill) -> None:
        self.skills.put(skill.id, skill)

    def save_profile(self, profile: Profile) -> None:
        self.profiles.put(profile.provider.id, profile)

    def switch_model(self, model: Model) -> None:
        self.model = model

    @classmethod
    def from_snapshot(cls, snapshot: CoreSnapshot) -> "InMemoryCoreHarness":
        core = cls(model=snapshot.current_model)
        core.models.extend(snapshot.models)
        core.skills.replace_all([(skill.id, skill) for skill in snapshot.skills])
        core.sessions.replace_all([(session.id, session) for session in snapshot.sessions])
        core.tasks.replace_all([(task.id, task) for task in snapshot.tasks])
        core.runs.replace_all([(run.id, run) for run in snapshot.runs])
        core.profiles.replace_all(
            [(profile.provider.id, profile) for profile in snapshot.profiles]
        )
        return core

    def snapshot(self) -> CoreSnapshot:
        return CoreSnapshot(
            current_model=self.model,
            models=list(self.models),
            skills=self.skills.list(),
            sessions=self.sessions.list(),
            tasks=self.tasks.list(),
            runs=self.runs.list(),
            profiles=self.profiles.list(),
            tool_specs=[],
        )
