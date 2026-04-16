<div align="center">
  <img src="web/src/assets/restflow.svg" alt="RestFlow Logo" width="120" height="120" />

# RestFlow

**Make your agent binary. Make your workflow binary. Make your skill binary.**

[![Site](https://img.shields.io/badge/site-restflow.ai-black)](https://restflow.ai)
[![Docs](https://img.shields.io/badge/docs-restflow.ai%2Fdocs-blue)](https://restflow.ai/docs/)
[![Release](https://img.shields.io/github/v/release/lhwzds/restflow?label=latest)](https://github.com/lhwzds/restflow/releases/latest)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-dea584)](https://www.rust-lang.org/)

</div>

---

## What RestFlow Is

RestFlow is a daemon-centric AI runtime that is evolving toward a simple product idea:

This product model is under active development and is being implemented incrementally on top of the existing daemon runtime.

- **Skill Binary**: package one reusable AI capability as a portable executable unit
- **Agent Binary**: compile one agent with its model, tools, policy, and behavior into a runnable unit
- **Workflow Binary**: compose multiple skills and agents into a fixed executable flow

The daemon remains the runtime center.
It is already the execution and persistence binary for RestFlow today, and the higher-level
skill / agent / workflow binary model is built on top of that runtime rather than replacing it.

In practice, this means RestFlow is not just "an AI chat app" or "a workflow editor".
It is building toward a portable execution system for AI work:

- agents
- skills
- workflows
- parallel execution

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
```

## Product Model

RestFlow organizes AI work into three product layers:

### Skill Binary

The smallest reusable unit.

- encapsulates one AI capability
- can carry instructions, dependencies, and executable behavior
- is designed to be shareable, installable, and runnable

### Agent Binary

A packaged AI worker.

- binds model, tools, runtime policy, and attached skills
- is intended to run as a stable executable unit
- is the main building block for real task execution

### Workflow Binary

A compiled execution flow.

- composes multiple skills and agents
- encodes a fixed execution order
- is intended for reproducible AI workflows and multi-step automation

## Runtime Architecture

Today, RestFlow's implementation center is still the daemon runtime.

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

## Current State

RestFlow already has the daemon-first runtime foundation:

- daemon-owned execution
- browser and CLI as clients
- persistent chat sessions
- background task runtime
- tool execution and tracing
- MCP/HTTP/IPC surfaces

The product direction from here is to raise these runtime capabilities into first-class,
portable artifacts:

- skill binaries
- agent binaries
- workflow binaries

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
