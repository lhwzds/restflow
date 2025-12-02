"""
Decorators for defining workflows, steps, and tools.
"""

from typing import Callable, TypeVar, Any
from functools import wraps

# Import native module (will fail until Rust is compiled)
try:
    from restflow_ai._native import (
        _register_workflow,
        _register_step,
        _register_tool,
        _compile_workflow,
        _execute_workflow,
        _resume_workflow,
        _get_traces,
    )
    NATIVE_AVAILABLE = True
except ImportError:
    NATIVE_AVAILABLE = False
    # Stub functions for development
    def _register_workflow(name, wrapper): pass
    def _register_step(name, wrapper): pass
    def _register_tool(name, wrapper): pass
    def _compile_workflow(json): return json
    def _execute_workflow(graph, args, kwargs): raise NotImplementedError()
    def _resume_workflow(execution_id): raise NotImplementedError()
    def _get_traces(execution_id): return []

from .parser import WorkflowParser

F = TypeVar('F', bound=Callable[..., Any])


class WorkflowWrapper:
    """Wrapper around a workflow function."""

    def __init__(self, func: Callable, graph: str, predicates: dict = None):
        self.func = func
        self.graph = graph
        self.predicates = predicates or {}
        self._last_run = None
        self.__name__ = func.__name__
        self.__doc__ = func.__doc__

    def run(self, *args, **kwargs):
        """Execute the workflow."""
        result = _execute_workflow(
            self.graph,
            list(args),
            kwargs,
        )
        self._last_run = result
        return result

    def _eval_predicate(self, predicate_id: str, context: dict) -> bool:
        """Evaluate a predicate with the current context."""
        if predicate_id not in self.predicates:
            raise ValueError(f"Unknown predicate: {predicate_id}")
        return self.predicates[predicate_id](context)

    def resume(self, execution_id: str):
        """Resume from a previous execution."""
        return _resume_workflow(execution_id)

    @property
    def last_run(self):
        """Get the last execution result."""
        return self._last_run

    def traces(self, execution_id: str = None):
        """Get traces for an execution."""
        eid = execution_id or (self._last_run.execution_id if self._last_run else None)
        if not eid:
            raise ValueError("No execution ID available")
        return _get_traces(eid)


class StepWrapper:
    """Wrapper around a step function."""

    def __init__(self, func: Callable, schema: dict):
        self.func = func
        self.schema = schema
        self.__name__ = func.__name__
        self.__doc__ = func.__doc__

    def __call__(self, *args, **kwargs):
        """Call the underlying function."""
        return self.func(*args, **kwargs)


class ToolWrapper:
    """Wrapper around a tool function."""

    def __init__(self, func: Callable, schema: dict):
        self.func = func
        self.schema = schema
        self.__name__ = func.__name__
        self.__doc__ = func.__doc__

    def __call__(self, *args, **kwargs):
        """Call the underlying function."""
        return self.func(*args, **kwargs)


def workflow(func: F) -> WorkflowWrapper:
    """
    Decorator to define a workflow.

    Usage:
        @workflow
        def my_workflow(query: str) -> str:
            data = search(query)
            return analyze(data)
    """
    # Parse with Python ast module
    parser = WorkflowParser()
    workflow_def, predicates = parser.parse(func)

    # Send to Rust for compilation and validation
    graph = _compile_workflow(workflow_def.to_json())

    # Create workflow wrapper
    wrapper = WorkflowWrapper(func, graph, predicates)
    _register_workflow(func.__name__, wrapper)

    return wrapper


def step(func: F) -> StepWrapper:
    """
    Decorator to define a workflow step.

    Usage:
        @step
        def search(query: str) -> dict:
            return web_search(query)
    """
    import inspect
    sig = inspect.signature(func)
    schema = _generate_schema(sig)

    wrapper = StepWrapper(func, schema)
    _register_step(func.__name__, wrapper)

    return wrapper


def tool(func: F) -> ToolWrapper:
    """
    Decorator to define an agent tool.

    Usage:
        @tool
        def calculator(expression: str) -> float:
            '''Evaluates a math expression'''
            return eval(expression)
    """
    import inspect
    sig = inspect.signature(func)
    schema = _generate_tool_schema(func.__name__, func.__doc__, sig)

    wrapper = ToolWrapper(func, schema)
    _register_tool(func.__name__, wrapper)

    return wrapper


def _generate_schema(sig) -> dict:
    """Generate JSON schema from function signature."""
    properties = {}
    required = []

    for name, param in sig.parameters.items():
        prop = {"type": "string"}  # Default type

        # Try to get type hint
        if param.annotation != inspect.Parameter.empty:
            prop["type"] = _python_type_to_json_type(param.annotation)

        properties[name] = prop

        # Check if required (no default)
        if param.default == inspect.Parameter.empty:
            required.append(name)

    return {
        "type": "object",
        "properties": properties,
        "required": required,
    }


def _generate_tool_schema(name: str, docstring: str, sig) -> dict:
    """Generate tool schema for LLM."""
    schema = _generate_schema(sig)
    schema["name"] = name
    schema["description"] = docstring or f"Tool: {name}"
    return schema


def _python_type_to_json_type(python_type) -> str:
    """Convert Python type to JSON schema type."""
    type_map = {
        str: "string",
        int: "integer",
        float: "number",
        bool: "boolean",
        list: "array",
        dict: "object",
    }
    return type_map.get(python_type, "string")


# Import inspect here for the helper functions
import inspect
