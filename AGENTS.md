# RestFlow AI Coding Guide

Always answer in 中文 to user！
Always answer in 中文 to user！
永远回答中文！
永远回答中文！
思考可以用英文但是回答一定用中文！
思考可以用英文但是回答一定用中文！

---

## Architecture Reference

Use [SYSTEM_ARCHITECTURE.md](./SYSTEM_ARCHITECTURE.md) as the canonical source for:

- daemon/runtime topology
- provider/model ownership
- boundary ingress rules for agent, background-agent, and subagent flows
- subagent vs. durable background runtime ownership

Use [docs/TASK_RUN_DOMAIN_MODEL.md](./docs/TASK_RUN_DOMAIN_MODEL.md) for canonical `Task / Run` naming rules.

Keep summaries in this file short and update the architecture document first when the two diverge.

## Code and Comment Language Standard

**IMPORTANT**: All code, comments, and documentation in the codebase MUST be written in English.

### Required English Usage

- ✅ Variable names: English
- ✅ Function names: English
- ✅ Comments (inline and block): English
- ✅ Type definitions: English
- ✅ Error messages in code: English
- ✅ Test descriptions: English
- ✅ Documentation files: English

### Exceptions

- ❌ User-facing messages: Can be Chinese or other languages as needed for the product
- ❌ Demo/mock data: Can contain multilingual examples (e.g., chat history showing translation features)

### Examples

**✅ Correct:**

```typescript
// Extract node configuration and remove label field
const config = extractNodeConfig(node);

// Verify that the API response contains valid data
expect(response.data).toBeDefined();
```

**❌ Incorrect:**

```typescript
// 提取节点配置并移除 label 字段
const config = extractNodeConfig(node);

// 验证 API 响应包含有效数据
expect(response.data).toBeDefined();
```

**这意味着：所有代码、注释和文档必须使用英文编写（除了面向用户的消息和演示数据）。**

---

## Git Commit Rules

**CRITICAL: Do NOT commit automatically!**
**CRITICAL: 永远不要自动 commit！**
**只有用户明确要求时才能 commit！**
**Only commit when the user explicitly asks!**

**CRITICAL: Do NOT add Co-Authored-By in commit messages!**
**CRITICAL: Do NOT add Co-Authored-By in commit messages!**
**永远不要添加 Co-Authored-By！**
**永远不要添加 Co-Authored-By！**

- ❌ **NEVER** commit automatically - wait for user to say "commit" or similar
- ❌ **NEVER** add `Co-Authored-By: Claude` or any similar attribution
- ❌ **NEVER** add `Co-Authored-By: <any AI>`
- ✅ Only commit when explicitly asked by user
- ✅ Keep commits with only the human author
- ✅ Use conventional commits format: `type(scope): description`

---

## ⚠️ Tailwind CSS v4 Width Classes Warning

**CRITICAL: DO NOT use `max-w-*` preset classes like `max-w-3xl`, `max-w-2xl`, etc.!**
**CRITICAL: 不要使用 `max-w-3xl`、`max-w-2xl` 等预设宽度类！**

### Problem

This project uses Tailwind CSS v4 with custom `@theme` configuration in `web/src/styles/tailwind.css`. The custom spacing values **override** the default Tailwind width presets:

```css
@theme {
  --spacing-2xl: 20px;
  --spacing-3xl: 24px;
  /* ... */
}
```

This causes `max-w-3xl` to be computed as **24px** instead of the expected **48rem (768px)**!

### Solution

**Always use arbitrary values for width constraints:**

```vue
<!-- ❌ WRONG - Will be only 24px wide! -->
<div class="max-w-3xl mx-auto">

<!-- ✅ CORRECT - Use arbitrary value -->
<div class="max-w-[48rem] mx-auto">

<!-- ❌ WRONG -->
<DialogContent class="max-w-2xl">

<!-- ✅ CORRECT -->
<DialogContent class="max-w-[42rem]">
```

### Common Width Values

