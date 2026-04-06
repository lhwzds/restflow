# Task / Run Domain Model

This document defines the canonical naming boundary for RestFlow.

## Core Principle

RestFlow uses one internal execution model:

- `Agent`: capability, identity, role, and configuration
- `Task`: a schedulable unit of work assigned to an agent
- `Run`: one execution of a task
- `Sub-agent`: delegated execution spawned within a run

`Agent` is not a task state model.
`Task` is not an agent capability definition.

## Layering Rules

### Execution Ownership

- `restflow-ai` owns subagent runtime capability and lifecycle.
- `restflow-core` owns durable background/task runtime and daemon-side execution orchestration.
- `restflow-core::runtime::subagent` is adapter-only and should not grow a second subagent runtime owner surface.
- `restflow-tools` owns tool surface and team/template adapters, not runtime ownership.

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

Legacy background-agent and execution-session names are only allowed at:

- request ingress compatibility
- short-term wire compatibility
- explicit migration tests

### CLI and Browser API

CLI commands, daemon request wrappers, stores, stream state, and route parameters must prefer:

- `task_id`
- `run_id`
- `Task*`
- `Run*`

Compatibility wrappers may exist temporarily, but they must be thin aliases around the canonical surface.

### User-Facing UI Copy

User-facing copy may use:

- `Agent`
- `Sub-agent`
- `Run`

These terms are presentation vocabulary only. They must not create a second state model in the UI layer.

## Export Policy

Shared public exports should expose canonical names only.

Legacy names should not remain as primary public exports from `restflow-core`.
If compatibility is still required, keep it as:

- crate-private aliases for internal migration
- deprecated wrapper files in the browser client
- ingress-only compatibility in transport parsing

## Migration Guardrails

When changing execution-related code:

1. Introduce or consume canonical `Task / Run` types first.
2. Keep legacy names out of new APIs and new shared exports.
3. Restrict compatibility to explicit wrappers or ingress normalization.
4. Prefer deleting wrapper logic instead of maintaining dual business paths.
