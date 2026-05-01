# RestFlow AI

`restflow-ai` is the Python SDK for the RestFlow executable skill runtime. The
distribution name is `restflow-ai`; the import name is `restflow`.

RestFlow is not a replacement for LangChain, LangGraph, PydanticAI, OpenAI
Agents, or other main agent frameworks. It provides a local runtime for
building, installing, and calling executable skills from those frameworks.

## Install

```bash
pip install restflow-ai
```

## Use An Installed Skill

```python
import restflow

result = restflow.skill("regex-finder").call(
    {
        "pattern": "TODO",
        "path": ".",
    }
)
```

Skill IDs resolve under `~/.restflow/skills` by default. Set
`RESTFLOW_SKILLS_DIR` to use a different local skill directory.

## Artifact Kinds

The first executable skill runtime supports:

- `rust_binary`: Cargo-built executable skills called with stdin/stdout JSON.
- `python_uv`: uv-managed Python skills called with stdin/stdout JSON.

## Agent Framework Integration

Wrap `restflow.skill(...).call(...)` in the tool abstraction of your existing
agent framework. The RestFlow repository includes dependency-light examples for
LangChain, LangGraph, OpenAI Agents-style function tools, and PydanticAI-style
tool bodies.
