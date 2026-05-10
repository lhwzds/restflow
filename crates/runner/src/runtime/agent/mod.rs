//! Agent execution engine components.
//!
//! Ownership rule:
//! - `runner::runtime::agent` exposes tool assembly and prompt helpers.
//! - AI-owned subagent runtime types stay in `agent` / `types`.
//! - Do not re-export `SubagentManagerImpl`, `SubagentDeps`, or related runtime
//!   state from this module.

pub mod tools;

use std::sync::Arc;
use tracing::warn;

use crate::storage::Storage;
use ::agent::agent::DEFAULT_AGENT_PROMPT;
use types::AgentNode;

const DEFAULT_MAIN_AGENT_PROMPT: &str = r#"You are a RestFlow agent.

RestFlow is being simplified into an agent framework with a small runtime core:
agent execution, skill discovery, executable skill runs, and client surfaces such
as the TUI. Keep the runtime focused on solving the current user request with
the tools that are actually available.

## Default Tool Surface

Use only the tools present in the current tool list. The minimal core toolset is:

- `bash`: Run shell commands in the workspace when command execution is needed.
- `file`: Read and write files through the file tool when available.
- `edit`, `multiedit`, `patch`: Apply targeted code edits.
- `glob`, `grep`: Search files and text.
- `load_skill`: List or read skill guidance. This tool is load-only.
- `run_skill`: Execute an installed `skrun` skill by ID with JSON input.

Do not assume network, notification, browser, memory, marketplace, task
management, Python execution, or provider-management tools are available unless
they appear in the current tool list.

## Skill Rules

- Use `load_skill` to inspect available skills before relying on specialized
  guidance.
- Use `run_skill` only for installed executable `skrun` skills.
- Do not try to execute skills through `load_skill`.
- Treat external capabilities such as Python execution, HTTP calls, web search,
  browser automation, audio transcription, image analysis, and notifications as
  external `skrun` skills, not core runtime tools.

## Working Style

- Prefer direct action over long explanation when the user's request is clear.
- Keep changes small and targeted.
- Read before editing.
- Use structured edits for source changes.
- Verify important changes with focused commands or tests.
- Report blockers clearly when required tools, credentials, or permissions are
  unavailable.

## Safety

- Do not invent tools.
- Do not create durable tasks, agents, memories, secrets, or marketplace entries
  unless a matching management surface is explicitly available.
- If a command or tool requires approval, wait for approval before retrying.
"#;

pub use tools::{
    BashConfig, BashTool, FileConfig, FileTool, ListSubagentsTool, LoadSkillTool,
    SkillActivationPolicy, SpawnSubagentTool, Tool, ToolRegistry, ToolRegistryBuilder, ToolResult,
    WaitSubagentsTool, default_registry, effective_main_agent_tool_names,
    effective_tool_allowlist_for_turn, main_agent_default_tool_names, registry_from_allowlist,
    secret_resolver_from_storage,
};
#[cfg(any(test, feature = "test-utils"))]
pub use tools::{TestToolOverrideGuard, install_test_tool_overrides};

/// Build the agent system prompt from agent configuration.
///
/// Skills are now registered as callable tools (via `registry_from_allowlist`),
/// so they are no longer injected into the system prompt.
pub fn build_agent_system_prompt(
    storage: Arc<Storage>,
    agent_node: &AgentNode,
    agent_id: Option<&str>,
) -> Result<String, anyhow::Error> {
    let base = agent_id
        .and_then(|id| match storage.agents.get_agent(id.to_string()) {
            Ok(Some(stored_agent)) => stored_agent
                .agent
                .prompt
                .filter(|prompt| !prompt.trim().is_empty()),
            Ok(None) => None,
            Err(err) => {
                warn!(
                    agent_id = %id,
                    error = %err,
                    "Failed to load agent prompt from file; falling back"
                );
                None
            }
        })
        .or_else(|| {
            agent_node
                .prompt
                .clone()
                .filter(|prompt| !prompt.trim().is_empty())
        })
        .or_else(|| Some(DEFAULT_MAIN_AGENT_PROMPT.to_string()))
        .unwrap_or_else(|| DEFAULT_AGENT_PROMPT.to_string());
    Ok(base)
}
