"""
Evaluation utilities for AI agents and workflows.

Provides Dataset management and evaluation metrics.
"""

from dataclasses import dataclass, field
from typing import Callable, Optional, Any
import json

# Import native module (will fail until Rust is compiled)
try:
    from restflow_ai._native import _run_evaluation
    NATIVE_AVAILABLE = True
except ImportError:
    NATIVE_AVAILABLE = False
    def _run_evaluation(*args, **kwargs): raise NotImplementedError("Native module not available")


@dataclass
class Example:
    """A single evaluation example."""
    input: dict
    expected: Any
    metadata: dict = field(default_factory=dict)


@dataclass
class EvalResult:
    """Result from a single evaluation."""
    example_id: str
    input: dict
    expected: Any
    actual: Any
    score: float
    passed: bool
    latency_ms: float
    tokens_used: int
    cost: float
    error: Optional[str] = None


@dataclass
class EvalSummary:
    """Summary of evaluation results."""
    total: int
    passed: int
    failed: int
    accuracy: float
    avg_latency_ms: float
    total_tokens: int
    total_cost: float
    results: list[EvalResult]


class Dataset:
    """
    Dataset for evaluation.

    Usage:
        dataset = Dataset.from_list([
            {"input": {"query": "2+2"}, "expected": "4"},
            {"input": {"query": "3*3"}, "expected": "9"},
        ])

        # Or load from file
        dataset = Dataset.load("test_cases.json")
    """

    def __init__(self, examples: list[Example]):
        self.examples = examples

    @classmethod
    def from_list(cls, items: list[dict]) -> "Dataset":
        """Create dataset from list of dicts."""
        examples = []
        for item in items:
            examples.append(Example(
                input=item.get("input", {}),
                expected=item.get("expected"),
                metadata=item.get("metadata", {}),
            ))
        return cls(examples)

    @classmethod
    def load(cls, path: str) -> "Dataset":
        """Load dataset from JSON file."""
        with open(path, "r") as f:
            data = json.load(f)

        if isinstance(data, list):
            return cls.from_list(data)
        elif isinstance(data, dict) and "examples" in data:
            return cls.from_list(data["examples"])
        else:
            raise ValueError("Invalid dataset format")

    def save(self, path: str) -> None:
        """Save dataset to JSON file."""
        data = [
            {
                "input": ex.input,
                "expected": ex.expected,
                "metadata": ex.metadata,
            }
            for ex in self.examples
        ]
        with open(path, "w") as f:
            json.dump(data, f, indent=2)

    def __len__(self) -> int:
        return len(self.examples)

    def __iter__(self):
        return iter(self.examples)

    def subset(self, indices: list[int]) -> "Dataset":
        """Get a subset of the dataset."""
        return Dataset([self.examples[i] for i in indices])

    def sample(self, n: int) -> "Dataset":
        """Randomly sample n examples."""
        import random
        indices = random.sample(range(len(self.examples)), min(n, len(self.examples)))
        return self.subset(indices)


def evaluate(
    target: Callable,
    dataset: Dataset,
    scorer: Optional[Callable[[Any, Any], float]] = None,
    max_concurrency: int = 5,
) -> EvalSummary:
    """
    Evaluate a workflow or agent against a dataset.

    Args:
        target: The workflow or agent to evaluate.
        dataset: Dataset of test cases.
        scorer: Optional custom scoring function (actual, expected) -> float.
                Default uses exact match (1.0 or 0.0).
        max_concurrency: Maximum parallel evaluations.

    Returns:
        EvalSummary with detailed results.

    Usage:
        @workflow
        def my_workflow(query: str) -> str:
            ...

        dataset = Dataset.from_list([...])
        summary = evaluate(my_workflow, dataset)
        print(f"Accuracy: {summary.accuracy:.2%}")
    """
    # Default scorer: exact match
    if scorer is None:
        def scorer(actual, expected):
            return 1.0 if actual == expected else 0.0

    # Prepare examples for Rust
    examples_json = json.dumps([
        {
            "input": ex.input,
            "expected": ex.expected,
            "metadata": ex.metadata,
        }
        for ex in dataset.examples
    ])

    # Get target name for registration lookup
    target_name = getattr(target, "__name__", str(target))

    # Run evaluation in Rust (handles concurrency and tracing)
    raw_results = _run_evaluation(
        target_name=target_name,
        examples=examples_json,
        max_concurrency=max_concurrency,
    )

    # Process results
    results = []
    total_passed = 0
    total_latency = 0.0
    total_tokens = 0
    total_cost = 0.0

    for i, raw in enumerate(raw_results):
        actual = raw.get("actual")
        expected = dataset.examples[i].expected
        score = scorer(actual, expected)
        passed = score >= 0.5

        result = EvalResult(
            example_id=raw.get("example_id", str(i)),
            input=dataset.examples[i].input,
            expected=expected,
            actual=actual,
            score=score,
            passed=passed,
            latency_ms=raw.get("latency_ms", 0.0),
            tokens_used=raw.get("tokens_used", 0),
            cost=raw.get("cost", 0.0),
            error=raw.get("error"),
        )
        results.append(result)

        if passed:
            total_passed += 1
        total_latency += result.latency_ms
        total_tokens += result.tokens_used
        total_cost += result.cost

    total = len(results)
    return EvalSummary(
        total=total,
        passed=total_passed,
        failed=total - total_passed,
        accuracy=total_passed / total if total > 0 else 0.0,
        avg_latency_ms=total_latency / total if total > 0 else 0.0,
        total_tokens=total_tokens,
        total_cost=total_cost,
        results=results,
    )


def exact_match(actual: Any, expected: Any) -> float:
    """Exact match scorer."""
    return 1.0 if actual == expected else 0.0


def contains_match(actual: Any, expected: Any) -> float:
    """Check if expected is contained in actual."""
    if isinstance(actual, str) and isinstance(expected, str):
        return 1.0 if expected.lower() in actual.lower() else 0.0
    return 0.0


def numeric_match(actual: Any, expected: Any, tolerance: float = 0.01) -> float:
    """Numeric match with tolerance."""
    try:
        actual_num = float(actual)
        expected_num = float(expected)
        diff = abs(actual_num - expected_num)
        return 1.0 if diff <= tolerance else 0.0
    except (ValueError, TypeError):
        return 0.0
