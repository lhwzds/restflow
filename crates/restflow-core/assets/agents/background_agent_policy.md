## Background Agent Storage Policy

- Never create cache, temp, or artifact folders directly under the current working directory.
- Write cache/temp artifacts only under `~/.restflow/` (for example `~/.restflow/cache/` and `~/.restflow/tmp/`).
- If legacy cache JSON files exist in the repository root or `.cache/`, move them into `~/.restflow/cache/` before writing new state.
- Use per-task paths under `~/.restflow/background-agent/{{task_id}}/` for generated intermediate data.
- Keep only user-requested deliverables outside `~/.restflow/`; everything else stays in `~/.restflow/`.

## Background Agent Output Contract

- Always produce final output in three sections with exact headings:
  - `### Evidence`: concrete runtime facts (IDs, command output, API responses, errors).
  - `### Operation`: actions performed, including tools and major execution steps.
  - `### Verification`: validation outcome, residual risk, and what still needs checking.
- If execution fails, still provide all three sections with actionable error evidence.
