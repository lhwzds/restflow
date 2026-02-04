# Daemon-Centric Architecture Plan

## Overview

RestFlow is moving to a daemon-centric architecture where all external entry points communicate with a single daemon process. The daemon owns storage access and exposes IPC/HTTP APIs for clients.

```
┌─────────────────────────────────────────────────────────────────────┐
│ External Access Layer                                                │
├─────────────┬─────────────┬─────────────┬─────────────┬──────────────┤
│ Web App     │ Tauri       │ CLI         │ Telegram    │ MCP          │
│ (Browser)   │ (Desktop)   │ (Terminal)  │ (Bot)       │ (Protocol)   │
└──────┬──────┴──────┬──────┴──────┬──────┴──────┬──────┴─────────────┘
       │             │             │             │
       │ HTTP/WS     │ IPC         │ IPC         │ stdio
       ▼             ▼             ▼             ▼
┌─────────────────────────────────────────────────────────────────────┐
│ RestFlow Daemon                                                      │
│ ┌──────────────┐ ┌─────────────┐ ┌─────────────┐ ┌───────────────┐  │
│ │ HTTP Server  │ │ IPC Server  │ │ Task Runner │ │ MCP Server    │  │
│ │ (API Gateway)│ │ (Local IPC) │ │ (Scheduler) │ │ (AI Tools)    │  │
│ └──────┬───────┘ └──────┬──────┘ └──────┬──────┘ └──────┬────────┘  │
│        │                │               │               │           │
│        └────────────────┼───────────────┼───────────────┘           │
│                         ▼                                           │
│                 ┌───────────────────┐                               │
│                 │ Service Layer     │                               │
│                 │ (Agent/Skill/etc) │                               │
│                 └─────────┬─────────┘                               │
│                           ▼                                         │
│                 ┌───────────────────┐                               │
│                 │ Storage Layer     │                               │
│                 │ (redb exclusive)  │                               │
│                 └───────────────────┘                               │
└─────────────────────────────────────────────────────────────────────┘
```

## Phased Plan

```
Phase 1: IPC Protocol Foundation
Phase 2: CLI IPC Integration
Phase 3: Daemon HTTP API
Phase 4: Tauri IPC Integration
Phase 5: Server as API Gateway
Phase 6: Unified Startup & Lifecycle
```

## Phase 1: IPC Protocol Foundation (P0)

Goal: Extend IPC protocol to cover all database operations.

Tasks:
- Expand `IpcRequest` in `crates/restflow-core/src/daemon/ipc_protocol.rs`
- Extend `IpcServer` handlers in `crates/restflow-core/src/daemon/ipc_server.rs`
- Add streaming response support in `crates/restflow-core/src/daemon/ipc_protocol.rs`
- Add IPC client helpers in `crates/restflow-core/src/daemon/ipc_client.rs`
- Unit tests in `crates/restflow-core/tests/`

New IPC request types include memory, session, auth, and system queries, plus streaming execution.

## Phase 2: CLI IPC Integration (P0)

Goal: CLI uses IPC when daemon is running; falls back to direct DB when not.

Tasks:
- `CommandExecutor` trait in `crates/restflow-cli/src/executor/mod.rs`
- `DirectExecutor` in `crates/restflow-cli/src/executor/direct.rs`
- `IpcExecutor` in `crates/restflow-cli/src/executor/ipc.rs`
- Refactor CLI commands to use executor abstraction
- Integration tests in `crates/restflow-cli/tests/`

## Phase 3: Daemon HTTP API (P1)

Goal: Built-in HTTP server for Web UI and external services.

Endpoints:
- `/api/agents/*`
- `/api/skills/*`
- `/api/tasks/*`
- `/api/memory/*`
- `/api/sessions/*`
- `/api/auth/*`
- `/api/config`
- `/api/execute` (WebSocket streaming)
- `/health`

## Phase 4: Tauri IPC Integration (P1)

Goal: Tauri app communicates with daemon via IPC instead of direct DB access.

Key changes:
- Introduce `TauriExecutor` and IPC client wrapper
- Add daemon lifecycle manager (start, readiness check, cleanup)
- Refactor Tauri commands to use IPC
- Stream handling in `crates/restflow-tauri/src/chat/stream.rs`

## Phase 5: Daemon HTTP API (P2)

Goal: daemon HTTP API replaces the standalone server crate.

Deployment modes:
- Mode A: Direct access to daemon HTTP API
- Mode B: Optional external gateway (not part of this repo)

## Phase 6: Unified Startup & Lifecycle (P2)

Goal: Single entry points for starting daemon and full stack.

Commands:
- `restflow daemon start`
- `restflow daemon start --http`
- `restflow start` (daemon + HTTP + open browser)

## Dependencies

```
Phase 1 → Phase 2, Phase 3, Phase 4
Phase 3 → Phase 5
Phase 2/3/4/5 → Phase 6
```

## Acceptance Criteria

Phase 1
- IPC request types complete
- IPC server handles all requests
- Unit test coverage > 80%

Phase 2
- CLI works via IPC when daemon is running
- Direct DB access when daemon is not running
- No database lock conflicts

Phase 3
- Daemon HTTP API complete
- WebSocket streaming works
- API docs generated

Phase 4
- Tauri app auto-starts/connects to daemon
- Tauri commands use IPC
- Streaming chat works

Phase 5
- Server has no AppCore dependency
- Deployable as server+daemon or daemon only
- Proxy overhead < 5ms

Phase 6
- `restflow start` works end-to-end
- Automatic restart on crash
- Unified log aggregation
