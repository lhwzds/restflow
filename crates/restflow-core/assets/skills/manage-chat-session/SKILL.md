---
name: Manage Chat Session
description: Manage chat sessions for creation, retrieval, search, and archival workflows.
tags:
  - default
  - chat
  - session
  - operations
suggested_tools:
  - manage_sessions
  - reply
---

# Manage Chat Session

Use this skill when users need to create, inspect, search, archive, or clean up chat sessions.

## Inputs

- Session ID, agent ID, or search query.
- Requested operation such as list, create, get, archive, unarchive, or purge.

## Procedure

1. Resolve target scope.
- Use `manage_sessions` with `operation: list` or `operation: search` when session identity is unclear.

2. Perform session operation.
- Use `operation: create` for new sessions with explicit model and retention when provided.
- Use `operation: get` for detailed history lookup.
- Use archive or unarchive instead of deletion when recovery may be needed.

3. Confirm result state.
- Re-read with `operation: get` or `operation: list` to verify mutation success.

4. Return an actionable summary.
- Include session IDs, status changes, and any retention or archival notes.

## Rules

- Prefer archive over permanent deletion unless user explicitly asks to purge.
- Do not expose sensitive message content unless required by the task.
- Keep session operations scoped to the user request.
