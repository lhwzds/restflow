---
title: TUI
description: RestFlow terminal UI model.
---

# TUI

The RestFlow TUI is the default interactive entrypoint. It is designed around a
conversation transcript, a fixed composer, temporary overlays, and daemon-backed
streaming.

## Interaction Model

- Normal text sends a message to the active session.
- `/` opens command selection.
- `@` opens skill selection.
- `Esc` cancels active input or interrupts an active stream.
- `Ctrl-C` exits the TUI.

## Rendering Model

The transcript is persistent conversation history. The composer and overlays are
temporary UI. Overlay state should not be appended to conversation history.

## Daemon Boundary

The TUI talks to the local daemon. If the daemon is unavailable, the UI should
show a clear offline state and allow `/daemon` to start or stop it.
