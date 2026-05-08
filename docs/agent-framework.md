---
title: Agent Framework
covers:
  - crates/ai/Cargo.toml
  - crates/ai/README.md
  - crates/ai/**/*.rs
  - crates/runtime/src/runtime/agent/**/*.rs
  - crates/runtime/src/runtime/orchestrator/**/*.rs
  - crates/runtime/src/services/tool_registry/**/*.rs
  - crates/tools/src/**/*.rs
---

# Agent Framework

RestFlow's core framework is intentionally small. The runtime executes an
agent, exposes a minimal coding toolset, loads skill guidance, and delegates
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

Client-visible task and session stream events live in `types`.
They should describe realtime UI/runtime state only and should not embed trace
payloads. Trace records can be derived from runtime events or exported through a
separate adapter.

The shared telemetry event domain lives in `ai`; `runtime`
projects those events into daemon-owned persistence.

## Python Integration

Python integration stays outside the core runtime:

- generic `run_python` behavior belongs in an external skrun example;
- Python-defined tools should be exposed through skrun-compatible commands;
- RestFlow should not own a Python package, PyO3 native module, or embedded
  Python SDK boundary.

This keeps the framework focused on agent orchestration rather than owning every
language runtime.
