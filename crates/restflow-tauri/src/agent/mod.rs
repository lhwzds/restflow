//! Agent execution engine components.

pub mod tools;

use std::sync::Arc;

use restflow_core::models::AgentNode;
use restflow_core::storage::Storage;

pub use tools::{
    BashConfig, BashTool, EmailTool, FileConfig, FileTool, HttpTool, ListAgentsTool,
    SpawnAgentTool, SpawnTool, SubagentDeps, SubagentSpawner, TelegramTool, Tool, ToolRegistry,
    ToolRegistryBuilder, ToolResult, UseSkillTool, WaitAgentsTool, default_registry,
    effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    secret_resolver_from_storage,
};

/// Build agent system prompt — delegates to the canonical implementation in restflow-core.
pub fn build_agent_system_prompt(
    storage: Arc<Storage>,
    agent_node: &AgentNode,
) -> Result<String, anyhow::Error> {
    restflow_core::runtime::agent::build_agent_system_prompt(storage, agent_node, None)
}
