# CLI Reference

RestFlow provides a powerful command-line interface for managing workflows, agents, and authentication.

## Installation

```bash
# Build from source
cargo build --release --bin restflow

# Or install directly
cargo install --path crates/restflow-cli
```

## Commands Overview

| Command | Description |
|---------|-------------|
| `restflow chat` | Start interactive TUI chat |
| `restflow claude` | Execute via Claude Code CLI |
| `restflow agent` | Agent management |
| `restflow auth` | Authentication management |
| `restflow skill` | Skill management |
| `restflow task` | Task management |
| `restflow secret` | Secret management |
| `restflow mcp` | Start as MCP server |

## Claude Command

Execute prompts via Claude Code CLI using OAuth authentication.

### Usage

```bash
restflow claude [OPTIONS] -p <PROMPT>
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `-p, --prompt <PROMPT>` | Prompt to send to Claude | Required |
| `-m, --model <MODEL>` | Model to use (opus, sonnet, haiku) | `sonnet` |
| `--session-id <ID>` | Session ID for conversation continuity | - |
| `--resume` | Resume existing session | `false` |
| `-w, --cwd <DIR>` | Working directory | Current dir |
| `--timeout <SECONDS>` | Timeout in seconds | `300` |
| `--auth-profile <ID>` | Auth profile ID to use | Auto-select |
| `--format <FORMAT>` | Output format (text, json) | `text` |

### Examples

```bash
# Basic execution
restflow claude -p "Hello, respond with OK"

# Use specific model
restflow claude -p "Explain Rust ownership" -m opus

# JSON output
restflow claude -p "List 3 colors" --format json

# Session management
restflow claude -p "Remember my name is Alice" --session-id my-session
restflow claude -p "What's my name?" --session-id my-session --resume

# Pipe input
echo "Explain this code" | restflow claude

# Specify working directory
restflow claude -p "List files in this project" --cwd ~/projects/myapp
```

### Prerequisites

Before using `restflow claude`, you need to configure a ClaudeCode auth profile:

```bash
restflow auth add --provider claude-code --key "sk-ant-oat01-..." --name "My Claude Code"
```

See [Authentication](auth.md) for details on obtaining and configuring OAuth tokens.

## Agent Commands

Manage AI agents.

```bash
# List agents
restflow agent list

# Show agent details
restflow agent show <ID>

# Create agent
restflow agent create --name "My Agent" --model sonnet

# Execute agent
restflow agent exec <ID> -i "Your prompt"

# Delete agent
restflow agent delete <ID>
```

## Task Commands

Manage scheduled tasks.

```bash
# List tasks
restflow task list

# Create task
restflow task create --agent <ID> --name "Daily Report" --cron "0 9 * * *"

# Run task immediately
restflow task run <ID>

# Pause/Resume task
restflow task pause <ID>
restflow task resume <ID>
```

## Skill Commands

Manage reusable skills.

```bash
# List skills
restflow skill list

# Show skill
restflow skill show <ID>

# Import skill from file
restflow skill import ./my-skill.md

# Export skill
restflow skill export <ID> -o ./exported-skill.md
```

## Global Options

These options are available for all commands:

| Option | Description |
|--------|-------------|
| `--db-path <PATH>` | Custom database path |
| `-v, --verbose` | Enable verbose logging |
| `--format <FORMAT>` | Output format (text, json) |
| `-h, --help` | Print help |
