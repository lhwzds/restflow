"""
Agent class for AI agent execution.
"""

from dataclasses import dataclass
from typing import Optional

# Import native module (will fail until Rust is compiled)
try:
    from restflow_ai._native import _run_agent, _resume_agent
    NATIVE_AVAILABLE = True
except ImportError:
    NATIVE_AVAILABLE = False
    def _run_agent(*args, **kwargs): raise NotImplementedError("Native module not available")
    def _resume_agent(*args, **kwargs): raise NotImplementedError("Native module not available")


@dataclass
class AgentResult:
    """Result from agent execution."""
    final_answer: str
    iterations: int
    tool_calls: list
    total_tokens: int
    total_cost: float
    execution_id: str


class Agent:
    """
    AI Agent with tool use capability.

    Usage:
        result = Agent.run(
            goal="Find and summarize recent AI news",
            tools=[search, summarize],
            max_iterations=10,
        )
    """

    @staticmethod
    def run(
        goal: str,
        tools: list = None,
        model: str = "gpt-4",
        max_iterations: int = 10,
        temperature: float = 0.7,
    ) -> AgentResult:
        """
        Run the agent to achieve a goal.

        Args:
            goal: The objective for the agent to achieve.
            tools: List of tool functions available to the agent.
            model: LLM model to use (default: gpt-4).
            max_iterations: Maximum number of think-act-observe cycles.
            temperature: LLM temperature for randomness.

        Returns:
            AgentResult with final answer and execution details.
        """
        tool_names = [t.__name__ for t in (tools or [])]

        result = _run_agent(
            goal=goal,
            tools=tool_names,
            model=model,
            max_iterations=max_iterations,
            temperature=temperature,
        )

        return AgentResult(
            final_answer=result.get("final_answer", ""),
            iterations=result.get("iterations", 0),
            tool_calls=result.get("tool_history", []),
            total_tokens=result.get("total_tokens", 0),
            total_cost=result.get("total_cost", 0.0),
            execution_id=result.get("execution_id", ""),
        )

    @staticmethod
    def resume(execution_id: str) -> AgentResult:
        """
        Resume an interrupted agent execution.

        Args:
            execution_id: The ID of the execution to resume.

        Returns:
            AgentResult with final answer and execution details.
        """
        result = _resume_agent(execution_id)

        return AgentResult(
            final_answer=result.get("final_answer", ""),
            iterations=result.get("iterations", 0),
            tool_calls=result.get("tool_history", []),
            total_tokens=result.get("total_tokens", 0),
            total_cost=result.get("total_cost", 0.0),
            execution_id=execution_id,
        )
