---
name: RestFlow Self-Heal Ops
description: Diagnose and repair common runtime failures with evidence-first output.
tags:
  - default
  - ops
  - self-heal
suggested_tools:
  - manage_background_agents
  - manage_agents
  - manage_sessions
  - bash
  - file
  - diagnostics
  - process
  - reply
---

# RestFlow Self-Heal Ops

Use this skill when users report issues like:
- "agent does not reply"
- "model not specified"
- "daemon is running but tasks fail"
- "background agent finished but no notification"

## Operating Procedure

1. Collect evidence first.
- Capture exact error strings from logs and task progress.
- Record affected agent IDs, background agent IDs, and session IDs.

2. Classify the failure.
- Model configuration issue.
- Execution mode mismatch.
- Runtime/daemon unhealthy.
- Notification delivery issue.
- Input/template mismatch.

3. Apply the minimum safe fix.
- Update only the required fields.
- Avoid destructive actions.
- Keep user data and history unchanged.

4. Verify immediately.
- Trigger one minimal run for verification.
- Confirm status transition and absence of previous error.

## Response Contract

Always respond with three sections:

## Evidence
- Exact logs and IDs.

## Operation
- Exact actions taken in order.

## Verification
- What was checked and current status.
- Remaining risk, if any.
