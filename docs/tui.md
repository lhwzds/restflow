---
title: TUI
covers:
  - crates/tui/Cargo.toml
  - crates/tui/src/lib.rs
---

# tui

`tui` is the terminal interface crate. It owns the interactive transcript,
composer, overlays, command palette, model/skill selectors, and streaming
render state.

## Responsibilities

- Render session history and the active turn.
- Keep tool and sub-agent activity visible in the message panel.
- Route slash commands and composer input to the CLI/daemon client surface.
- Preserve UI state separately from persisted transcript state.

## Boundaries

- `runner` owns actual agent execution.
- `daemon` owns background hosting.
- `types` owns transcript, run, model, and skill data shapes shared with other
  crates.