| Preset (Don't Use) | Arbitrary Value (Use This) |
| ------------------ | -------------------------- |
| `max-w-sm`         | `max-w-[24rem]`            |
| `max-w-md`         | `max-w-[28rem]`            |
| `max-w-lg`         | `max-w-[32rem]`            |
| `max-w-xl`         | `max-w-[36rem]`            |
| `max-w-2xl`        | `max-w-[42rem]`            |
| `max-w-3xl`        | `max-w-[48rem]`            |
| `max-w-4xl`        | `max-w-[56rem]`            |

---

## Testing Requirements

**CRITICAL: Every feature implementation MUST include tests!**
**CRITICAL: 每次实现功能都必须添加测试！**

### Required Tests

When implementing a new feature, you MUST add:

1. **Frontend Unit Tests** (Vitest)
   - Location: `web/src/**/__tests__/*.test.ts`
   - Test composables, utilities, and component logic
   - Run: `cd web && npm run test`

2. **Backend Unit Tests** (Rust)
   - Location: `crates/*/src/**/*.rs` (inline `#[cfg(test)]` modules)
   - Test services, models, and business logic
   - Run: `cargo test`

3. **E2E Tests** (Playwright)
   - Location: `e2e-tests/tests/*.spec.ts`
   - Test user workflows and UI interactions
   - Run: `cd e2e-tests && npm test`

### Test Checklist

Before completing a feature:

- [ ] Frontend unit tests added and passing
- [ ] Backend unit tests added and passing (if backend changes)
- [ ] E2E tests added and passing (for UI features)
- [ ] All existing tests still pass

### Example Test Locations

| Change Type      | Test Location                                |
| ---------------- | -------------------------------------------- |
| Vue composable   | `web/src/composables/**/__tests__/*.test.ts` |
| API function     | `web/src/api/__tests__/*.test.ts`            |
| Utility function | `web/src/utils/__tests__/*.test.ts`          |
| Rust service     | `crates/restflow-core/src/services/*.rs`     |
| UI workflow      | `e2e-tests/tests/*.spec.ts`                  |

### CI Parity And Local Preflight

**CRITICAL: When fixing CI regressions, run the same entrypoint as CI before concluding the fix is complete.**
**CRITICAL: 修 CI 问题时，必须优先运行与 CI 完全一致的本地入口命令！**

Rules:

1. **Use the CI entrypoint, not a near-equivalent shortcut**
   - E2E: run `cd e2e-tests && npm test`
   - Frontend: run `cd web && npm run test`
   - Backend/clippy: run the same package-targeted `cargo test` / `cargo clippy` command used by CI
   - Do not substitute `npx playwright test` for `npm test` unless you are intentionally debugging below the CI wrapper

2. **Reproduce CI-only guarded flows by removing local provider credentials**
   - If a test or tool behavior depends on provider availability, rerun the minimal test with provider env vars unset
   - Example pattern:
     ```bash
     env -u OPENAI_API_KEY -u ANTHROPIC_API_KEY -u GEMINI_API_KEY \
       cargo test -p restflow-cli --test mcp_daemon <test_name> -- --nocapture
     ```
   - This catches confirmation-required / preview / blocked flows that may be skipped on a fully configured local machine

3. **Prefer a focused regression test before broad reruns**
   - When CI exposes a bug, first add or tighten the smallest package-level test that reproduces it
   - For task/run guard behavior, prefer Rust package tests in the affected crate before relying on full GitHub Actions reruns

4. **Exercise non-Unix compilation paths when touching IPC or daemon boundary code**
   - If you change code under `crates/restflow-core/src/daemon/`, `ipc_client/`, `mcp/`, or platform-gated modules, run:
     ```bash
     cargo check -p restflow-core --target x86_64-pc-windows-msvc
     ```
   - On macOS/Linux hosts this may still be blocked by third-party native dependencies, but it can catch our own Windows-only Rust API drift before CI does

5. **Mirror browser contract renames in E2E mocks immediately**
   - If UI copy, IPC request names, or route contracts change, update E2E stubs and assertions in the same change
   - Typical drift hotspots:
     - menu labels
     - IPC request names
     - run/session/task route patterns
     - cleanup helpers that still use legacy request types

6. **Do not treat CI as the first structured debugger**
   - Before pushing, prefer a short preflight sequence for touched areas
   - Suggested pattern:
     ```bash
     cargo test -p restflow-tools <focused_test> -- --nocapture
     env -u OPENAI_API_KEY -u ANTHROPIC_API_KEY -u GEMINI_API_KEY \
       cargo test -p restflow-cli --test mcp_daemon <focused_test> -- --nocapture
     cd e2e-tests && node ./scripts/run-isolated-e2e.mjs --grep "<affected workflow>"
     ```

These checks do not replace full CI, but they should eliminate a large class of "passes locally, fails only on CI" regressions.

---

## Table of Contents

1. [Project Overview](#project-overview)
2. [Quick Start](#quick-start)
3. [Project Structure](#project-structure)
4. [Development Setup](#development-setup)
5. [Common Commands](#common-commands)
6. [CLI Daemon (Telegram + AI Chat)](#cli-daemon-telegram--ai-chat)
7. [Git Standards](#git-standards)
8. [Multi-Agent Parallel Development Guidelines](#multi-agent-parallel-development-guidelines)
9. [Type Synchronization](#type-synchronization)
10. [Quick Debugging](#quick-debugging)
11. [Environment Configuration](#environment-configuration)

## Project Overview

**RestFlow** is an **AI assistant that can execute workflows**. Unlike traditional workflow tools, RestFlow's "workflow" means AI step-by-step execution - the AI thinks, decides, acts, and observes in a loop (similar to how Skills work) until your task is complete.

**Core Identity:**

- 🤖 **AI Assistant** - Understands intent, makes decisions, executes tasks autonomously
- ⚡ **Workflow = AI Execution** - AI executes multi-step tasks (Think → Act → Observe loop)
- 🔧 **Skill System** - Extensible capabilities via prompts, workflows, or code

**Tech Stack:**

- **Frontend**: Browser-based Vue 3 + TypeScript + VueFlow UI
- **Backend**: Rust (RMCP for MCP HTTP transport, redb embedded database)
- **AI Integration**: Multi-provider LLM support (Anthropic, OpenAI, DeepSeek, Gemini, Codex)
- **Tools**: HTTP requests, Bash, File ops, Email, Telegram, Python (via Monty sandbox)

**Key Features:**

- AI Agent with ReAct loop execution
- Skill-based capability system (Prompts, Workflows, Code)
- Visual drag-and-drop workflow editor
- Scheduled task execution with notifications
- MCP (Model Context Protocol) server support
- Integrated terminal with PTY support

## Quick Start

### Prerequisites

- **Rust**: 1.85+ (install via [rustup.rs](https://rustup.rs/))
- **Node.js**: 22+ and npm 10+
- **Python**: 3.11+ (for Python node execution)
- **Platform Tools**:
  - macOS: Xcode Command Line Tools
  - Linux: build-essential, pkg-config, libssl-dev
  - Windows: Visual Studio C++ Build Tools

### First Run

```bash
# Clone repository
git clone https://github.com/yourusername/restflow.git
cd restflow

# Install frontend dependencies
cd web
npm install
cd ..

# Run in development mode (choose one):

# Option 1: Daemon with MCP HTTP (port 8787)
cargo run --package restflow-cli -- daemon start --foreground
# Frontend: cd web && npm run dev

# Option 2: CLI
cargo run --package restflow-cli
```

## Project Structure

```
restflow/
├── crates/                        # Rust workspace
│   ├── restflow-traits/           # Level 0: Shared trait definitions (25+ traits, 50+ types)
│   ├── restflow-sandbox/          # Level 0: Standalone sandbox utility
│   ├── restflow-contracts/        # Level 1: Shared transport and boundary contracts
│   ├── restflow-models/           # Level 1: Shared provider/model primitives and catalog
│   ├── restflow-telemetry/        # Level 1: Shared telemetry and trace shaping
│   ├── restflow-storage/          # Level 1: Database layer (redb, secrets, config)
│   ├── restflow-browser/          # Level 1: Browser automation runtime
│   ├── restflow-ai/               # Level 2: AI Agent framework
│   │   ├── src/
│   │   │   ├── agent/             #   ReAct loop, subagent runtime, state machine
│   │   │   ├── cache/             #   File cache, permission cache, search cache
│   │   │   ├── llm/               #   Multi-provider LLM client (Anthropic, OpenAI, Gemini, Codex)
│   │   │   └── tools/             #   Tool wrapper interface
│   ├── restflow-tools/            # Level 2: Tool implementations (70+ tools)
│   │   ├── src/
│   │   │   ├── impls/             #   All tool impls + ToolRegistryBuilder
│   │   │   ├── security/          #   BashSecurityChecker, SSRF, network allowlists
│   │   │   └── skill/             #   SkillAsTool, register_skills
│   ├── restflow-core/             # Level 3: Core business logic
│   │   ├── src/
│   │   │   ├── auth/              #   Auth profile discovery, management, refresh
│   │   │   ├── channel/           #   Communication channels (Telegram, Discord, Slack)
│   │   │   ├── daemon/            #   Background daemon (IPC, MCP, health, recovery)
│   │   │   ├── hooks/             #   Hook executor
│   │   │   ├── loader/            #   Skill loader (git, folder, package)
│   │   │   ├── lsp/               #   LSP client, manager, protocol
│   │   │   ├── mcp/               #   MCP server + tool handlers
│   │   │   ├── memory/            #   Chunker, search, export, unified search
│   │   │   ├── models/            #   Core-only models, auth policy, and re-exports
│   │   │   ├── performance/       #   Cache, metrics, task queue, worker pool
│   │   │   ├── registry/          #   Skill marketplace registry
│   │   │   ├── runtime/           #   Daemon-owned runtime execution
│   │   │   │   ├── agent/         #     Agent tool assembly and prompt helpers
│   │   │   │   ├── background_agent/  # Durable background/task runtime owner
│   │   │   │   ├── channel/       #     Chat dispatcher, commands, forwarder
│   │   │   │   └── subagent/      #     Storage-backed subagent definition adapters
│   │   │   ├── security/          #   Approval, checker, shell parser
│   │   │   ├── services/          #   Service layer
│   │   │   │   ├── adapters/      #     Trait adapters (storage→traits bridge)
│   │   │   │   └── tool_registry.rs  # ToolRegistryBuilder orchestration
│   │   │   └── storage/           #   Database layer (delegates to restflow-storage)
│   ├── restflow-cli/              # Level 4: CLI application
│   │   ├── src/
│   │   │   ├── commands/          #     CLI command handlers
│   │   │   ├── config/            #     Settings
│   │   │   ├── daemon/            #     Daemon runner (Telegram, Discord, Slack)
│   │   │   ├── executor/          #     Direct + IPC executors
│   │   │   └── output/            #     JSON + table formatters
├── web/                           # Vue.js frontend
│   ├── src/
│   │   ├── api/                   #   API clients
│   │   ├── components/            #   Vue components
│   │   ├── composables/           #   Vue composables
│   │   ├── constants/             #   App constants
│   │   ├── locales/               #   i18n translations
│   │   ├── plugins/               #   Plugins (CodeMirror, etc.)
│   │   ├── router/                #   Vue Router
│   │   ├── stores/                #   Pinia stores
│   │   ├── styles/                #   CSS/theme styles
│   │   ├── types/                 #   Generated TypeScript (ts-rs)
│   │   ├── utils/                 #   Utilities
│   │   └── views/                 #   Page views
├── python/                        # Python runtime
│   ├── runtime/                   #   Core Python code
│   └── scripts/                   #   User scripts (gitignored)
├── SYSTEM_ARCHITECTURE.md         # Canonical runtime and crate architecture
├── docs/                          # Focused design notes
│   └── TASK_RUN_DOMAIN_MODEL.md
└── CLAUDE.md                      # This file
```

### Crate Dependency Hierarchy

```
Level 0 (Foundation):  restflow-traits    restflow-sandbox
Level 1 (Shared):      restflow-contracts restflow-models restflow-telemetry restflow-storage restflow-browser
Level 2 (Framework):   restflow-ai        restflow-tools
Level 3 (Engine):      restflow-core
Level 4 (Apps):        restflow-cli
```

**Notes**:

- `restflow-core` consumes shared contracts, models, telemetry, storage, and the AI/tooling framework.
- `restflow-tools` only depends on `restflow-ai` in `[dev-dependencies]` (for test mocks like `MockLlmClient`). There is no production dependency from tools to ai.
- `restflow-browser` is currently a standalone runtime crate and is not part of the main daemon execution stack.

### Model and Provider Ownership

Canonical reference: [SYSTEM_ARCHITECTURE.md](./SYSTEM_ARCHITECTURE.md)

Use this ownership split when touching provider/model code:

- `restflow-traits`: canonical provider identity and runtime switching contracts
- `restflow-models`: shared provider metadata, model catalog, selectors, and runtime model specs
- `restflow-ai`: concrete client construction and hot-swapping
- `restflow-core`: daemon-only pairing, auth-aware availability, and policy

Contributor rules:

- Do not define another canonical provider enum outside `restflow-traits`.
- Do not add local alias tables in CLI/tool/agent code when the alias can live in `restflow-models`.
- Keep auth/storage-aware policy in `restflow-core` unless it becomes a shared primitive.

## Development Setup

### Backend Setup

```bash
# Build all binaries
cargo build

# Run tests
cargo test

# Format code
cargo fmt

# Check lints (use -D warnings to match CI behavior)
cargo clippy -- -D warnings
```

**Requirement**: Run `cargo clippy` for every backend change and ensure it completes with zero warnings/errors before submitting or requesting review.

### External Volume Build Rule

**CRITICAL: If the repository is located on an external macOS volume, do NOT run Rust test/build artifacts directly from that volume's default `target/` directory.**
**CRITICAL: 如果仓库位于外置 macOS 磁盘上，不要直接在该卷的默认 `target/` 目录里运行 Rust 测试/构建产物！**

### Why

On recent macOS versions, AMFI / Code Trust may block unsigned Rust test binaries and generated Mach-O `.dylib` files when they are executed from an external APFS volume. This can look like:

- `cargo test` hanging after launching the test binary
- test binaries stuck in sleep state
- kernel logs containing `AMFI ... has no CMS blob` or `Unrecoverable CT signature issue`

### Required Rule

When the repo is on an external drive:

1. **Enable ownership on the external volume**
   ```bash
   sudo diskutil enableOwnership /Volumes/<volume-name>
   ```

2. **Always move `CARGO_TARGET_DIR` to an internal-disk path before running `cargo build`, `cargo test`, `cargo check`, or `cargo clippy`**
   ```bash
   export CARGO_TARGET_DIR="$HOME/.cargo-targets/restflow"
   ```

3. **Do not rely on ad-hoc `codesign` of Cargo outputs as the default workflow**
   - Signing every generated test binary and `.dylib` is not stable enough for day-to-day development.
   - The supported workflow is: repo can stay on the external drive, but Rust build artifacts must live on an internal volume.

4. **Re-sign the installed CLI after copying it into `~/.local/bin`**
   - On recent macOS versions, copying `restflow` from a build directory into `~/.local/bin` can trigger `AppleSystemPolicy` / AMFI load failures even when the original release binary runs correctly.
   - If `~/.local/bin/restflow --version` hangs before entering `main`, or kernel logs show `load code signature error 2` / `Security policy would not allow process`, re-sign the installed file in place:
   ```bash
   codesign -f -s - "$HOME/.local/bin/restflow"
   ```

### Recommended Commands

```bash
mkdir -p "$HOME/.cargo-targets/restflow"
export CARGO_TARGET_DIR="$HOME/.cargo-targets/restflow"

cargo test
cargo clippy -- -D warnings
```

### Verification

If this rule is violated, check:

```bash
mount | grep '/Volumes/<volume-name>'
/usr/bin/log show --last 5m --style compact --predicate 'process == "kernel"' | rg 'AMFI|CMS blob|CT signature'
```

If AMFI/CT errors appear, move `CARGO_TARGET_DIR` off the external volume before further debugging application code.

**⚠️ CI uses `-D warnings`**: The CI pipeline runs `cargo clippy -- -D warnings`, which treats all warnings as errors. Always test locally with this flag to avoid CI failures. Common issues:

- Unused variables/constants: Add `#[allow(dead_code)]` or remove them
- Unused imports: Remove or prefix with `_`

### Frontend Setup

```bash
cd web

# Install dependencies
npm install

# Development server
npm run dev

# Build for production
npm run build

# Run tests
npm run test

# Format code
npm run format

# Generate TypeScript types from Rust
npm run generate:types
```

### Database

- **Default location**: `./restflow.db` in current directory
- **Custom path**: `cargo run --package restflow-cli -- --db-path /path/to/db`
- **In-memory (testing)**: Use `:memory:` as path

## Common Commands

### Running Different Modes

```bash
# CLI mode (shows help, use subcommands for actions)
cargo run --package restflow-cli

# Daemon mode (MCP HTTP server on port 8787)
cargo run --package restflow-cli -- daemon start --foreground
```

---

## CLI Command Reference

```bash
restflow --help                 # Show all commands
restflow <command> --help       # Show command details
```

**Key Commands:**

```bash
restflow daemon start           # Start background service (Telegram + scheduler + MCP HTTP)
restflow agent list             # List agents
restflow agent create           # Create a new agent
restflow secret set KEY VALUE   # Manage API keys
restflow memory search "query"  # Search long-term memory
restflow session list           # List chat sessions
restflow mcp tools              # List MCP tools
```

---

## Browser Frontend

The browser frontend talks directly to the local daemon over HTTP:

- `GET /api/health`
- `POST /api/request`
- `POST /api/stream`
- `POST /mcp`

Frontend transport wrappers live under `web/src/api/` and are backed by daemon HTTP request/stream clients.

---

## CLI Daemon (Telegram + AI Chat)

The CLI daemon runs in the background, enabling Telegram bot integration with AI chat support.

### Starting the Daemon

```bash
# Start daemon in background
cargo run --package restflow-cli -- daemon start

# Start daemon in foreground (for debugging)
cargo run --package restflow-cli -- daemon start --foreground

# With verbose logging
cargo run --package restflow-cli -- daemon -v start --foreground
```

### Managing the Daemon

```bash
# Check daemon status
cargo run --package restflow-cli -- daemon status

# Stop the daemon
cargo run --package restflow-cli -- daemon stop
```

### Logs Location

Daemon logs are written to:

```
~/Library/Application Support/restflow/logs/restflow.log.YYYY-MM-DD
```

View logs in real-time:

```bash
tail -f ~/Library/Application\ Support/restflow/logs/restflow.log.*
```

### Prerequisites for Telegram + AI Chat

1. **Telegram Bot Token**: Configure via `restflow secret set TELEGRAM_BOT_TOKEN <token>`
2. **AI API Key**: Set one of:
   - `restflow secret set ANTHROPIC_API_KEY <key>`
   - `restflow secret set OPENAI_API_KEY <key>`
3. **Agent**: At least one agent must exist (system creates "default" automatically)

### Message Flow

```
User Message → Telegram Bot
      ↓
MessageRouter (routes by type)
      ├─ /command → CommandHandler (/run, /status, /stop, /help)
      ├─ Task linked → TaskForwarder (forward to running task)
      └─ Natural language → ChatDispatcher (AI conversation)
                                    ↓
                            AI Agent executes
                                    ↓
                            Response → Telegram
```

### Common Issues

| Issue                    | Solution                                        |
| ------------------------ | ----------------------------------------------- |
| "Database already open"  | Daemon already running, use `daemon stop` first |
| "No API key configured"  | Set ANTHROPIC_API_KEY or OPENAI_API_KEY         |
| Daemon exits immediately | Check logs for errors, try foreground mode      |
| No response from bot     | Verify TELEGRAM_BOT_TOKEN is correct            |

---

## MCP HTTP Server (Port 8787)

The daemon exposes an MCP (Model Context Protocol) server over HTTP using JSON-RPC 2.0 via the `rmcp` crate's Streamable HTTP transport.

### Endpoint

```
POST http://localhost:8787/mcp
Content-Type: application/json
Accept: application/json, text/event-stream
```

### Request Format (JSON-RPC 2.0)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "METHOD_NAME",
  "params": {}
}
```

### Step 1: Initialize (required first call)

```bash
curl -X POST http://localhost:8787/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
```

### Step 2: List Tools

```json
{ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }
```

### Step 3: Call a Tool

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "list_skills",
    "arguments": {}
  }
}
```

### Available MCP Tools

| Category                | Tools                                                                                       |
| ----------------------- | ------------------------------------------------------------------------------------------- |
| Skills                  | `list_skills`, `get_skill`, `create_skill`, `update_skill`, `delete_skill`, `skill_execute` |
| Agents                  | `list_agents`, `get_agent`                                                                  |
| Memory                  | `memory_search`, `memory_store`, `memory_stats`                                             |
| Sessions                | `chat_session_list`, `chat_session_get`                                                     |
| Tasks                   | `manage_tasks` (legacy alias: `manage_background_agents`)                                  |
| Hooks                   | `manage_hooks`                                                                              |
| Runtime (session-only)  | `switch_model`                                                                              |
| Runtime (from registry) | `http`, `email`, `telegram`, `bash`, `file`, `python`, `web_search`, `web_fetch`, etc.      |

### Response Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{ "type": "text", "text": "..." }]
  }
}
```

### Key Implementation Files

- MCP server: `crates/restflow-core/src/mcp/server.rs`
- HTTP transport setup: `crates/restflow-core/src/daemon/mcp.rs`
- Custom port: `restflow daemon start --foreground --mcp-port 9000`

---

### Testing

```bash
# Backend tests
cargo test
cargo test --package restflow-core

# Frontend tests
cd web && npm run test

# E2E tests
cd web && npm run test:e2e
```

### Building for Production

```bash
# CLI binary (binary name is "restflow", package is "restflow-cli")
cargo build --release --package restflow-cli

# Web assets
cd web && npm run build
```

### Installing

```bash
# Install CLI binary to ~/.local/bin
mkdir -p ~/.local/bin
cp target/release/restflow ~/.local/bin/restflow
codesign -f -s - ~/.local/bin/restflow

# Serve the production web UI from the daemon
RESTFLOW_WEB_DIST_DIR=/absolute/path/to/restflow/web/dist \
restflow daemon start --foreground
```

Open the browser against the daemon HTTP port after the daemon is running.

## Git Standards

### Commit Messages

**REMINDER: Do NOT add Co-Authored-By in commit messages!**
**永远不要添加 Co-Authored-By！**

Follow conventional commits format:

```
<type>(<scope>): <subject>

<body>
```

**⚠️ NO Co-Authored-By footer! Keep only human author!**

**Types:**

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `style`: Code style (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Tests
- `chore`: Build, CI, maintenance

**Examples:**

```bash
git commit -m "feat(engine): Add parallel node execution"
git commit -m "fix(storage): Handle transaction deadlock"
git commit -m "docs: Update API documentation"
```

### Atomic Commit Strategy

**When the user requests atomic commits, follow this workflow:**

1. **Per-file commit**: Each logically independent file gets its own commit
2. **Test before commit**: Run the relevant tests for each file before committing
3. **Never reconstruct intermediate states** - commit the final version of each file, not replay history
4. **Order**: Commit files from least-dependent to most-dependent (utilities first, then consumers)

```bash
# Workflow per file:
cargo test --package <pkg> -- <test_module>::tests   # 1. Test the specific file
git add <file>                                        # 2. Stage only that file
git commit -m "test(scope): add tests for XxxAdapter" # 3. Commit with conventional format
```

### Dirty Working Tree Safety Protocol

**CRITICAL: The working tree often has pre-existing uncommitted changes from other agents or manual work.**
**CRITICAL: 工作目录经常有其他 agent 或手动操作留下的未提交改动！**

Before making atomic commits, you MUST follow this protocol to avoid accidentally committing other people's changes:

**Step 0: Clear the staging area FIRST**

```bash
# ALWAYS unstage everything before starting your commits
git reset HEAD
# Verify nothing is staged
git diff --cached --stat   # Should show nothing
```

**Step 1: Stage ONLY your specific files by name**

```bash
# ✅ CORRECT - Stage specific files
git add path/to/your/file.rs

# ❌ WRONG - Never use these in a dirty tree
git add .
git add -A
git add -u
```

**Step 2: Verify staged content before committing**

```bash
# Check exactly what will be committed
git diff --cached --stat   # Should list ONLY your files
git diff --cached          # Review actual diff
```

**Step 3: If a commit accidentally includes wrong files**

```bash
# Undo the commit but keep all changes
git reset --soft HEAD~1
# Unstage everything
git reset HEAD
# Start over with Step 1
```

**Why this matters:**

- `git commit` commits ALL staged files, not just the ones you just added
- Other agents may have left files in the staging area (`git add` without `git commit`)
- Git rename detection can pull in unrelated files when staging modified `mod.rs`/`lib.rs` files
- `Cargo.lock` is frequently pre-staged and will silently sneak into your commit

**Environment variable safety in tests:**

- Tests that modify env vars (e.g., `RESTFLOW_DIR`, `RESTFLOW_MASTER_KEY`) MUST use a `Mutex` guard
- Save/restore env vars immediately after the component that needs them is initialized
- Use the `env_lock()` pattern to prevent parallel test interference:

```rust
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
```

### Branch Strategy

- `main`: Production-ready code
- `develop`: Development branch
- `feature/*`: New features
- `fix/*`: Bug fixes
- `release/*`: Release preparation

---

## Multi-Agent Parallel Development Guidelines

**CRITICAL: When creating plans for parallel agent development, you MUST consider file conflicts!**
**CRITICAL: 创建并行开发计划时，必须考虑文件冲突问题！**

### Core Principle

When multiple AI agents work on different features simultaneously, they often modify the same files, causing merge conflicts. Plan file ownership carefully and merge in order.

### Conflict Hotspot Files (Avoid Parallel Modification)

```
⚠️ HIGH CONFLICT RISK - Assign to ONE agent only:
├── Cargo.lock                    # Auto-generated, always conflicts
├── crates/*/Cargo.toml          # Dependency declarations
├── crates/restflow-cli/
│   ├── src/main.rs              # Command dispatch
│   └── src/cli.rs               # CLI structure definitions
├── crates/restflow-core/
│   ├── src/lib.rs               # Public exports
│   └── src/models/mod.rs        # Model re-exports
└── web/src/types/               # Generated types
```

### Best Practices

| Strategy                        | Description                                             |
| ------------------------------- | ------------------------------------------------------- |
| **Module Isolation**            | Different agents work on different crates/directories   |
| **Interface First**             | Define public interfaces before parallel implementation |
| **Frequent Rebase**             | After each PR merges, other PRs immediately rebase      |
| **Dependency Coordination**     | One agent manages Cargo.toml, others request additions  |
| **Small PR Strategy**           | Split into smaller PRs to reduce conflict scope         |
| **Sequential for Shared Files** | If files must be shared, implement sequentially         |
| **Check Open PRs First**        | Before coding, run `gh pr list` to see what's in flight |

### Conflict Resolution Checklist

When conflicts occur:

- [ ] Check for duplicate dependencies in Cargo.toml
- [ ] Regenerate Cargo.lock with `cargo update`
- [ ] Merge struct fields carefully (don't lose any)
- [ ] Verify build passes: `cargo check`
- [ ] Run clippy: `cargo clippy -- -D warnings`

---

## Type Synchronization

RestFlow uses ts-rs to generate TypeScript types from Rust structs:

### When to regenerate types:

- After modifying any Rust struct with `#[derive(TS)]`
- After adding new models or enums
- After changing API contracts

### How to regenerate:

```bash
cd web
npm run generate:types
```

This updates files in `web/src/types/` to match Rust definitions.

### Type-safe workflow:

1. Put shared provider/model/runtime-facing types in `crates/restflow-models/src/`.
2. Keep daemon-only pairing or policy types in `crates/restflow-core/src/models/`.
3. Run type generation.
4. TypeScript compiler ensures frontend compatibility.
5. No runtime type mismatches!

## Quick Debugging

### Enable Debug Logs

```bash
# General debug
RUST_LOG=debug cargo run --package restflow-cli -- daemon start --foreground

# Specific module
RUST_LOG=restflow_core::engine=trace cargo run

# Multiple modules
RUST_LOG=restflow_core::engine=debug,restflow_core::storage=trace cargo run
```

### Common Issues

**Workflow stuck processing:**

- Check: `RUST_LOG=restflow_core::engine=debug`
- Solution: Increase worker count or check for infinite loops

**Database locked:**

- Check: `lsof restflow.db`
- Solution: `pkill restflow` or remove lock file

**TypeScript errors after Rust changes:**

- Solution: `cd web && npm run generate:types`

**Python node not working:**

- Check: Python 3.11+ installed
- Solution: RestFlow auto-downloads uv on first use

## Environment Configuration

### Frontend Environment Variables

Create `.env` in `web/` directory:

```bash
# Demo mode (optional)
VITE_DEMO_MODE=false
```

### Backend Configuration

Configure via CLI arguments:

```bash
# Custom database path
cargo run --package restflow-cli -- --db-path /path/to/database.db

# Custom MCP port
cargo run --package restflow-cli -- daemon start --foreground --mcp-port 9000
```

### API Keys

Managed through the application (stored in database):

1. Via UI: Settings → Secrets
2. Via API: POST to `/api/secrets`
3. Via environment: Falls back to env vars (e.g., `OPENAI_API_KEY`)
