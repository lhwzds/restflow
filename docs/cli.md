---
title: cli
covers:
  - crates/cli/Cargo.toml
  - crates/cli/build.rs
  - crates/cli/src/main.rs
  - crates/cli/man/*.1
---

# cli

`cli` builds the `restflow` binary.

## Responsibilities

- Command parsing and command dispatch.
- Daemon lifecycle commands.
- Agent, session, skill, secret, config, and maintenance commands.
- Shell completions and man page generation.
- TUI launch path.

## Boundaries

- Command handlers should delegate runtime work to `daemon`, `runner`, `core`,
  and `tui`.
- CLI output should stay thin and operational.
