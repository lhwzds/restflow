<div align="center">
  <img src="web/src/assets/restflow.svg" alt="RestFlow Logo" width="120" height="120" />

# RestFlow

**AI agents work. You rest.**

An intelligent AI agent that can create, manage, and execute automated workflows

[![Demo](https://img.shields.io/badge/demo-restflow.ai-brightgreen)](https://restflow.ai)
[![Docs](https://img.shields.io/badge/docs-restflow.ai%2Fdocs-blue)](https://restflow.ai/docs/)
[![Status](https://img.shields.io/badge/status-prototype-orange)](https://github.com/lhwzds/restflow)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-dea584)](https://www.rust-lang.org/)
[![Vue 3](https://img.shields.io/badge/vue-3.x-4fc08d)](https://vuejs.org/)

</div>

> ⚠️ **Early Development** - This project is in active development. APIs and features may change.

---

## What is RestFlow?

RestFlow is an **AI assistant that can execute workflows**. Unlike traditional workflow automation tools, RestFlow's "workflow" means AI step-by-step execution - the AI thinks, decides, acts, and observes in a loop until your task is complete.

- 🤖 **AI-Powered** - Understands natural language, makes decisions, handles exceptions
- ⚡ **Executes Workflows** - AI executes multi-step tasks autonomously (Think → Act → Observe)
- 🔧 **Extensible Skills** - Create new capabilities via prompts, workflows, or code
- 📅 **Scheduled Tasks** - Run automations on schedule with notifications
- 📊 **Full Traceability** - Every execution is logged and auditable

### How It Works

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   User: "Monitor my server and notify me if it's done"      │
│                                                             │
│                          ↓                                  │
│                                                             │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│   │  AI Agent   │───►│  Workflow   │───►│    Send     │     │
│   │  Understands│    │  Skills     │    │ Notification│     │
│   └─────────────┘    │  Python     │    └─────────────┘     │
│                      └─────────────┘                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Key Features

- **🤖 AI Agent Core** - ReAct loop with multi-provider LLM support (Anthropic, OpenAI, DeepSeek)
- **📋 Skill System** - Define agent capabilities via prompts, workflows, or code
- **🔧 Built-in Tools** - HTTP requests, Python scripts, Email, and more
- **📅 Task Scheduling** - Cron-based automation with Telegram notifications
- **💻 Desktop App** - Native Tauri application with integrated terminal
- **🔌 MCP Support** - Model Context Protocol for AI tool integration

## Execution Model (Current)

RestFlow currently exposes two user-facing execution paths:

- **Workspace Chat (Main Agent)**: interactive chat where each user message triggers assistant response generation.
- **Background Agents**: asynchronous or scheduled runs managed through `manage_background_agents` and `/api/background-agents`.

Legacy command surfaces are removed:

- `restflow task ...`
- `restflow agent-task ...`
- `restflow agent exec ...`

## Installation

### CLI (Recommended)

Both `restflow` and `rf` command names are supported.

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

### Desktop App

```bash
git clone https://github.com/lhwzds/restflow.git
cd restflow
cargo tauri dev
```

### Docker (Web Server Mode)

```bash
docker compose up -d --build
```

Access at http://localhost:3000

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

# Or use Claude Code CLI (requires Claude Code installed)
restflow auth add --provider claude-code --key <your-oauth-token>
restflow claude -p "Hello, world!"
```

Generate shell completions:

```bash
restflow completions bash > restflow.bash
```

## Documentation

**[restflow.ai/docs](https://restflow.ai/docs/)** - Full documentation
