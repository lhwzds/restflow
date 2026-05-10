---
title: Agent Framework
covers:
  - crates/types/src/lib.rs
  - crates/llm/src/lib.rs
  - crates/agent/src/lib.rs
  - crates/tools/src/**/*.rs
  - crates/core/src/lib.rs
  - crates/runner/src/lib.rs
  - crates/daemon/src/lib.rs
---

# Agent Framework

RestFlow's core framework is intentionally small. The runner executes an agent,
exposes a minimal coding toolset, loads skill guidance, and delegates
specialized capabilities to executable skrun skills.

## Core Runtime

The default agent surface is:

- `bash`
- `file`
- `edit`
- `multiedit`
- `patch`
- `glob`
- `grep`
- `load_skill`
- `run_skill`

`load_skill` is read-only. It lists skills and loads guidance. `run_skill`
executes installed skrun skills with JSON input.

RestFlow no longer owns the legacy binary skill build pipeline. Portable
executable capabilities should be installed and run through skrun.

## Outside the Core

These capabilities are not core tools:

- generic Python execution
- HTTP clients
- web search and web fetch
- browser automation
- image analysis
- audio transcription
- email and chat notifications
- memory stores beyond transcript and checkpoint adapters

They should live as external skrun skills or optional clients.

## Event Boundary

Client-visible session and run event types live in `types`.
They should describe realtime UI/runtime state only and should not embed trace
payloads. Trace records can be derived from runtime events or exported through a
separate adapter.

The foreground TUI and background daemon consume the same runner-level event
stream. Long-lived persistence belongs in session JSONL and file-backed state,
with redb reserved for secrets.

## Python Integration

Python integration stays outside the core runtime:

- generic `run_python` behavior belongs in an external skrun example;
- Python-defined tools should be exposed through skrun-compatible commands;
- RestFlow should not own a Python package, PyO3 native module, or embedded
  Python SDK boundary.

This keeps the framework focused on agent orchestration rather than owning every
language runtime.
