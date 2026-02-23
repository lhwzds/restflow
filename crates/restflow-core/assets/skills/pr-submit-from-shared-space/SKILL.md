---
name: PR Submit From KV Store
description: Create GitHub pull requests from kv_store draft artifacts using body files only
tags:
  - default
  - pr
  - kv-store
suggested_tools:
  - kv_store
  - bash
  - file
---

# PR Submit From KV Store

Use this skill to submit a pull request from artifacts stored in `kv_store`.

## Inputs
- `task_id` (required)
- `base` (optional)
- `head` (optional)

## Data Contract
Read these keys first:
- `pr:{task_id}:title` (text/plain)
- `pr:{task_id}:body` (text/markdown)
- `pr:{task_id}:checks` (application/json, optional)

Fail fast if `title` or `body` is missing or empty.

## Procedure
1. Read required keys from `kv_store`.
2. Write title/body to local temp files.
3. Build command with `gh pr create --title "$(cat title_file)" --body-file body_file`.
4. If `base` is set, append `--base`.
5. If `head` is set, append `--head`.
6. Execute command with `bash`.
7. Parse output and write result to `pr:{task_id}:result` as JSON.

## Hard Rules
- Never use inline `gh pr create --body "..."`.
- Always use `--body-file`.
- Keep `work_items` limited to lifecycle metadata and the shared key prefix.

## Result Shape
Store this JSON to `pr:{task_id}:result`:

```json
{
  "task_id": "<task_id>",
  "url": "<pr_url>",
  "number": 123,
  "head": "feature/...",
  "base": "main",
  "status": "created"
}
```
