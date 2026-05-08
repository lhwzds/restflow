---
title: skrun Examples
covers:
  - examples/skrun/**
  - crates/tools/src/impls/skrun.rs
---

# skrun Examples

RestFlow core calls executable skills through `run_skill`. External tools should
be packaged for skrun instead of being registered as core Rust tools.

Reference examples live under `examples/skrun/`:

- `python-exec`
- `http-request`
- `web-fetch`
- `web-search`
- `email-send`
- `telegram-send`
- `discord-send`
- `slack-send`
- `browser-automation`
- `transcribe`
- `vision`
- `memory-file`

Each example uses a JSON input and emits a JSON object. Missing credentials,
commands, or Python packages return structured errors.

## Example Invocation

```bash
python examples/skrun/python-exec/run.py '{"code":"print(1 + 1)"}'
```

Through RestFlow, the agent should call:

```json
{
  "id": "python-exec",
  "input": {
    "code": "print(1 + 1)"
  }
}
```

The concrete installation format is owned by skrun. RestFlow embeds the `skrun`
crate and reads installed artifacts from the skrun skills directory, so runtime
execution does not require a separate `skrun` executable on `PATH`.
