//! Agent execution engine components.

pub mod tools;

use std::sync::Arc;
use tracing::warn;

use crate::models::AgentNode;
use crate::prompt_files;
use crate::storage::Storage;
use restflow_ai::agent::DEFAULT_AGENT_PROMPT;

pub use tools::{
    BashConfig, BashTool, EmailTool, FileConfig, FileTool, HttpTool, ListAgentsTool,
    SpawnAgentTool, SpawnTool, SubagentDeps, SubagentSpawner, TelegramTool, Tool, ToolRegistry,
    ToolRegistryBuilder, ToolResult, UseSkillTool, WaitAgentsTool, default_registry,
    effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    secret_resolver_from_storage,
};

/// Build the agent system prompt from agent configuration.
///
/// Skills are now registered as callable tools (via `registry_from_allowlist`),
/// so they are no longer injected into the system prompt.
pub fn build_agent_system_prompt(
    _storage: Arc<Storage>,
    agent_node: &AgentNode,
    agent_id: Option<&str>,
) -> Result<String, anyhow::Error> {
    let base = agent_id
        .and_then(|id| match prompt_files::load_agent_prompt(id) {
            Ok(prompt) => prompt,
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
        .or_else(|| prompt_files::load_default_main_agent_prompt().ok())
        .unwrap_or_else(|| DEFAULT_AGENT_PROMPT.to_string());
    Ok(base)
}
