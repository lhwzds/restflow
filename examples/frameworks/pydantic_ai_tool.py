"""PydanticAI tool integration example for RestFlow executable skills."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

import restflow


@dataclass
class RegexFinderArgs:
    pattern: str
    path: str = "."


def regex_finder_pydantic_tool(arguments: RegexFinderArgs) -> dict[str, Any]:
    """Call a RestFlow skill from a PydanticAI tool implementation."""

    return restflow.skill("regex-finder").call(
        {
            "pattern": arguments.pattern,
            "path": arguments.path,
        }
    )


__all__ = ["RegexFinderArgs", "regex_finder_pydantic_tool"]
