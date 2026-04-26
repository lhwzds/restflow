---
name: Team
description: Coordinate short-lived parallel subagents through spawn_subagent_batch and saved team templates.
tags:
  - system
  - team
  - subagent
  - coordination
suggested_tools:
  - spawn_subagent_batch
  - wait_subagents
  - list_subagents
---

# Team

Use this systemskill when the user asks for a team, parallel review, fan-out planning, or coordinated subagent execution.

## Procedure

1. Prefer `spawn_subagent_batch` for multi-agent work.
- Use a direct `workers` list for one-off execution.
- Use a saved `team` template only when the user wants reusable structure.
- Use `preview: true` before creating or reusing a saved team if the operation is broad or risky.

2. Keep saved teams structural.
- Save agent/model/count/tool-shape preferences.
- Do not save the current task prompt as reusable team structure.
- Do not create a long-lived team runtime.

3. Collect and merge results.
- Wait for spawned subagents when the user needs an answer in the current turn.
- Merge conclusions into one user-facing response.
- Mention failed or timed-out subagents only when they affect confidence.

## Rules

- `spawn_subagent_batch` is the only team execution primitive.
- Team templates are reusable configuration, not task/run history.
- Use tasks only when the work must continue after the current conversation.
