//! Agent execution engine components.

mod skills;
pub mod tools;

use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

use crate::models::AgentNode;
use crate::prompt_files;
use crate::storage::Storage;

pub use skills::{ProcessedSkill, SkillLoader};
pub use tools::{
    BashConfig, BashTool, EmailTool, FileConfig, FileTool, HttpTool, ListAgentsTool,
    SpawnAgentTool, SpawnTool, SubagentDeps, SubagentSpawner, TelegramTool, Tool, ToolRegistry,
    ToolRegistryBuilder, ToolResult, UseSkillTool, WaitAgentsTool, default_registry,
    effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    secret_resolver_from_storage,
};

pub fn build_agent_system_prompt(
    storage: Arc<Storage>,
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
        .unwrap_or_else(|| "You are a helpful AI assistant.".to_string());
    let skill_ids = agent_node.skills.clone().unwrap_or_default();
    let skill_vars: Option<HashMap<String, String>> = agent_node.skill_variables.clone();

    let loader = SkillLoader::new(storage);
    loader.build_system_prompt(&base, &skill_ids, skill_vars.as_ref())
}
