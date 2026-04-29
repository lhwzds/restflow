"""Store API placeholders."""

from typing import Protocol, TypeVar


T = TypeVar("T")


class Store(Protocol[T]):
    def get(self, id: str) -> T | None: ...
    def put(self, id: str, value: T) -> None: ...
    def delete(self, id: str) -> bool: ...

