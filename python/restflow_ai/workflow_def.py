"""
WorkflowDef dataclasses - The contract between Python and Rust.
"""

from dataclasses import dataclass, field
from typing import Union, Optional
import json


@dataclass
class StepDef:
    """A single step execution."""
    name: str
    output: str
    inputs: list[str] = field(default_factory=list)
    config: dict = field(default_factory=dict)


@dataclass
class ParallelDef:
    """Parallel execution of multiple steps."""
    outputs: list[str]
    steps: list['NodeDef']


@dataclass
class ConditionDef:
    """Conditional branching."""
    predicate_id: str
    predicate_source: str
    then_branch: 'NodeDef'
    else_branch: Optional['NodeDef'] = None


@dataclass
class LoopDef:
    """Loop construct."""
    iter_var: str
    iterable: str
    body: 'NodeDef'


@dataclass
class SequenceDef:
    """Sequential execution."""
    steps: list['NodeDef']


# Union type for all node types
NodeDef = Union[StepDef, ParallelDef, ConditionDef, LoopDef, SequenceDef]


@dataclass
class ParameterDef:
    """Workflow parameter."""
    name: str
    type_hint: str
    default: Optional[str] = None


@dataclass
class WorkflowDef:
    """Complete workflow definition."""
    name: str
    parameters: list[ParameterDef]
    body: NodeDef
    return_var: Optional[str] = None

    def to_json(self) -> str:
        """Serialize to JSON for Rust."""
        return json.dumps(self._to_dict(), indent=2)

    def _to_dict(self) -> dict:
        """Convert to dictionary."""
        return {
            "name": self.name,
            "parameters": [
                {
                    "name": p.name,
                    "type_hint": p.type_hint,
                    "default": p.default,
                }
                for p in self.parameters
            ],
            "body": self._node_to_dict(self.body),
            "return_var": self.return_var,
        }

    def _node_to_dict(self, node: NodeDef) -> dict:
        """Convert a node to dictionary."""
        if isinstance(node, StepDef):
            return {
                "type": "Step",
                "name": node.name,
                "output": node.output,
                "inputs": node.inputs,
                "config": node.config,
            }
        elif isinstance(node, ParallelDef):
            return {
                "type": "Parallel",
                "outputs": node.outputs,
                "steps": [self._node_to_dict(s) for s in node.steps],
            }
        elif isinstance(node, ConditionDef):
            result = {
                "type": "Condition",
                "predicate_id": node.predicate_id,
                "predicate_source": node.predicate_source,
                "then_branch": self._node_to_dict(node.then_branch),
            }
            if node.else_branch:
                result["else_branch"] = self._node_to_dict(node.else_branch)
            return result
        elif isinstance(node, LoopDef):
            return {
                "type": "Loop",
                "iter_var": node.iter_var,
                "iterable": node.iterable,
                "body": self._node_to_dict(node.body),
            }
        elif isinstance(node, SequenceDef):
            return {
                "type": "Sequence",
                "steps": [self._node_to_dict(s) for s in node.steps],
            }
        else:
            raise ValueError(f"Unknown node type: {type(node)}")
