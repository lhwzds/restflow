---
title: Task and Run Domain Model
description: Canonical Task and Run naming for RestFlow.
---

# Task and Run Domain Model

RestFlow uses `Task` for durable background work and `Run` for each execution
attempt. This naming is canonical across CLI, TUI, daemon IPC, HTTP, and web
surfaces.

## Core Terms

- `Task`: user-visible durable work item.
- `Run`: one execution attempt for a task.
- `RunArtifact`: structured output produced by a run.

## Rules

- Do not reintroduce background-agent names in public APIs.
- Do not use session records as durable task records.
- Keep task/run state transitions owned by the daemon runtime.

See the repository-level domain note for the full model.
