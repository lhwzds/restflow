# Migration Test Plan

## Purpose

This document defines the minimum test gates required to validate the daemon-centric migration without breaking behavior compatibility.

## 1. Phase Test Scope

### Phase 1: Runtime Ownership and Routing

Scope:
- Daemon-only execution ownership for chat and background tasks
- Channel routing and session binding consistency
- Event ID stability between realtime stream and persisted records

Primary risks:
- Split execution paths between client and daemon
- Session routing drift across channel reconnects

### Phase 2: Client Surface Convergence

Scope:
- Tauri and CLI command paths calling daemon APIs only
- Removal or disabling of local fallback write paths
- Compatibility of existing client interactions

Primary risks:
- Partial fallback paths still writing local state
- UX regression from API contract mismatch

### Phase 3: Persistence and Migration

Scope:
- Legacy config/profile/key migration correctness
- Storage consistency after migration completion
- Idempotent behavior for repeated migration execution

Primary risks:
- Data loss during migration
- Duplicate or conflicting records after replay

### Phase 4: Cleanup and Compatibility Removal

Scope:
- Removal of deprecated compatibility paths
- Validation that no required legacy behavior is dropped
- Post-cleanup production-like verification

Primary risks:
- Hidden runtime dependencies on removed compatibility code
- Operational regressions in daemon lifecycle commands

## 2. Quality Gates (Backend / Frontend / E2E)

All gates below are blocking gates for merge.

### Backend Gate

Required checks:
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace`

Pass criteria:
- Zero formatter drift
- Zero clippy warnings/errors
- All backend tests pass with no migration-related failures

### Frontend Gate

Required checks:
- `cd web && npm run test`
- `cd web && npm run build`

Pass criteria:
- Unit/integration frontend tests pass
- Production build succeeds with no type or bundling errors

### E2E Gate

Required checks:
- `cd e2e-tests && npm test`

Pass criteria:
- Critical user workflows pass under daemon-centric runtime
- No new flaky failures introduced by migration changes

## 3. Smoke Validation Flows

Run these flows after each migration phase merge candidate:

1. Daemon lifecycle smoke:
- Start daemon in foreground
- Verify health/status command
- Stop daemon cleanly

2. Chat execution smoke:
- Send a chat request through CLI or UI
- Confirm streaming response is visible
- Confirm persisted history matches stream output

3. Background task smoke:
- Create and run a background task
- Verify progress events and completion state
- Verify persisted history is queryable

4. Migration command smoke:
- Run `restflow migrate --dry-run`
- Run `restflow migrate` on a migration fixture
- Re-run migration to confirm idempotency behavior

## 4. Regression Judgment Criteria

A change is considered regression-free only if all conditions are met:

1. Functional compatibility:
- Existing user workflows remain available with equivalent outcomes
- No required workflow depends on removed compatibility path

2. Data integrity:
- No data loss in sessions, messages, traces, or task history
- No duplicate records caused by mixed realtime/persisted identities

3. Runtime stability:
- No crash/loop/deadlock in daemon routing and execution paths
- Daemon start/stop/status behavior remains operationally stable

4. Operational confidence:
- All blocking gates pass in CI and local verification
- Smoke flows pass on a clean environment and a migrated environment

If any criterion fails, the phase must be rolled back or fixed before merge.

## 5. Suggested Execution Order

1. Run backend gate first for fast structural feedback.
2. Run frontend gate to validate client contract compatibility.
3. Run E2E gate for end-to-end behavior confirmation.
4. Run smoke validation flows on a migration fixture before final merge.
