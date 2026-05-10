---
title: Task and Run Domain Model
covers:
  - crates/types/src/lib.rs
  - crates/core/src/lib.rs
  - crates/runner/src/lib.rs
  - crates/daemon/src/lib.rs
  - crates/tui/src/lib.rs
---

# Task / Run Domain Model

This document defines the canonical naming boundary for RestFlow.

## Core Principle

RestFlow keeps `Task / Run` as the background execution vocabulary:

- `Agent`: capability, identity, role, and configuration
- `Task`: a durable background goal or trigger assigned to an agent
- `Run`: one execution segment recorded against a session
- `Sub-agent`: delegated ephemeral child run spawned within a parent run

`Agent` is not a task state model.
`Task` is not an agent capability definition.

The durable/ephemeral split belongs to the envelope around a run:

- Background task: durable goal/trigger metadata plus repeated run summaries.
- Foreground TUI turn: session-local run state with direct user steering.
- Sub-agent: ephemeral child run, `parent_run_id`, and no separate task record.

TUI command overlays may show background tasks and sub-agent child runs
together for navigation, but storage and daemon state must keep the durable task
envelope separate from ephemeral child-run execution. During an active agent
turn, their running state belongs in the message panel as transient activity,
not as a separate durable UI state model.

## Layering Rules

### Execution Ownership

- `types` owns shared run and sub-agent data shapes.
- `agent` owns the core agent loop and sub-agent runtime primitives.
- `tools` owns callable tool implementations only.
- `runner` binds agents, tools, sessions, and sub-agent managers into execution.
- `daemon` hosts background work; it should not become the foreground TUI runtime.
- `tui` renders current turn activity and session history.
- Team-style coordination is skrun skill guidance, not a saved product object or reusable template.

### Core, Contracts, Runtime, Storage Adapters

These layers must use canonical run terms:

- `RunSummary`
- `RunListQuery`
- `RunKind`
- `RunTimeline`
- `ExecutionThread`

Non-canonical execution names must not appear in task/run runtime, storage, or
transport surfaces.

### CLI and Browser API

CLI commands, daemon request wrappers, stores, stream state, and route parameters must prefer:

- `run_id`
- `Run*`

Compatibility wrappers should be removed instead of extending dual task/run surfaces.

### User-Facing UI Copy

User-facing copy may use:

- `Agent`
- `Task`
- `Sub-agent`
- `Run`

These terms are presentation vocabulary only. They must not create a second state model in the UI layer.

## Export Policy

Shared public exports should expose canonical names only.

Legacy names should not remain as public exports from `runtime`.

## Migration Guardrails

When changing execution-related code:

1. Introduce or consume canonical `Task / Run` types first.
2. Keep legacy names out of new APIs and new shared exports.
3. Prefer deleting wrapper logic instead of maintaining dual business paths.
