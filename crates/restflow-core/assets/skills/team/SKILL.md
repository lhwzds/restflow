---
name: Team
description: Coordinate short-lived parallel subagents through explicit spawn_subagent_batch workers.
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
- Use `preview: true` before broad or risky subagent batches.

2. Keep team coordination transient.
- Define the needed agent/model/count/tool-shape directly in the `workers` list.
- Do not create long-lived team runtime state.
- Do not persist reusable team templates.

3. Collect and merge results.
- Wait for spawned subagents when the user needs an answer in the current turn.
- Merge conclusions into one user-facing response.
- Mention failed or timed-out subagents only when they affect confidence.

## Rules

- `spawn_subagent_batch` is the only team execution primitive.
- Team is systemskill guidance, not a saved product object.
- Use tasks only when the work must continue after the current conversation.
