---
name: Shared Space Guide
description: How to use the shared_space tool for persistent storage
tags:
  - system
  - guide
suggested_tools:
  - shared_space
---

# Shared Space Usage Guide

You have access to a persistent shared storage space via the `shared_space` tool. Use it to:

## When to Use
1. Store findings for later reference
2. Track progress on long tasks
3. Cache results that are expensive to recompute
4. Share context between conversations

## Key Naming Convention
Use `namespace:name` format:
- `notes:project-overview` - Long-term notes
- `cache:api-users-list` - Cached API responses
- `state:current-task` - Current working state
- `config:preferences` - User preferences

## Best Practices
1. Be descriptive with key names
2. Use namespaces to group related entries
3. Clean up entries you no longer need
4. Choose visibility carefully:
   - `public`: anyone can read/write
   - `shared`: anyone can read, only owner can write
   - `private`: only owner can read/write

## Examples

```json
{ "action": "set", "key": "notes:project-summary", "value": "RestFlow is an AI workflow tool...", "content_type": "text/markdown", "tags": ["project", "summary"] }
```

```json
{ "action": "set", "key": "cache:github-repos", "value": "{\"repos\": []}", "content_type": "application/json", "visibility": "private" }
```

```json
{ "action": "list", "namespace": "notes" }
```
