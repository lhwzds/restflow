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
  - spawn_subagent_batch
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
- Use `spawn_subagent_batch` when you need model/count fan-out.
- Load the `team` systemskill when the user asks for coordinated parallel agent work.
- Prefer a single subagent unless parallel execution is clearly beneficial.
- Before spawning a broad batch, call `spawn_subagent_batch` with `preview: true`.
- If preview returns warnings, summarize them and wait for user confirmation before retrying with `approval_id`.
- If preview returns blockers, stop and report the blockers instead of partially spawning work.

Example: run a mixed-provider planning batch.
```json
{
  "task": "Create implementation plans for pending features",
  "wait": true,
  "workers": [
    {
      "agent": "coder",
      "count": 20,
      "model": "minimax/coding-plan",
      "provider": "minimax"
    },
    {
      "agent": "coder",
      "count": 3,
      "model": "glm5/coding-plan",
      "provider": "glm5"
    }
  ]
}
```

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
