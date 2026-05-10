---
title: llm
covers:
  - crates/llm/Cargo.toml
  - crates/llm/src/lib.rs
---

# llm

`llm` owns model clients and streaming abstractions.

## Responsibilities

- HTTP model clients and CLI-backed model adapters.
- Streaming response normalization.
- Retry, switching, and model client construction.
- Test utilities for model-facing code.

## Boundaries

- Agent loop policy belongs in `agent`.
- Shared model IDs and provider contracts belong in `types`.
- User-facing command wiring belongs in `cli` and `tui`.
