"""Model API placeholders."""

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


@dataclass
class ModelSelection:
    current: Model
