---
name: Manage Agent
description: Manage agent definitions, inspect configuration, and apply minimal safe updates.
tags:
  - default
  - agent
  - configuration
  - operations
suggested_tools:
  - list_agents
  - get_agent
  - manage_agents
  - switch_model
  - reply
---

# Manage Agent

Use this skill for agent inventory, configuration updates, and controlled lifecycle changes.

## Inputs

- Agent ID or agent name.
- Requested operation such as create, inspect, or update.

## Procedure

1. Discover and identify target agents.
- Use `list_agents` to locate the correct target.
- Use `get_agent` for full configuration inspection before changes.

2. Apply minimal updates.
- Use `manage_agents` with the smallest required payload.
- For create or update, call `manage_agents` with `preview: true` first.
- If preview returns warnings, summarize them and wait for user confirmation before retrying with `confirmation_token`.
- If preview returns blockers, stop and report them instead of forcing the change.
- Preserve unrelated fields to avoid accidental behavior drift.

3. Validate runtime compatibility.
- If changing models, use `switch_model` only when required for the active execution context.
- Confirm the updated agent remains runnable.

4. Report what changed.
- Include agent ID, changed fields, and post-change checks.

## Rules

- Do not rewrite agent prompts or tools unless explicitly requested.
- Prefer update over delete-and-recreate.
- Keep auditability by reporting exact changed fields.
