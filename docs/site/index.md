---
title: RestFlow
description: Local agent framework and terminal UI for skill-based AI work.
---

# RestFlow

RestFlow is a local AI agent framework with a terminal-first UI. The core
runtime owns agent execution, a small tool surface, skill discovery, and
executable skill runs through skrun.

## What RestFlow Owns

- Agent runtime orchestration.
- Minimal coding tools: shell, file, edit, patch, glob, and grep.
- `load_skill` for skill discovery and guidance loading.
- `run_skill` for executable skrun skills.
- TUI and client surfaces that consume runtime events.

## Quick Start

```bash
brew install lhwzds/tap/restflow
```

```bash
restflow
```

Use the TUI for local agent work and `@skill` to select skill guidance for a
turn.

## Documentation Map

- [TUI](./tui.md) covers the terminal interface.
- [Skills](./skills.md) covers skill activation and system skills.
- [Agent Framework](./agent-framework.md) covers the simplified core runtime.
- [skrun Examples](./skrun-examples.md) covers external tool migration.
- [Task and Run Model](./task-run-domain-model.md) covers durable task naming.
