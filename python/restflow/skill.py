"""# codocia

Python skill owns the SDK wrapper for executable skill artifacts.

## Owns
- Python artifact dataclasses
- native-backed skill calls
- installed skill lookup helpers
- local skill installation wrapper

## Must Not
- duplicate Rust runtime validation
- execute skill subprocesses directly
- own agent planning

## Inputs
- skill IDs or artifact paths
- JSON-compatible skill input dictionaries
- optional RESTFLOW_SKILLS_DIR

## Outputs
- Python dictionaries returned from executable skills
- SkillArtifact metadata objects

## Depends On
- runtime

## Used By
- external Python agent frameworks

## Verify
- python3 -m unittest discover -s python/tests
"""

from __future__ import annotations

import json
import importlib
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class Skill:
    id: str
    name: str
    source: str = "user"
    source_ref: str | None = None
    read_only: bool = False
    description: str | None = None
    content: str = ""
    suggested_tools: list[str] = field(default_factory=list)


@dataclass
class SkillContext:
    assigned: list[Skill] = field(default_factory=list)
    mentioned: list[Skill] = field(default_factory=list)
    issues: list[str] = field(default_factory=list)


@dataclass
class ArtifactProtocol:
    transport: str = "stdio-json"
    input: str = "single-json-value"
    output: str = "single-json-value"


@dataclass
class ArtifactSchema:
    input: str | None = None
    output: str | None = None


@dataclass
class ArtifactSource:
    kind: str
    reference: str | None = None

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> "ArtifactSource":
        return cls(kind=str(value["type"]), reference=value.get("ref"))

    def to_dict(self) -> dict[str, Any]:
        body: dict[str, Any] = {"type": self.kind}
        if self.reference is not None:
            body["ref"] = self.reference
        return body


@dataclass
class SkillArtifact:
    schema_version: int
    kind: str
    id: str
    name: str
    version: str
    entry: str
    protocol: ArtifactProtocol = field(default_factory=ArtifactProtocol)
    schema: ArtifactSchema = field(default_factory=ArtifactSchema)
    source: ArtifactSource | None = None

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> "SkillArtifact":
        protocol = value.get("protocol") or {}
        schema = value.get("schema") or {}
        source = value.get("source")
        return cls(
            schema_version=int(value["schema_version"]),
            kind=str(value["kind"]),
            id=str(value["id"]),
            name=str(value["name"]),
            version=str(value["version"]),
            entry=str(value["entry"]),
            protocol=ArtifactProtocol(
                transport=str(protocol.get("transport", "stdio-json")),
                input=str(protocol.get("input", "single-json-value")),
                output=str(protocol.get("output", "single-json-value")),
            ),
            schema=ArtifactSchema(input=schema.get("input"), output=schema.get("output")),
            source=ArtifactSource.from_dict(source) if isinstance(source, dict) else None,
        )

    def to_dict(self) -> dict[str, Any]:
        body: dict[str, Any] = {
            "schema_version": self.schema_version,
            "kind": self.kind,
            "id": self.id,
            "name": self.name,
            "version": self.version,
            "entry": self.entry,
            "protocol": {
                "transport": self.protocol.transport,
                "input": self.protocol.input,
                "output": self.protocol.output,
            },
            "schema": {
                "input": self.schema.input,
                "output": self.schema.output,
            },
        }
        if self.source is not None:
            body["source"] = self.source.to_dict()
        return body

class SkillRuntimeError(RuntimeError):
    """Raised when a local executable skill cannot be loaded or called."""


@dataclass
class SkillHandle:
    root: Path
    artifact: SkillArtifact
    uv: str = "uv"

    def call(self, value: dict[str, Any] | None = None, timeout: float = 60.0) -> dict[str, Any]:
        input_value = dict(value or {})
        output_json = _call_native_json(
            "runtime_run_skill_json",
            str(self.root),
            json.dumps(input_value, separators=(",", ":")),
            int(timeout),
        )
        return _json_object(output_json, "skill output")

    def _entry_path(self) -> Path:
        return self.root / self.artifact.entry


