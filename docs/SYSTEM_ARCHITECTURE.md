# RestFlow System Architecture

## Status

- Updated: 2026-02-28
- Scope: Runtime architecture, deployment model, and migration baseline
- Audience: Core contributors working on Tauri, CLI, daemon, and runtime channels

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
├── config.json
├── restflow.db
├── master.key
└── logs/
```

Supported environment overrides:

- `RESTFLOW_DIR`
- `RESTFLOW_MASTER_KEY`

## 8. Migration Baseline

Automatic migrations are expected for legacy key/config/profile formats.

Manual migration command:

```bash
restflow migrate --dry-run
restflow migrate
```

Legacy directories/files can be removed only after migration verification.

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

