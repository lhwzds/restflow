"""LangGraph node integration example for RestFlow executable skills."""

from __future__ import annotations

from typing import Any

import restflow


def regex_finder_node(state: dict[str, Any]) -> dict[str, Any]:
    """Call a RestFlow skill and return a LangGraph-compatible state patch."""

    result = restflow.skill("regex-finder").call(
        {
            "pattern": state["pattern"],
            "path": state.get("path", "."),
        }
    )
    return {"regex_finder_result": result}


__all__ = ["regex_finder_node"]
