"""
RestFlow AI - Rust-powered AI Agent Framework

Usage:
    from restflow_ai import workflow, step, tool, Agent

    @step
    def search(query: str) -> dict:
        return {"results": [...]}

    @workflow
    def analyze(query: str) -> str:
        data = search(query)
        return summarize(data)

    result = analyze.run("AI news")
"""

from .decorators import workflow, step, tool
from .agent import Agent
from .eval import Dataset, evaluate

__version__ = "0.1.0"
__all__ = ["workflow", "step", "tool", "Agent", "Dataset", "evaluate"]
