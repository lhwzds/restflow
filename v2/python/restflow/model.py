"""Model API placeholders."""

from dataclasses import dataclass


@dataclass
class Model:
    provider: str
    id: str

