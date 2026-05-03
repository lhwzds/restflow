---
title: Agent Framework
covers:
  - crates/restflow-ai/**/*.rs
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

`RunEvent` represents runtime events only. It should not embed trace payloads.
Trace records can be derived from runtime events or exported through a separate
adapter.

## Python Integration

Python should integrate through explicit boundaries:

- a Python SDK can create and run RestFlow agents;
- Python-defined tools can be called through a tool host;
- generic `run_python` behavior belongs in an external skrun example.

This keeps the framework focused on agent orchestration rather than owning every
language runtime.
