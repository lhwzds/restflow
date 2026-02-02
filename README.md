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

## Quick Start

### Desktop App (Recommended)

```bash
# Clone and build
git clone https://github.com/lhwzds/restflow.git
cd restflow
cargo tauri dev
```

### Docker (Web Server Mode)

```bash
docker compose up -d --build
```

Access at http://localhost:3000

### CLI (TUI)

Launch the interactive terminal UI:

```bash
cargo run -p restflow-cli -- chat
```

Theme selection:

```bash
restflow --theme light chat
```

Generate shell completions:

```bash
restflow completions bash > restflow.bash
```

### Try the Online Demo

**[restflow.ai](https://restflow.ai)** - Live demo deployed on Vercel

## Documentation

**[restflow.ai/docs](https://restflow.ai/docs/)** - Full documentation
