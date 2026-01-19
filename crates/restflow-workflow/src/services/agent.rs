//! Agent service layer
//!
//! Agent execution is handled by restflow-ai's AgentExecutor.
//! Use restflow-server's /api/agents endpoints for agent execution.

use crate::{node::agent::AgentNode, storage::agent::StoredAgent, AppCore};
use anyhow::{Context, Result};
use std::sync::Arc;

pub async fn list_agents(core: &Arc<AppCore>) -> Result<Vec<StoredAgent>> {
    core.storage
        .agents
        .list_agents()
        .context("Failed to list agents")
}

pub async fn get_agent(core: &Arc<AppCore>, id: &str) -> Result<StoredAgent> {
    core.storage
        .agents
        .get_agent(id.to_string())
        .with_context(|| format!("Failed to get agent {}", id))?
        .ok_or_else(|| anyhow::anyhow!("Agent {} not found", id))
}

pub async fn create_agent(
    core: &Arc<AppCore>,
    name: String,
    agent: AgentNode,
) -> Result<StoredAgent> {
    core.storage
        .agents
        .create_agent(name.clone(), agent)
        .with_context(|| format!("Failed to create agent {}", name))
}

pub async fn update_agent(
    core: &Arc<AppCore>,
    id: &str,
    name: Option<String>,
    agent: Option<AgentNode>,
) -> Result<StoredAgent> {
    core.storage
        .agents
        .update_agent(id.to_string(), name, agent)
        .with_context(|| format!("Failed to update agent {}", id))
}

pub async fn delete_agent(core: &Arc<AppCore>, id: &str) -> Result<()> {
    core.storage
        .agents
        .delete_agent(id.to_string())
        .with_context(|| format!("Failed to delete agent {}", id))
}

// TODO: Implement agent execution using restflow-ai AgentExecutor
// For now, agent execution should use restflow-server's /api/agents/execute endpoint
