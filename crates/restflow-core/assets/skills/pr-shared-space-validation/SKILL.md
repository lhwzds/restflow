---
name: PR KV Store Validation
description: Validate kv_store PR artifacts and block risky content before submission
tags:
  - default
  - pr
  - validation
suggested_tools:
  - kv_store
---

# PR KV Store Validation

Use this skill before PR submission.

## Inputs
- `task_id` (required)

## Required Keys
- `pr:{task_id}:title`
- `pr:{task_id}:body`

## Validation Checklist
1. Required keys exist.
2. Title and body are not empty.
3. Body does not contain secret-like patterns.
4. Key prefix strictly follows `pr:{task_id}:...`.

## Secret-like Pattern Rules
Block submission if title/body matches any of:
- `AKIA[0-9A-Z]{16}`
- `ghp_[A-Za-z0-9]{36,}`
- `sk-[A-Za-z0-9]{20,}`
- `-----BEGIN (RSA|EC|OPENSSH|PGP) PRIVATE KEY-----`

## Output Contract
Write a structured JSON report to `pr:{task_id}:checks`:

```json
{
  "task_id": "<task_id>",
  "ok": true,
  "missing_keys": [],
  "secret_hits": [],
  "checked_at": "<iso-8601>"
}
```

If `ok` is false, stop and do not proceed to PR submission.
