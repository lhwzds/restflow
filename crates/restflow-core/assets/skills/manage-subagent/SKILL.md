---
name: Manage Subagent
description: Manage subagent discovery, execution, and result collection with safe coordination.
tags:
  - default
  - agent
  - subagent
  - operations
suggested_tools:
  - list_subagents
  - spawn_subagent
  - wait_subagents
  - reply
---

# Manage Subagent

Use this skill when a task should be split into one or more specialized subagent runs.

## Inputs

- Goal statement for the delegated task.
- Optional agent selector, model override, and timeout.

## Procedure

1. Discover available subagents.
- Use `list_subagents` first.
- Pick the smallest capable agent for the task.

2. Spawn subagents with explicit task boundaries.
- Use `spawn_subagent` with a clear, testable task prompt.
- Use `spawn_subagent` `workers` and `team` fields when you need model/count fan-out or saved team presets.
- Prefer a single subagent unless parallel execution is clearly beneficial.

3. Wait and collect results.
- Use `wait_subagents` with all spawned task IDs.
- Aggregate outputs before replying.

4. Report execution outcome.
- Include selected agent, task IDs, and success or failure state.
- Include unresolved risks if any subagent timed out or failed.

## Rules

- Do not spawn duplicate subagents for identical work.
- Keep delegated scope narrow and avoid hidden assumptions.
- Return merged, user-facing conclusions rather than raw fragments.
