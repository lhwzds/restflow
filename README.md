# RestFlow

**A local agent CLI with executable skills.**

[简体中文](./README.zh-CN.md)

[![Docs](https://img.shields.io/badge/docs-GitHub-blue)](./docs/index.md)
[![Release](https://img.shields.io/github/v/release/lhwzds/restflow?label=latest)](https://github.com/lhwzds/restflow/releases/latest)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-dea584)](https://www.rust-lang.org/)

---

## What RestFlow Is

RestFlow is a local agent CLI with a terminal-first interface and an external
executable skill boundary.

- **Skill Binary**: package one reusable AI capability as a portable executable unit
- **Agent Binary**: compile one agent with its model, tools, policy, and behavior into a runnable unit

The runtime center is the Rust agent loop plus TUI. Specialized external
capabilities are packaged and run through `skrun`.

In practice, this means RestFlow is a local command-line agent system:

- agents
- skills
- executable skill runs

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
cargo install --git https://github.com/lhwzds/restflow --package cli
```

### Start the daemon

```bash
restflow daemon start
```

### Add a model credential

```bash
restflow secret set OPENAI_API_KEY sk-xxx
```

### Open the TUI

```bash
restflow
```

## Links

- Repository: [GitHub](https://github.com/lhwzds/restflow)
- Docs: [Repository documentation](./docs/index.md)
- Releases: [GitHub Releases](https://github.com/lhwzds/restflow/releases/latest)
