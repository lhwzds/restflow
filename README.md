<div align="center">
  <img src="web/src/assets/restflow.svg" alt="RestFlow Logo" width="120" height="120" />

# RestFlow

**Daemon-centric AI runtime for background tasks and agent execution**

[![Site](https://img.shields.io/badge/site-restflow.ai-black)](https://restflow.ai)
[![Docs](https://img.shields.io/badge/docs-restflow.ai%2Fdocs-blue)](https://restflow.ai/docs/)
[![Release](https://img.shields.io/github/v/release/lhwzds/restflow?label=latest)](https://github.com/lhwzds/restflow/releases/latest)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-dea584)](https://www.rust-lang.org/)

</div>

---

## Quick Start

### Install

**Homebrew**

```bash
brew install lhwzds/tap/restflow
```

**npm**

```bash
npm install -g restflow-cli
```

**From source**

```bash
cargo install --git https://github.com/lhwzds/restflow --package restflow-cli
```

### Start the daemon

```bash
restflow daemon start --foreground
```

### Add a model credential

```bash
restflow secret set OPENAI_API_KEY sk-xxx
# or
restflow secret set ANTHROPIC_API_KEY sk-ant-xxx
```

### Optional: connect external coding agents

```bash
# Sync RestFlow MCP to Codex
restflow mcp codex sync

# Sync RestFlow MCP to Claude Code
restflow mcp claude sync
```

### Optional: add CLI-backed execution backends

```bash
# Claude Code OAuth token
restflow auth add --provider claude-code --key <your-token>
```

## Architecture at a Glance

RestFlow is not a split frontend/backend app with duplicated execution logic.
It is a daemon-centric runtime:

- `restflow-core` owns daemon execution, persistence, background task runtime, and chat routing
- `restflow-ai` owns the agent loop, model execution, and subagent runtime capability
- `restflow-tools` owns tool implementations and registry assembly helpers
- Browser and CLI are client facades over daemon HTTP/MCP/IPC surfaces

Execution naming follows one canonical model:

- `Agent`: capability and identity
- `Task`: schedulable unit of work assigned to an agent
- `Run`: one execution of a task
- `Sub-agent`: delegated execution spawned within a run

See the local architecture references for the current design:

- [SYSTEM_ARCHITECTURE.md](./SYSTEM_ARCHITECTURE.md)
- [docs/TASK_RUN_DOMAIN_MODEL.md](./docs/TASK_RUN_DOMAIN_MODEL.md)

## Links

- Site: [restflow.ai](https://restflow.ai)
- Docs: [restflow.ai/docs](https://restflow.ai/docs/)
- Releases: [GitHub Releases](https://github.com/lhwzds/restflow/releases/latest)

## Development

```bash
# Rust workspace
cargo check

# Web app
cd web
npm install
npm run dev
```

Default MCP HTTP endpoint:

```text
http://localhost:8787/mcp
```
