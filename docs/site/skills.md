---
title: Skills
description: RestFlow skill loading and executable skrun model.
---

# Skills

RestFlow uses skills to expose focused guidance and executable capabilities to
agents. The runtime-visible skill catalog is system skills plus skrun skills.

## Skill Sources

- `system`: built-in guidance.
- `external`: installed executable skills discovered through skrun.

## TUI Usage

- `/skill` views installed skills.
- `@skill` selects a skill for the current turn.
- Natural language can still trigger assigned skill guidance when the runtime
  authorizes it.

## System Skills

## Runtime Tools

- `load_skill` lists skills or reads a skill by ID. It does not execute skills.
- `run_skill` executes an installed skrun skill by ID with JSON input.

Team and parallel-agent behavior should live as system skill guidance until the
single-agent framework boundary is stable.
