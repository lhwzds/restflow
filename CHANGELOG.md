# Changelog

All notable changes to this project will be documented in this file.

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

