<div align="center">
  <img src="web/src/assets/restflow.svg" alt="RestFlow Logo" width="120" height="120" />

# RestFlow

**AI agents work. You rest.**

A high-performance AI agent runtime built in Rust — run 10+ agents in parallel

[![Demo](https://img.shields.io/badge/demo-restflow.ai-brightgreen)](https://restflow.ai)
[![Docs](https://img.shields.io/badge/docs-restflow.ai%2Fdocs-blue)](https://restflow.ai/docs/)
[![Release](https://img.shields.io/github/v/release/lhwzds/restflow?label=latest)](https://github.com/lhwzds/restflow/releases/latest)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-dea584)](https://www.rust-lang.org/)
[![Vue 3](https://img.shields.io/badge/vue-3.x-4fc08d)](https://vuejs.org/)

</div>

---

## What is RestFlow?

RestFlow is a **high-performance AI agent runtime** built in Rust. It runs multiple autonomous agents in parallel — each agent thinks, decides, acts, and observes in a ReAct loop until your task is complete.

### Why RestFlow?

- **Rust-Powered Performance** — Async Tokio runtime handles 10+ concurrent agents with minimal resource usage
- **Multi-Provider LLM** — Anthropic, OpenAI, DeepSeek, Claude Code, OpenAI Codex, Gemini — switch freely
- **Sandboxed Python** — Monty-backed Python execution with security isolation
- **MCP Native** — First-class Model Context Protocol server for AI tool integration
- **Security-First** — Approval-based security gate for dangerous operations (bash, file writes)

### How It Works

```
You: "Research competitors, build a prototype, and monitor the deployment"

              ┌──────────────────────────────────────────────┐
              │         RestFlow Agent Runtime               │
              │         Rust · Tokio · 10+ Parallel          │
              │                                              │
              │  🔬 Researcher ─── HTTP, Search, Vision      │
              │  💻 Coder ──────── Bash, File, Python        │
              │  📊 Analyst ────── Python, Memory, HTTP      │
              │  ✍️  Writer ──────── File, Search, Memory    │
              │  🔍 Reviewer ───── File, Bash, Memory        │
              │  📡 Monitor ────── HTTP, Bash, Telegram      │
              │         ·                                    │
              │         · (add more agents as needed)        │
              │                                              │
              │  Each agent: Think → Act → Observe → Loop    │
              └──────────────────────────────────────────────┘
                    ↓              ↓              ↓
               📧 Email      📱 Telegram     📁 Reports
```

## Key Features

| Category           | Features                                                                                  |
| ------------------ | ----------------------------------------------------------------------------------------- |
| **AI Agent Core**  | ReAct loop, 10+ parallel background agents, subagent spawning, working + long-term memory |
| **Built-in Tools** | HTTP, Bash, File, Python (Monty sandbox), Email, Telegram, Web Search, Vision, Transcribe |
| **LLM Providers**  | Anthropic Claude, OpenAI GPT, DeepSeek, Claude Code CLI, OpenAI Codex, Gemini             |
| **Security**       | Approval gate for bash/file ops, command chain detection, header sanitization             |
| **Integration**    | MCP server (port 8787), Telegram bot, scheduled cron tasks, skill system                  |
| **Platform**       | Browser + local daemon, CLI daemon, Docker, self-hosted                                   |

## Architecture

- **Workspace Chat (Main Agent)** — Interactive chat where each message triggers the AI agent with full tool access
- **Background Agents** — Autonomous agents running in parallel, managed via `manage_background_agents`, with scheduling and Telegram notifications
- **Subagents** — Main agent can spawn specialist subagents (researcher, coder, reviewer, writer, analyst)

## Installation

### CLI (Recommended)

Both `restflow` and `rf` command names are supported (via Homebrew and npm installs).

**Homebrew (macOS/Linux)**

```bash
brew install lhwzds/tap/restflow
```

**npm (Cross-platform)**

```bash
npm install -g restflow-cli
```

**Direct Download**

Download pre-built binaries from [GitHub Releases](https://github.com/lhwzds/restflow/releases/latest):

- macOS: `restflow-aarch64-apple-darwin.tar.gz` (Apple Silicon) / `restflow-x86_64-apple-darwin.tar.gz` (Intel)
- Linux: `restflow-aarch64-unknown-linux-gnu.tar.gz` (ARM64) / `restflow-x86_64-unknown-linux-gnu.tar.gz` (x64)
- Windows: `restflow-x86_64-pc-windows-msvc.zip`

**Build from Source**

```bash
cargo install --git https://github.com/lhwzds/restflow --package restflow-cli
```

### Docker

```bash
docker compose up -d --build
```

MCP HTTP server available at `http://localhost:8787/mcp`

## Quick Start

```bash
# Start RestFlow daemon
restflow start

# Configure API key
restflow secret set ANTHROPIC_API_KEY sk-ant-xxx
# or: restflow secret set OPENAI_API_KEY sk-xxx

# Configure Telegram bot for AI chat
restflow secret set TELEGRAM_BOT_TOKEN <your-bot-token>
# Now chat with your AI agent via Telegram!

# Or add Claude Code as a provider (requires Claude Code installed)
restflow auth add --provider claude-code --key <your-oauth-token>
```

Generate shell completions:

```bash
restflow completions bash > restflow.bash
```

## Documentation

**[restflow.ai/docs](https://restflow.ai/docs/)** - Full documentation
