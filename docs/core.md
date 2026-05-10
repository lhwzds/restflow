---
title: core
covers:
  - crates/core/Cargo.toml
  - crates/core/src/lib.rs
---

# core

`core` owns local application state and service implementations.

## Responsibilities

- AppCore construction and service access.
- File-backed agents, sessions, config, and skill catalog integration.
- Secret storage and redb-backed secret access.
- Auth profile handling, model metadata, maintenance, and local state helpers.
- Session import and session JSONL persistence.

## Boundaries

- `core` should not own the agent loop.
- `core` should not own UI rendering.
- Redb usage should stay limited to secrets; other state should remain
  file-backed.
