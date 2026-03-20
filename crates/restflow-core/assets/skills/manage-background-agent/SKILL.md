---
name: Manage Background Agent
description: Manage background agent lifecycle, execution, progress inspection, and operator messaging.
tags:
  - default
  - agent
  - background
  - operations
suggested_tools:
  - manage_background_agents
  - reply
---

# Manage Background Agent

Use this skill for long-running or scheduled work executed by background agents.

## Inputs

- Task intent or agent ID.
- Optional run configuration such as timeout, schedule, and memory scope.

## Procedure

1. Inspect existing background agents.
- Use `manage_background_agents` with `operation: list` first.
- Reuse an existing agent when possible.

2. Create or run as needed.
- For new work, create an agent definition.
- Trigger execution with `operation: run`.
- Before `create`, `convert_session`, `promote_to_background`, `run`, `run_batch`, or `control` with `run_now`, call `manage_background_agents` with `preview: true`.
- If preview returns warnings, summarize them and wait for user confirmation before retrying with `confirmation_token`.
- If preview returns blockers, stop and report the blockers.

3. Track health and progress.
- Query `operation: progress` for recent events.
- Use `operation: list_deliverables` for outputs.
- Use `operation: list_messages` to inspect runtime conversation if needed.

4. Operate safely.
- Pause, resume, or stop only for explicit operational reasons.
- Send user messages through `operation: send_message` when interaction is required.

## Rules

- Prefer `run` on an existing definition over creating duplicates.
- Keep lifecycle transitions explicit and auditable.
- Do not delete agents unless explicitly requested.
