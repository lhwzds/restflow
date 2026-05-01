"""Repository protocol and in-memory store helpers."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Generic, Protocol, TypeVar


T = TypeVar("T")


class Store(Protocol[T]):
    def get(self, id: str) -> T | None: ...
    def list(self) -> list[T]: ...
    def put(self, id: str, value: T) -> None: ...
    def delete(self, id: str) -> bool: ...
    def exists(self, id: str) -> bool: ...
    def replace_all(self, records: list[tuple[str, T]]) -> None: ...


@dataclass
class MemoryStore(Generic[T]):
    records: dict[str, T] = field(default_factory=dict)

    def get(self, id: str) -> T | None:
        return self.records.get(id)

    def list(self) -> list[T]:
        return list(self.records.values())

    def put(self, id: str, value: T) -> None:
        self.records[id] = value

    def delete(self, id: str) -> bool:
        return self.records.pop(id, None) is not None

    def exists(self, id: str) -> bool:
        return id in self.records

    def replace_all(self, records: list[tuple[str, T]]) -> None:
        self.records = dict(records)
