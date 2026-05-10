---
title: types
covers:
  - crates/types/Cargo.toml
  - crates/types/src/lib.rs
---

# types

`types` is the shared contract crate. It contains data structures and traits
that must be visible across the workspace.

## Responsibilities

- Agent, model, provider, skill, session, transcript, run, and sub-agent data
  types.
- Tool traits, tool errors, tool registry contracts, and toolset abstractions.
- Store traits for agents, sessions, secrets, and related services.
- Request/response contracts used by CLI, TUI, daemon, and runner.

## Boundaries

- Do not put runtime side effects here.
- Keep implementation-specific helpers in the owning crate.
- Public cross-crate shapes belong here when more than one crate must agree on
  serialization or behavior.
