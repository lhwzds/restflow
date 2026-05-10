---
title: TUI
covers:
  - crates/tui/**/*.rs
  - crates/cli/src/main.rs
  - crates/runner/src/lib.rs
  - crates/daemon/src/lib.rs
---

# TUI

The RestFlow TUI is the default interactive entrypoint. It is designed around a
conversation transcript, a fixed composer, temporary overlays, and foreground
runner streaming.

## Interaction Model

- Normal text sends a message to the active session.
- `/` opens command selection.
- `@` opens skill selection.
- `Esc` cancels active input or interrupts an active stream.
- `Ctrl-C` exits the TUI.

## Rendering Model

The transcript is persistent conversation history. The composer and overlays
are temporary UI. Overlay state should not be appended to conversation history.

## Runtime Boundary

The TUI owns foreground interaction and should not depend on the daemon for
normal chat turns. The daemon is reserved for hosted background work and
process-level management.
