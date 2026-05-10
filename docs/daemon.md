---
title: daemon
covers:
  - crates/daemon/Cargo.toml
  - crates/daemon/src/lib.rs
---

# daemon

`daemon` owns hosted background work and process services.

## Responsibilities

- Daemon lifecycle and launcher behavior.
- IPC protocol and request dispatch.
- Background run hosting.
- Stream forwarding for daemon-managed work.
- MCP and process-level integration surfaces.

## Boundaries

- The foreground TUI should not depend on daemon-hosted execution for normal
  chat turns.
- Shared request/response types belong in `types`.
- Turn execution logic belongs in `runner`.
