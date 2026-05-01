"""Provider and model selection data models."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass
class Provider:
    id: str


@dataclass
class Model:
    provider: Provider
    id: str


@dataclass
class ModelSpec:
    model: Model
    name: str
    description: str | None = None
    client_model: str | None = None
    client_kind: str | None = None
    base_url: str | None = None


@dataclass
class ModelSelection:
    current: Model
