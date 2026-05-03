---
title: Agent Framework
covers:
  - crates/restflow-ai/Cargo.toml
  - crates/restflow-ai/README.md
  - crates/restflow-ai/pyproject.toml
  - crates/restflow-ai/**/*.rs
  - python/restflow_ai/**/*.py
  - crates/restflow-core/src/runtime/agent/**/*.rs
  - crates/restflow-core/src/runtime/orchestrator/**/*.rs
  - crates/restflow-core/src/services/tool_registry/**/*.rs
  - crates/restflow-tools/src/**/*.rs
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

Client-visible task and session stream events live in `restflow-contracts`.
They should describe realtime UI/runtime state only and should not embed trace
payloads. Trace records can be derived from runtime events or exported through a
separate adapter.

The shared telemetry event domain lives in `restflow-ai`; `restflow-core`
projects those events into daemon-owned persistence.

## Python Integration

Python should integrate through explicit boundaries:

- `restflow-ai` exposes a PyO3 native module for agent-facing SDK primitives;
- a Python SDK can create and run RestFlow agents through that native module;
- Python-defined tools can be called through a tool host;
- generic `run_python` behavior belongs in an external skrun example.

This keeps the framework focused on agent orchestration rather than owning every
language runtime.
