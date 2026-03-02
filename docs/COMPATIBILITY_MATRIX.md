# Compatibility Matrix

This document defines compatibility requirements for `CLI`, `MCP`, `Tauri IPC`, `Runtime`, `Storage`, and `Frontend`.
Any change that violates these requirements must be treated as breaking and gated behind explicit versioning and migration.

## 1. Compatibility Matrix

| Surface | Compatibility Contract | Breakage Indicator |
|---|---|---|
| CLI | Keep binary name `restflow`; keep existing command paths and flags working (`daemon`, `task`, `agent`, `secret`, and existing `--format` support). | Existing command invocation fails, is renamed, or changes output contract without versioning. |
| MCP | Keep JSON-RPC MCP lifecycle stable (`initialize` then tool calls); keep existing tool names and aliases callable; keep error envelope shape stable. | Existing MCP clients fail to call tools or cannot parse error/result payloads. |
| Tauri IPC | Keep exported command names callable; preserve generated bindings target usage in frontend; preserve response envelope semantics unless explicitly versioned. | Frontend IPC calls fail because command names, parameters, or payload shapes changed. |
| Runtime | Keep tool assembly entry points and call contracts stable; preserve current default behavior when no additional gate is configured. | Existing runtime integrations require call-site changes to keep working. |
| Storage | Keep existing table names, key formats, and read semantics backward compatible; migration must be additive-first and reversible where possible. | Existing data cannot be read after upgrade, or old readers fail on upgraded data. |
| Frontend | Keep API wrapper entry points and generated type import paths usable; avoid UX regressions in session/chat/task core workflows. | Existing pages fail at compile/runtime, or core workflows regress without explicit product decision. |

## 2. Non-Breaking Rules

1. Do not remove or rename public commands, MCP tools, IPC commands, API wrapper functions, or storage keys/tables that are already in use.
2. For behavior changes, prefer additive evolution:
   - Add new fields as optional.
   - Keep old fields readable until migration is completed.
   - Keep old command/tool aliases available.
3. Any unavoidable breaking change must include all of the following in the same merge:
   - Explicit version boundary.
   - Migration path (code and data).
   - Rollback strategy.
   - Release note describing impact and upgrade steps.
4. Preserve backward parsing:
   - Accept previously valid input formats.
   - Keep tolerant readers for older payloads/data.
5. Enforce compatibility in tests before merge (see checklist below).

## 3. Pre-Merge Validation Checklist

1. CLI compatibility
   - Command smoke tests pass for existing command paths and common flags.
   - Existing machine-readable output format (where supported) remains parseable.
2. MCP compatibility
   - `initialize`, `tools/list`, and representative tool invocation tests pass.
   - Error payload contract remains backward compatible.
3. Tauri IPC compatibility
   - Existing command binding and invocation tests pass.
   - Frontend build succeeds against current generated bindings.
4. Runtime compatibility
   - Tool assembly and invocation contract tests pass.
   - Default runtime behavior remains unchanged when no new feature flag/config is provided.
5. Storage compatibility
   - Migration tests validate old data can be read after upgrade.
   - Key/index format compatibility tests pass.
6. Frontend compatibility
   - Unit tests for API wrappers and stores pass.
   - Core workflow E2E smoke tests pass (session/chat/task critical path).
7. Final gate
   - No unversioned removals or renames of public compatibility surfaces.
   - Changelog/release note is updated when compatibility-sensitive behavior changed.
