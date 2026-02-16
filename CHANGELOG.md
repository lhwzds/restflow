# Changelog

All notable changes to this project will be documented in this file.

## [0.3.2] - 2026-02-16

### Added
- Coordinator agent role with tool filtering and spawn_subtask capability.
- Append-only JSONL event log for background task replay.
- SSRF protection for HttpTool and WebFetchTool.
- CLI commands for background-agent, shared space, deliverables, and triggers.
- CLI `memory store` command.
- CLI `skill update` command.
- `get_skill_context` MCP tool (renamed from `skill_execute`).

### Changed
- Agent update now auto-clears Codex-specific fields when switching to non-Codex models.

### Fixed
- TOCTOU race condition in secret create/update operations.
- Resource leak in task tracking cleanup.
- UTF-8 panic in LLM error body truncation.
- Lock poisoning panic in LLM client (switched to parking_lot::RwLock).

## [0.3.1] - 2026-02-15

### Fixed
- Bypass that prevented failover for background tasks removed.
- Unified retry logic with RetryingLlmClient decorator.

## [0.3.0] - 2026-02-14

### Changed
- Major refactoring to agent-centric architecture.

## [0.2.1] - 2026-02-06

### Added
- LLM client retry handling for Anthropic and OpenAI with exponential backoff.
- Structured LLM HTTP errors with retry metadata support.
- Streaming agent execution updates to chat UI (including tool call stream events).

### Changed
- Chat dispatcher response timeout default increased from 60 seconds to 300 seconds.

### Fixed
- Clippy/doc lint regressions introduced in recent retry and streaming changes.
- Post-merge CI/build issues around streaming agent execution paths.

