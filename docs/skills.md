---
title: Skills
covers:
  - crates/restflow-core/src/loader/**/*.rs
  - crates/restflow-core/src/registry/**/*.rs
  - crates/restflow-core/src/services/skill*.rs
  - crates/restflow-tools/src/skill/**/*.rs
  - crates/restflow-tools/src/impls/load_skill.rs
  - crates/restflow-tools/src/impls/skrun.rs
---

# Skills

RestFlow uses skills to expose focused guidance and executable capabilities to
agents. The runtime-visible skill catalog is discovered through skrun.

## Skill Sources

- `external`: installed skrun skills, including md-only guidance skills and executable skills.

## TUI Usage

- `/skill` views installed skills.
- `@skill` selects a skill for the current turn.
- Natural language can still trigger assigned skill guidance when the runtime
  authorizes it.

## Runtime Tools

- `load_skill` lists skills or reads a skill by ID. It does not execute skills.
- `run_skill` executes an installed skrun skill by ID with JSON input.

Team and parallel-agent behavior should live as md-only skrun guidance until
the single-agent framework boundary is stable.
