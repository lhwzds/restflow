"""OpenAI Agents SDK integration example for RestFlow executable skills."""

from __future__ import annotations

from typing import Any

import restflow


def regex_finder_function_tool(pattern: str, path: str = ".") -> dict[str, Any]:
    """Use this function body inside an OpenAI Agents function tool."""

    return restflow.skill("regex-finder").call(
        {
            "pattern": pattern,
            "path": path,
        }
    )


__all__ = ["regex_finder_function_tool"]
