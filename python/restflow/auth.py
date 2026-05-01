"""Provider profile and secret-reference data models."""

from __future__ import annotations

from dataclasses import dataclass

from .model import Provider


@dataclass
class SecretRef:
    key: str


@dataclass
class Profile:
    provider: Provider
    secret: SecretRef
