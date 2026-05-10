---
title: runner
covers:
  - crates/runner/Cargo.toml
  - crates/runner/src/lib.rs
---

# runner

`runner` binds agents, tools, sessions, and services into executable turns.

## Responsibilities

- Build tool registries for an agent run.
- Resolve effective tools and skill activation for a turn.
- Execute foreground and background session turns.
- Bridge sub-agent definitions from storage into the agent runtime.
- Persist turn results through `core` services.

## Boundaries

- `agent` owns the inner agent loop.
- `tools` owns tool implementations.
- `daemon` hosts background execution.
- `tui` renders foreground interaction.
