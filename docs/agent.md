---
title: agent
covers:
  - crates/agent/Cargo.toml
  - crates/agent/README.md
  - crates/agent/src/lib.rs
---

# agent

`agent` owns the core agent runtime.

## Responsibilities

- ReAct turn execution.
- Context loading, compaction, pruning, and prompt preparation.
- Steering and deferred user input handling.
- Sub-agent runtime primitives and tracking.
- Agent-side cache helpers and execution state.

## Boundaries

- Tool implementations live in `tools`.
- Model client implementations live in `llm`.
- File-backed state and app services live in `core`.
- Foreground/background orchestration belongs in `runner`.
