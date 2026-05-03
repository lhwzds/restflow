# Current Branch Simplification Plan

## Goal

Simplify the current `codex/tui-system-simplification` branch into one coherent
product direction:

- RestFlow owns daemon-backed chat, task/run execution, and skill activation.
- RestFlow treats built-in systemskills and `skrun` executable skills as the
  runtime-visible skill catalog.
- `load_skill` is load-only.
- `run_skill` is the only new executable skill runner.
- The TUI `/skill` surface becomes a view/read surface instead of a creation,
  install, marketplace, and mutation manager.

## Current Branch Context

The current working tree is not a narrow TUI-only change. It already touches:

- CLI help/manpage visibility for mutable skill commands.
- Runtime tool assembly and skill activation.
- Skill catalog adapters.
- The `run_skill` tool implementation.
- TUI skill manager state, rendering, and slash-command copy.

The simplification plan should therefore be reviewed as a skill-runtime and TUI
surface simplification, not just a terminal UI cleanup.

## Scope

1. Skill catalog boundary
   - Systemskills remain bundled, read-only, and highest priority.
   - `skrun` skills are discovered through the public `skrun skill` CLI
     contract and are read-only from RestFlow.
   - Legacy storage-backed skills remain a compatibility read path only until a
     separate cleanup removes or migrates persisted user data.

2. Runtime tool boundary
   - `load_skill` lists and reads skill content only.
   - `run_skill` runs an installed executable skill by id with JSON object
     input.
   - Storage-backed skill ids are not registered as direct runtime tools.

3. TUI and CLI surface
   - `/skill` presents skills and details.
   - Create/install/update/delete/import/export/search commands are hidden from
     the primary CLI help surface while compatibility handlers remain.
   - The TUI should not encourage creating or installing skills from inside
     RestFlow.

4. Verification
   - Add focused Rust tests for catalog ordering, `skrun` parsing, runtime tool
     registration, and TUI skill manager behavior.
   - Run package-level tests before broader workspace checks.

## Non-Goals

- Do not remove the storage schema or persisted skill records in this branch.
- Do not delete hidden CLI handlers until a separate data-cleanup or migration
  plan exists.
- Do not introduce a second marketplace or package manager surface inside
  RestFlow.
- Do not change Task / Run terminology or durable runtime ownership.
- Do not move daemon-owned execution paths into the TUI or CLI.

## Constraints

- `SYSTEM_ARCHITECTURE.md` is canonical: the daemon remains the execution and
  persistence owner, and client surfaces stay thin.
- `docs/TASK_RUN_DOMAIN_MODEL.md` is canonical for Task / Run naming.
- `restflow-tools` owns tool implementations, not durable runtime ownership.
- `restflow-core::runtime::subagent` remains adapter-only.
- Code, comments, and repository documentation must be English.
- The repo is on an external macOS volume; Rust build artifacts should use an
  internal `CARGO_TARGET_DIR` for local validation.

## Design Rules

1. One read path for runtime-visible skills
   - Build the runtime skill provider from systemskills plus `skrun`.
   - Preserve deterministic shadowing: systemskill ids win over `skrun`, and
     both win over legacy storage records.

2. Separate reading from execution
   - `load_skill` must never execute.
   - `run_skill` must never mutate the skill catalog.
   - `run_skill` input must be a JSON object and must pass the security gate.

3. Keep legacy storage out of model-visible tools
   - Legacy storage-backed skill records may remain visible in service-level
     list/get flows only if they are clearly compatibility data.
   - They should not be auto-registered as callable tool names.

4. Make TUI copy match capability
   - Use "View skills" rather than "Manage skills" if creation and install are
     not primary capabilities.
   - Remove the "Create Skill" row and any shortcut copy that implies mutation.

5. Treat explicit skill mentions carefully
   - If `@skill` can activate suggested tools without assignment, the behavior
     needs tests that cover dangerous suggested tools and the security gate.
   - If that is too permissive, keep assignment-based activation for suggested
     tools and use `load_skill` for unassigned explicit mentions.

## Implementation Steps

1. Freeze the intended public surface
   - Document the visible commands: `skill list`, `skill show`, `/skill`, and
     `@skill`.
   - Document the hidden compatibility commands and why they are not removed in
     this branch.

