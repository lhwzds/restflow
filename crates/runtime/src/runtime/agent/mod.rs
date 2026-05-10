//! Agent execution engine components.
//!
//! Ownership rule:
//! - `runtime::runtime::agent` exposes tool assembly and prompt helpers.
//! - AI-owned subagent runtime types stay in `agent` / `types`.
//! - Do not re-export `SubagentManagerImpl`, `SubagentDeps`, or related runtime
//!   state from this module.

pub mod tools;

use std::sync::Arc;
use tracing::warn;

use crate::storage::Storage;
use ::agent::agent::DEFAULT_AGENT_PROMPT;
use types::AgentNode;

const DEFAULT_MAIN_AGENT_PROMPT: &str = include_str!("../../../prompts/agents/default.md");

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