@dataclass
class SkillRuntime:
    root: str | Path | None = None
    uv: str = "uv"

    def list(self) -> list[SkillArtifact]:
        root = str(self.base_dir()) if self.root is not None else None
        decoded = json.loads(_call_native_json("runtime_list_skills_json", root))
        if not isinstance(decoded, list):
            raise SkillRuntimeError("native skill list must decode to a JSON array")
        return [SkillArtifact.from_dict(item) for item in decoded if isinstance(item, dict)]

    def skill(self, id_or_path: str | Path) -> SkillHandle:
        root = self.resolve_skill_dir(id_or_path)
        return SkillHandle(root=root, artifact=load_artifact(root), uv=self.uv)

    def install_local(
        self,
        source: str | Path,
        *,
        skill_id: str | None = None,
        overwrite: bool = False,
    ) -> SkillHandle:
        root = str(self.base_dir()) if self.root is not None else None
        artifact = SkillArtifact.from_dict(
            _json_object(
                _call_native_json(
                    "runtime_install_local_skill_json",
                    str(Path(source).expanduser()),
                    root,
                    skill_id,
                    overwrite,
                ),
                "installed skill artifact",
            )
        )
        return self.skill(artifact.id)

    def resolve_skill_dir(self, id_or_path: str | Path) -> Path:
        candidate = Path(id_or_path).expanduser()
        if (candidate / "artifact.json").is_file():
            return candidate.resolve()
        return self.base_dir() / str(id_or_path)

    def base_dir(self) -> Path:
        if self.root is not None:
            return Path(self.root).expanduser().resolve()
        configured = os.environ.get("RESTFLOW_SKILLS_DIR")
        if configured:
            return Path(configured).expanduser().resolve()
        return Path.home() / ".restflow" / "skills"


def load_artifact(root: str | Path) -> SkillArtifact:
    return SkillArtifact.from_dict(
        _json_object(
            _call_native_json("runtime_load_artifact_json", str(Path(root).expanduser())),
            "skill artifact",
        )
    )


def build_skill(root: str | Path, target_dir: str | Path | None = None) -> SkillArtifact:
    return SkillArtifact.from_dict(
        _json_object(
            _call_native_json(
                "runtime_build_skill_json",
                str(Path(root).expanduser()),
                str(Path(target_dir).expanduser()) if target_dir is not None else None,
            ),
            "built skill artifact",
        )
    )


def list_skills(root: str | Path | None = None) -> list[SkillArtifact]:
    return SkillRuntime(root=root).list()


def skill(id_or_path: str | Path, root: str | Path | None = None) -> SkillHandle:
    return SkillRuntime(root=root).skill(id_or_path)


def install_local_skill(
    source: str | Path,
    root: str | Path | None = None,
    *,
    skill_id: str | None = None,
    overwrite: bool = False,
) -> SkillHandle:
    return SkillRuntime(root=root).install_local(
        source,
        skill_id=skill_id,
        overwrite=overwrite,
    )


def _native_runtime() -> Any:
    try:
        module = importlib.import_module("restflow.restflow_native")
    except ImportError as error:
        raise SkillRuntimeError(
            "restflow native runtime is unavailable; install the package with its PyO3 extension"
        ) from error

    required = [
        "runtime_load_artifact_json",
        "runtime_list_skills_json",
        "runtime_build_skill_json",
        "runtime_run_skill_json",
        "runtime_install_local_skill_json",
    ]
    missing = [name for name in required if not hasattr(module, name)]
    if missing:
        raise SkillRuntimeError(f"restflow native runtime is missing: {', '.join(missing)}")
    return module


def _call_native_json(function_name: str, *args: Any) -> str:
    function = getattr(_native_runtime(), function_name)
    try:
        return str(function(*args))
    except Exception as error:
        raise SkillRuntimeError(str(error)) from error


def _json_object(value: str, label: str) -> dict[str, Any]:
    try:
        decoded = json.loads(value)
    except json.JSONDecodeError as error:
        raise SkillRuntimeError(f"decode native {label} JSON") from error
    if not isinstance(decoded, dict):
        raise SkillRuntimeError(f"native {label} must decode to a JSON object")
    return decoded
