# RestFlow System Architecture

## Status

- Updated: 2026-03-11
- Scope: Runtime architecture, deployment model, and migration baseline
- Audience: Core contributors working on Tauri, CLI, daemon, and runtime channels

## Refactor Docs

- [Minimal Refactor Roadmap](./MINIMAL_REFACTOR_ROADMAP.md)
- [Compatibility Matrix](./COMPATIBILITY_MATRIX.md)
- [Migration Test Plan](./MIGRATION_TEST_PLAN.md)

## 1. Architectural Decision

RestFlow follows a **daemon-centric** architecture.

- Daemon is the only execution and persistence owner.
- Tauri and CLI are client facades and must call daemon APIs (IPC/RPC).
- Business execution happens in core runtime on daemon side.
- Storage writes are centralized in daemon-owned service/storage layers.

This avoids split-brain behavior, inconsistent routing logic, and duplicated write paths.

## 2. System Invariants

1. Single writer: only daemon writes sessions, tool traces, background task state, and bindings.
2. Single execution center: agent execution and routing decisions are daemon-owned.
3. Single event identity: realtime and persisted events must share stable IDs.
4. Client isolation: Tauri/CLI must not add direct storage business paths.

## 3. Runtime Topology

```text
UI/Clients
  - Tauri Desktop
  - CLI
  - External channel connectors (Telegram/Discord/Slack)
  - MCP callers
        |
        | IPC / HTTP / MCP
        v
Daemon
  - IPC server
  - MCP HTTP server
  - Channel router + chat dispatcher
  - Background agent runner
  - Runtime event publishing
  - Service layer
        |
        v
Storage
  - sessions/messages
  - tool_traces
  - background_agents + history
  - auth/secrets/config
```

## 4. Main Execution Flows

### 4.1 Chat Session Flow

1. Client sends request to daemon.
2. Daemon routes message via channel runtime.
3. Runtime executes agent/tool loop.
4. Daemon emits realtime events and persists final state.
5. Client renders stream and later reads history from the same source of truth.

### 4.2 Background Agent Flow

1. Task is scheduled/triggered in daemon.
2. Runner executes task in daemon runtime.
3. Messages/events are published once with stable IDs.
4. Task history and message history are persisted by daemon only.

### 4.3 Tool Trace Flow

1. Runtime emits turn/tool events during execution.
2. `tool_traces` persists execution traces.
3. Session execution steps are backfilled from traces for persisted UI rendering.

## 5. Component Responsibilities

### Tauri

- UI state and interaction only.
- Calls daemon through executor/IPC.
- No local direct storage write path.

### CLI

- Command interface and user-facing formatting.
- Uses daemon as primary runtime endpoint.
- Does not duplicate core runtime behavior.

### Daemon/Core Runtime

- Owns chat routing, background execution, and event emission.
- Owns all persistence updates.
- Owns channel/session binding and policy enforcement.

## 6. Deployment Model

## Local Development

```bash
restflow daemon start --foreground
```

Common operations:

```bash
restflow daemon start
restflow daemon stop
restflow daemon status
```

MCP HTTP default endpoint:

- `http://localhost:8787/mcp`

### Service Management

- Linux: `systemd` (`scripts/restflow.service`)
- macOS: `launchd` (`scripts/com.restflow.daemon.plist`)

## 7. Data and Config Layout

RestFlow unified runtime directory:

```text
~/.restflow/
├── config.toml
├── restflow.db
├── master.key
└── logs/
```

Supported environment overrides:

- `RESTFLOW_DIR`
- `RESTFLOW_MASTER_KEY`

### 7.1 Effective Config Precedence

Runtime configuration resolves in this order:

1. Code defaults
2. Global `~/.restflow/config.toml`
3. Workspace `./.restflow/config.toml`

Database state is no longer part of the runtime configuration read path. The
database remains the persistence layer for secrets, traces, sessions, and other
runtime state.

### 7.2 Config Groups and Primary Consumers

The `config.toml` file is a unified document with explicit top-level sections.
Runtime configuration now uses the same section names across storage, CLI, and
tooling.

| Group | On-disk shape | Primary purpose | Representative keys | Primary consumers |
| --- | --- | --- | --- | --- |
| System | `[system]` | Cross-cutting system policy, retention, and feature flags | `worker_count`, `task_timeout_seconds`, `max_retries`, `chat_session_retention_days`, `log_file_retention_days` | cleanup services, daemon/runtime setup, feature flag loading |
| Agent | `[agent]` | Agent and sub-agent execution policy | `max_iterations`, `subagent_timeout_secs`, `max_parallel_subagents`, `max_tool_calls`, `tool_timeout_secs` | agent executor, subagent manager, background agent runtime, chat dispatcher |
| API | `[api]` | Default limits for MCP and API-facing operations | `memory_search_limit`, `session_list_limit`, `background_trace_line_limit`, `web_search_num_results` | MCP server handlers, runtime tool registry |
| Runtime | `[runtime]` | Default daemon runtime behavior | `background_runner_poll_interval_ms`, `background_runner_max_concurrent_tasks`, `chat_max_session_history` | background runner, chat dispatcher |
| Channel | `[channel]` | External channel integration defaults | `telegram_api_timeout_secs`, `telegram_polling_timeout_secs` | Telegram channel runtime |
| Registry | `[registry]` | Skill and marketplace integration defaults | `github_cache_ttl_secs`, `marketplace_cache_ttl_secs` | marketplace adapters, skill discovery/install flows |
| CLI | `[cli]` | CLI-only local behavior | `version`, `agent`, `model`, `sandbox.*` | CLI config loader, local sandbox execution |

### 7.3 Naming Principles

Section names describe ownership domains, not implementation details:

- `[system]` replaces ambiguous "root" terminology with an explicit public
  section.
- `_defaults` is not used in on-disk section names. The file stores effective
  runtime configuration, not just suggestion bags.
- CLI convenience selections are flattened into `[cli]` as `agent` and `model`;
  there is no `[cli.default]` subsection anymore.
- Tool-adjacent knobs should live with the subsystem that owns the behavior,
  not in a generic `[tool]` bucket. For example, `web_search_num_results`
  belongs to `[api]`, while channel transport timeouts belong to `[channel]`.

## 8. Migration Baseline

Automatic migrations are expected for legacy key/profile formats. Runtime
configuration now converges into `~/.restflow/config.toml`.

## 9. Guardrails for Contributors

Do:

- Add new business capabilities in daemon IPC/RPC handlers first.
- Keep routing ownership in daemon runtime components.
- Preserve one-way client facade boundaries.

Do not:

- Add direct storage access in Tauri commands.
- Add fallback write paths that bypass daemon ownership.
- Encode routing ownership only in display fields on session models.

## 10. Implementation Roadmap (High-Level)

1. Enforce daemon handshake and remove silent fallback execution paths.
2. Unify client command surfaces through daemon APIs.
3. Move routing ownership to explicit channel/session binding.
4. Unify realtime and persisted event identity to eliminate duplicates.
5. Remove obsolete compatibility paths after rollout verification.
