---
title: Task and Run Domain Model
covers:
  - crates/runtime/src/models/task_runtime.rs
  - crates/runtime/src/runtime/task_runtime/**/*.rs
  - crates/runtime/src/storage/task_runtime/**/*.rs
  - crates/runtime/src/mcp/server/tasks.rs
  - crates/tools/src/impls/task/**/*.rs
---

# Task / Run Domain Model

This document defines the canonical naming boundary for RestFlow.

## Core Principle

RestFlow keeps `Task / Run` as the background-runtime domain model:

- `Agent`: capability, identity, role, and configuration
- `Task`: a schedulable unit of work assigned to an agent
- `Run`: one agent execution
- `Sub-agent`: delegated ephemeral child run spawned within a parent run

`Agent` is not a task state model.
`Task` is not an agent capability definition.

The durable/ephemeral split belongs to the envelope around a run:

- Background task: durable `TaskSpec`, schedule, controls, and repeated runs.
- Sub-agent: ephemeral child run, `parent_run_id`, and no task storage row.

TUI command overlays may show background tasks and sub-agent child runs
together for navigation, but storage and daemon state must keep the durable task
envelope separate from ephemeral child-run execution. During an active agent
turn, their running state belongs in the message panel as transient activity,
not as a separate durable UI state model.

## Layering Rules

### Execution Ownership

- `ai` owns subagent runtime capability and lifecycle.
- `runtime` owns durable background/task runtime and daemon-side execution orchestration.
- `runtime::runtime::subagent` is adapter-only and should not grow a second subagent runtime owner surface.
- `tools` owns tool surfaces only, not runtime ownership.
- Team-style coordination is skrun skill guidance, not a saved product object or reusable template.
- `spawn_subagent_batch` is the only team-style execution primitive. Durable work must use Task/Run history instead of separate team runtime state, mailbox, assignment state, or approval state.

### Core, Contracts, Runtime, Storage Adapters

These layers must use canonical task/run terms:

- `Task`
- `TaskSpec`
- `TaskPatch`
- `TaskStatus`
- `TaskMessage`
- `TaskProgress`
- `TaskControlAction`
- `RunSummary`
- `RunListQuery`
- `ChildRunListQuery`

Non-canonical execution names must not appear in task/run runtime, storage, or
transport surfaces.

### CLI and Browser API

CLI commands, daemon request wrappers, stores, stream state, and route parameters must prefer:

- `task_id`
- `run_id`
- `Task*`
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
