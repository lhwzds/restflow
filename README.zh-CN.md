# RestFlow

**带可执行 skills 的本地 agent CLI。**

[English](./README.md)

[![Docs](https://img.shields.io/badge/docs-GitHub-blue)](./docs/index.md)
[![Release](https://img.shields.io/github/v/release/lhwzds/restflow?label=latest)](https://github.com/lhwzds/restflow/releases/latest)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-dea584)](https://www.rust-lang.org/)

---

## RestFlow 是什么

RestFlow 是一个本地 agent CLI，核心界面是终端和 TUI，并通过外部可执行
skill 扩展能力。

- **Skill Binary**：把一个可复用 AI 能力打包成可移植的可执行单元
- **Agent Binary**：把模型、工具、策略和行为绑定成一个可运行的 agent

核心运行时是 Rust agent loop 和 TUI。更专门的外部能力通过 `skrun` 打包和运行。

实际使用上，RestFlow 是一个本地命令行 agent 系统：

- agents
- skills
- executable skill runs

## 快速开始

### 安装

**Homebrew**

```bash
brew install lhwzds/tap/restflow
```

**npm**

```bash
npm install -g restflow-cli
```

**源码安装**

```bash
cargo install --git https://github.com/lhwzds/restflow --package cli
```

### 启动后台托管进程

```bash
restflow daemon start
```

### 添加模型凭据

```bash
restflow secret set OPENAI_API_KEY sk-xxx
```

### 打开 TUI

```bash
restflow
```

## 链接

- 仓库：[GitHub](https://github.com/lhwzds/restflow)
- 文档：[仓库文档](./docs/index.md)
- Releases: [GitHub Releases](https://github.com/lhwzds/restflow/releases/latest)