2. Normalize the effective skill catalog
   - Keep `SystemSkillProvider` as the first provider.
   - Add `SkrunSkillProvider` as the second provider.
   - Keep legacy storage records as a service-level compatibility fallback, not
     a runtime provider used by model-visible tools.
   - Add tests for systemskill-over-`skrun` shadowing and `skrun`-over-storage
     shadowing.

3. Harden the `skrun` CLI contract
   - Prefer `skrun skill list --format json`.
   - Keep TSV fallback only if it is part of the expected public contract.
   - Add tests for missing `skrun`, invalid JSON fallback, empty output, and
     `skill show --format json`.

4. Finalize runtime tool assembly
   - Register `load_skill` with the systemskill plus `skrun` provider.
   - Register `run_skill` explicitly, with security gate, timeout, and JSON
     object validation.
   - Delete or update tests that expected storage-backed skills to become
     direct runtime tools.

5. Simplify the TUI skill manager
   - Remove the create row.
   - Rename "Manage skills" to "View skills".
   - Remove shortcut copy for delete unless delete remains intentionally
     supported for legacy storage entries.
   - Add reducer/state/render tests for empty, systemskill, `skrun`, and
     legacy-storage catalog states.

6. Hide mutable CLI commands without deleting handlers
   - Hide create/update/delete/import/export/search/install in clap help.
   - Regenerate and review CLI manpages.
   - Keep direct command handlers working until the compatibility policy is
     decided.

7. Update product documentation
   - Update skill docs to explain systemskills, `skrun` skills, `load_skill`, and
     `run_skill`.
   - Cross-link the plan from the relevant skill/TUI docs only after the branch
     direction is accepted.

## Verification

Focused checks:

```bash
cargo test -p restflow-core services::adapters::skill_provider
cargo test -p restflow-tools skrun
cargo test -p restflow-core runtime::agent::tools
cargo test -p restflow-tui skill
```

Broader checks:

```bash
CARGO_TARGET_DIR="$HOME/.cargo-targets/restflow" cargo test
CARGO_TARGET_DIR="$HOME/.cargo-targets/restflow" cargo clippy -- -D warnings
```

Manual smoke checks:

```bash
restflow skill --help
restflow skill list
restflow skill show team
RESTFLOW_SKRUN_BIN=/path/to/fake/skrun restflow
```

TUI smoke criteria:

- `/skill` opens a view-only skill picker.
- The picker can show systemskills and `skrun` skills.
- The picker does not display a create row.
- Copy and shortcuts do not advertise unavailable mutation paths.
- `@skill` mentions either activate only safe/read behavior or are covered by
  security-gated suggested-tool tests.

## Risks

- Allowing unassigned `@skill` mentions to activate suggested tools may widen
  the runtime tool surface. This needs explicit security-gate coverage.
- Calling `skrun` during catalog listing may slow prompt preparation if the CLI
  is slow. Consider caching or a short timeout if this appears in smoke tests.
- Hiding CLI commands without removing handlers can confuse contributors unless
  the compatibility policy is documented.
- Keeping legacy storage skills visible in service flows but unavailable as
  runtime tools may need UI copy to explain compatibility-only entries.

## Open Decisions

1. Should `@skill` activate suggested tools for any known skill, or only for
   skills assigned to the agent?
2. Is TSV output from `skrun skill list` a supported public contract or only a
   temporary fallback?
3. Should legacy storage-backed skills remain visible in `/skill`, or should
   they move behind a hidden compatibility command?
4. Should `skrun` discovery be cached per turn/session to avoid repeated CLI
   calls?

## Acceptance Criteria

- The branch has one documented skill direction: systemskills plus `skrun` for
  runtime-visible skills.
- `load_skill` remains load-only.
- `run_skill` is the only executable skill runner introduced by the branch.
- TUI and CLI copy no longer claim primary skill creation/install management.
- Storage-backed skills are not registered as model-visible runtime tools.
- Focused tests cover catalog priority, runtime registration, `skrun` tool
  behavior, and TUI skill manager rendering.
- Full backend test and clippy preflight are identified before merge.
