---
title: RestFlow Documentation
covers:
  - README.md
  - Cargo.toml
  - Cargo.lock
  - Makefile
  - examples/skrun/**/*.py
  - homebrew-tap-template/**/*.md
  - homebrew-tap-template/**/*.rb
  - homebrew-tap-template/**/*.yml
  - npm/scripts/**/*.js
---

# RestFlow Documentation

RestFlow is a local agent CLI with a terminal-first interface and executable
skills. The docs are organized around the current Rust workspace crates.

The repository does not use Docker and does not maintain a separate frontend
application. The primary product surface is the `restflow` CLI and TUI.

## Crate Map

- [types](./types.md): shared public types, contracts, traits, and model catalog.
- [llm](./llm.md): model clients and streaming abstractions.
- [agent](./agent.md): agent loop, context management, steering, and sub-agent runtime.
- [tools](./tools.md): built-in local tool implementations and skill tools.
- [core](./core.md): local file-backed state, secrets, services, and app core.
- [runner](./runner.md): foreground/background turn orchestration.
- [daemon](./daemon.md): background hosting, IPC, launcher, and daemon services.
- [tui](./tui.md): terminal UI rendering and interaction state.
- [cli](./cli.md): `restflow` command-line entrypoint.

## Product Boundary

- RestFlow owns local agent execution, a terminal interface, model configuration,
  secrets, sessions, and executable skill calls.
- External capabilities should be packaged as skrun-compatible skills instead
  of growing the core tool surface.
- `daemon` is for hosted background work. Normal interactive chat belongs to
  the foreground TUI and runner path.
- Redb is reserved for secrets; sessions and agent configuration are
  file-backed.

## Publishing Boundary

This repository owns the Markdown source in `docs/`. Website publishing belongs
to the external Starlight website repository, which should fetch these Markdown
files at a configured Git ref and build the public documentation site.
