"""
Python AST parser for workflow definitions.

Parses Python functions decorated with @workflow into WorkflowDef structures.
"""

import ast
import inspect
from typing import Callable, Optional

from .workflow_def import (
    WorkflowDef,
    ParameterDef,
    NodeDef,
    StepDef,
    ParallelDef,
    ConditionDef,
    LoopDef,
    SequenceDef,
)


class WorkflowParser(ast.NodeVisitor):
    """Parse Python function into WorkflowDef."""

    def __init__(self):
        self.steps: list[NodeDef] = []
        self.var_to_step: dict[str, str] = {}  # var name → step that produces it
        self.predicates: dict[str, Callable] = {}  # predicate_id → lambda
        self.predicate_counter = 0

    def parse(self, func: Callable) -> tuple[WorkflowDef, dict[str, Callable]]:
        """Parse function and return (WorkflowDef, predicates)."""
        source = inspect.getsource(func)

        # Remove decorator lines
        lines = source.split('\n')
        source = '\n'.join(l for l in lines if not l.strip().startswith('@'))

        tree = ast.parse(source)
        func_def = tree.body[0]

        # Extract parameters
        parameters = self._parse_parameters(func_def)

        # Parse body
        for stmt in func_def.body:
            self.visit(stmt)

        workflow_def = WorkflowDef(
            name=func.__name__,
            parameters=parameters,
            body=SequenceDef(steps=self.steps),
            return_var=self._find_return_var(),
        )

        return workflow_def, self.predicates

    def visit_Assign(self, node: ast.Assign):
        """Handle: x = step(y) or a, b = step1(), step2()"""

        if isinstance(node.value, ast.Call):
            # Single assignment: x = step(y)
            step = self._parse_call(node.value)
            step.output = self._get_assign_target(node)
            self.var_to_step[step.output] = step.name
            self.steps.append(step)

        elif isinstance(node.value, ast.Tuple):
            # Tuple assignment: a, b = step1(), step2() → Parallel!
            outputs = self._get_tuple_targets(node.targets[0])
            parallel_steps = []

            for i, elt in enumerate(node.value.elts):
                if isinstance(elt, ast.Call):
                    step = self._parse_call(elt)
                    step.output = outputs[i]
                    parallel_steps.append(step)
                    self.var_to_step[outputs[i]] = step.name

            if parallel_steps:
                self.steps.append(ParallelDef(
                    outputs=outputs,
                    steps=parallel_steps,
                ))

    def visit_If(self, node: ast.If):
        """Handle: if condition: ... else: ..."""

        # Register predicate as callable
        predicate_id = f"pred_{self.predicate_counter}"
        self.predicate_counter += 1
        predicate_source = ast.unparse(node.test)

        # Parse then branch
        then_parser = WorkflowParser()
        then_parser.predicates = self.predicates
        then_parser.predicate_counter = self.predicate_counter
        for stmt in node.body:
            then_parser.visit(stmt)
        then_branch = SequenceDef(steps=then_parser.steps)
        self.predicate_counter = then_parser.predicate_counter

        # Parse else branch
        else_branch = None
        if node.orelse:
            else_parser = WorkflowParser()
            else_parser.predicates = self.predicates
            else_parser.predicate_counter = self.predicate_counter
            for stmt in node.orelse:
                else_parser.visit(stmt)
            else_branch = SequenceDef(steps=else_parser.steps)
            self.predicate_counter = else_parser.predicate_counter

        self.steps.append(ConditionDef(
            predicate_id=predicate_id,
            predicate_source=predicate_source,
            then_branch=then_branch,
            else_branch=else_branch,
        ))

    def visit_For(self, node: ast.For):
        """Handle: for x in xs: ..."""
        iter_var = node.target.id if isinstance(node.target, ast.Name) else "item"
        iterable = ast.unparse(node.iter)

        # Parse loop body
        body_parser = WorkflowParser()
        for stmt in node.body:
            body_parser.visit(stmt)
        body = SequenceDef(steps=body_parser.steps)

        self.steps.append(LoopDef(
            iter_var=iter_var,
            iterable=iterable,
            body=body,
        ))

    def visit_Return(self, node: ast.Return):
        """Handle: return step(x) or return var"""
        if isinstance(node.value, ast.Call):
            step = self._parse_call(node.value)
            step.output = "__return__"
            self.steps.append(step)
        elif isinstance(node.value, ast.Name):
            # Just reference existing variable
            pass

    def _parse_call(self, node: ast.Call) -> StepDef:
        """Parse function call into StepDef."""
        func_name = self._get_func_name(node.func)

        # Extract positional args as input references
        inputs = []
        for arg in node.args:
            if isinstance(arg, ast.Name):
                inputs.append(arg.id)
            else:
                inputs.append(ast.unparse(arg))

        # Extract keyword args as config
        config = {}
        for kw in node.keywords:
            config[kw.arg] = self._value_to_python(kw.value)

        return StepDef(
            name=func_name,
            output="",  # Will be filled by caller
            inputs=inputs,
            config=config,
        )

    def _get_func_name(self, node) -> str:
        if isinstance(node, ast.Name):
            return node.id
        elif isinstance(node, ast.Attribute):
            return f"{self._get_func_name(node.value)}.{node.attr}"
        return "unknown"

    def _get_assign_target(self, node: ast.Assign) -> str:
        target = node.targets[0]
        if isinstance(target, ast.Name):
            return target.id
        return "unknown"

    def _get_tuple_targets(self, node) -> list[str]:
        if isinstance(node, ast.Tuple):
            return [elt.id for elt in node.elts if isinstance(elt, ast.Name)]
        return []

    def _value_to_python(self, node):
        """Convert AST node to Python value for config."""
        if isinstance(node, ast.Constant):
            return node.value
        elif isinstance(node, ast.List):
            return [self._value_to_python(elt) for elt in node.elts]
        elif isinstance(node, ast.Dict):
            return {
                self._value_to_python(k): self._value_to_python(v)
                for k, v in zip(node.keys, node.values)
            }
        else:
            # For complex expressions, keep as string
            return ast.unparse(node)

    def _parse_parameters(self, func_def: ast.FunctionDef) -> list[ParameterDef]:
        params = []
        for arg in func_def.args.args:
            type_hint = ""
            if arg.annotation:
                type_hint = ast.unparse(arg.annotation)
            params.append(ParameterDef(
                name=arg.arg,
                type_hint=type_hint,
            ))
        return params

    def _find_return_var(self) -> Optional[str]:
        # Find last step that produces __return__ or last step output
        for step in reversed(self.steps):
            if isinstance(step, StepDef) and step.output == "__return__":
                return "__return__"
            elif isinstance(step, StepDef):
                return step.output
        return None
