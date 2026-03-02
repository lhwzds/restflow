# Minimal Refactor Roadmap (Behavior-Compatible, Non-Disruptive)

## 1. Problem Statement

RestFlow currently has several maintainability risks caused by duplicated assembly paths, weak ownership boundaries, and multi-source state updates. These issues slow delivery and increase regression risk.

This roadmap defines a minimal refactor that:
- Preserves runtime behavior and public contracts.
- Avoids disruptive migration and avoids destructive replacement.
- Improves internal clarity with incremental, reversible steps.

## 2. Constraints

1. No breaking changes to CLI, API, MCP, tool names, aliases, or response envelopes.
2. No destructive migration, no hard cutover, no one-shot replacement.
3. Daemon remains the single execution and persistence authority.
4. Each phase must be independently mergeable and revertible.
5. Existing behavior must be proven equivalent by tests before and after each internal change.

## 3. Phase Plan (A/B/C/D)

### Phase A: Guardrails and Baseline

Goal:
- Make behavior drift immediately visible before any structural change.

Primary outcomes:
- Contract coverage checks exist and run in CI.
- Baseline compatibility matrix and test plan are documented.

Atomic tasks:
1. Create/update compatibility matrix for CLI/API/MCP/tool contracts.
2. Add migration-safe test plan focused on behavior equivalence.
3. Add IPC coverage guardrail (`#[tauri::command]` inventory vs exported IPC surface).
4. Add CLI shape/dispatch equivalence tests (top-level and subcommand forms).
5. Add tool assembly equivalence tests (allowlist path vs full registry path).
6. Add storage index consistency regression tests (update/delete/cascade scenarios).

### Phase B: Tool Assembly Consolidation

Goal:
- Keep current entry points, but route them through one shared internal assembly model.

Primary outcomes:
- A single internal composition path with compatibility wrappers.
- No external contract changes.

Atomic tasks:
1. Introduce `ToolAssemblyContext` as unified dependency input.
2. Introduce `ToolSpec` catalog (name, aliases, requirements, builder).
3. Rewire `registry_from_allowlist` to shared internals.
4. Rewire `create_tool_registry` to shared internals.
5. Preserve both public entry points and verify equivalence tests pass.

### Phase C: Boundary and Storage Safety

Goal:
- Reduce hidden coupling and index drift without changing external behavior.

Primary outcomes:
- Validation responsibilities become explicit at service boundary.
- Storage consistency is less caller-dependent.

Atomic tasks:
1. Move external/runtime-aware validation from model layer to service validator.
2. Introduce internal read-before-write patterns where caller-supplied old values are risky.
3. Strengthen index/cascade cleanup guarantees with targeted tests.
4. Verify no semantic change in public APIs and storage outcomes.

### Phase D: Frontend State Simplification

Goal:
- Preserve UX while reducing duplicated state ownership and write paths.

Primary outcomes:
- One source of truth for selected session.
- Predictable stream write flow with fewer duplicated mutation points.

Atomic tasks:
1. Consolidate selected-session ownership in store layer.
2. Unify optimistic and event-driven write handling paths where duplicated.
3. Add/adjust frontend unit tests for session and stream state transitions.
4. Add/adjust E2E checks for unchanged user-visible behavior.

## 4. Merge Order

1. Merge Phase A first (all guardrails in place).
2. Merge Phase B next (internal tool assembly consolidation behind wrappers).
3. Merge Phase C after B (boundary/storage cleanup validated by A guardrails).
4. Merge Phase D last (frontend state simplification validated end-to-end).

Rule:
- Do not merge a later phase unless all earlier phase guardrails are green on current `main`.

## 5. Rollback Strategy

Phase-level rollback:
1. Revert only the commits belonging to the affected phase.
2. Keep Phase A guardrails in place during rollback for immediate regression detection.
3. Retain compatibility wrappers and legacy-compatible entry points until a full stability cycle confirms safety.

Operational rollback trigger examples:
- Contract equivalence test fails.
- Behavior mismatch in CLI/API/MCP smoke checks.
- Storage index consistency regression.
- Frontend E2E behavior drift.

## 6. Definition of Done (DoD)

A phase is done only when all items below are true:
1. All relevant tests pass (`cargo test`, frontend unit tests, E2E where applicable).
2. Contract compatibility is preserved (CLI/API/MCP/tool names/aliases/response envelope).
3. No destructive migration introduced; rollout remains incremental and reversible.
4. Phase documentation is updated with evidence of behavior equivalence.
5. Rollback path for that phase is verified and documented.
