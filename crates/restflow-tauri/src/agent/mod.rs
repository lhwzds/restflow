//! Unified agent components.

mod skills;
pub mod tools;

use std::collections::HashMap;
use std::sync::Arc;

use restflow_core::models::AgentNode;
use restflow_core::storage::Storage;

pub use restflow_ai::agent::{ExecutionResult, UnifiedAgent, UnifiedAgentConfig};
pub use skills::{ProcessedSkill, SkillLoader};
pub use tools::{
    BashConfig, BashTool, EmailTool, FileConfig, FileTool, HttpTool, ListAgentsTool, PythonTool,
    SpawnAgentTool, SpawnTool, SubagentDeps, SubagentSpawner, TelegramTool, Tool, ToolRegistry,
    ToolRegistryBuilder, ToolResult, UseSkillTool, WaitAgentsTool, default_registry,
    effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    secret_resolver_from_storage,
};

pub fn build_agent_system_prompt(
    storage: Arc<Storage>,
    agent_node: &AgentNode,
) -> Result<String, anyhow::Error> {
    let base = agent_node
        .prompt
        .clone()
        .unwrap_or_else(|| "You are a helpful AI assistant.".to_string());
    let skill_ids = agent_node.skills.clone().unwrap_or_default();
    let skill_vars: Option<HashMap<String, String>> = agent_node.skill_variables.clone();

    let loader = SkillLoader::new(storage);
    loader.build_system_prompt(&base, &skill_ids, skill_vars.as_ref())
}
