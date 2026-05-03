---
title: RestFlow Documentation
covers:
  - README.md
  - SYSTEM_ARCHITECTURE.md
  - Cargo.toml
  - Cargo.lock
  - Dockerfile
  - Makefile
  - docker-compose.dev.yml
  - crates/**/*.rs
  - examples/skrun/**/*.py
  - npm/scripts/**/*.js
  - scripts/**/*.sh
---

# RestFlow Documentation

RestFlow is a local AI agent framework with a terminal-first interface. The
runtime owns agent execution, a small built-in tool surface, skill discovery,
and executable skill calls through skrun.

The Rust workspace is narrowed to the runtime crates that still define product
boundaries: shared contracts/types, storage, AI execution, tools, core daemon
runtime, CLI, and TUI.

## What RestFlow Owns

- Agent runtime orchestration.
- Minimal coding tools: shell, file, edit, patch, glob, and grep.
- `load_skill` for skill discovery and guidance loading.
- `run_skill` for executable skrun skills.
- TUI and client surfaces that consume runtime events.

## Documentation Map

- [Agent Framework](./agent-framework.md) covers the simplified runtime.
- [Skills](./skills.md) covers skill loading and executable skrun skills.
- [skrun Examples](./skrun-examples.md) covers external tool migration.
- [TUI](./tui.md) covers the terminal interface.
- [Task and Run Domain Model](./TASK_RUN_DOMAIN_MODEL.md) covers durable task
  naming.

## Publishing Boundary

This repository owns the Markdown source in `docs/`. Website publishing belongs
to the external Starlight website repository, which should fetch these Markdown
files at a configured Git ref and build the public documentation site.
