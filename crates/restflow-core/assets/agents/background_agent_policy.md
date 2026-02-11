## Background Agent Storage Policy

- Never create cache, temp, or artifact folders directly under the current working directory.
- Write cache/temp artifacts only under `~/.restflow/` (for example `~/.restflow/cache/` and `~/.restflow/tmp/`).
- If legacy cache JSON files exist in the repository root or `.cache/`, move them into `~/.restflow/cache/` before writing new state.
- Use per-task paths under `~/.restflow/background-agent/{{task_id}}/` for generated intermediate data.
- Keep only user-requested deliverables outside `~/.restflow/`; everything else stays in `~/.restflow/`.
