"""Auth API placeholders."""

from dataclasses import dataclass


@dataclass
class SecretRef:
    key: str


@dataclass
class Profile:
    provider: str
    secret: SecretRef

