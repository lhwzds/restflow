use crate::{AppCore, node::agent::AgentNode, storage::agent::StoredAgent};
use anyhow::{Result, Context};
use std::sync::Arc;

pub async fn list_agents(core: &Arc<AppCore>) -> Result<Vec<StoredAgent>> {
    core.storage.agents.list_agents()
        .context("Failed to list agents")
}

pub async fn get_agent(core: &Arc<AppCore>, id: &str) -> Result<StoredAgent> {
    core.storage.agents.get_agent(id.to_string())
        .with_context(|| format!("Failed to get agent {}", id))?
        .ok_or_else(|| anyhow::anyhow!("Agent {} not found", id))
}

pub async fn create_agent(core: &Arc<AppCore>, name: String, agent: AgentNode) -> Result<StoredAgent> {
    core.storage.agents.insert_agent(name.clone(), agent)
        .with_context(|| format!("Failed to create agent {}", name))
}

pub async fn update_agent(
    core: &Arc<AppCore>,
    id: &str,
    name: Option<String>,
    agent: Option<AgentNode>
) -> Result<StoredAgent> {
    core.storage.agents.update_agent(id.to_string(), name, agent)
        .with_context(|| format!("Failed to update agent {}", id))
}

pub async fn delete_agent(core: &Arc<AppCore>, id: &str) -> Result<()> {
    core.storage.agents.delete_agent(id.to_string())
        .with_context(|| format!("Failed to delete agent {}", id))
}

pub async fn execute_agent(
    core: &Arc<AppCore>,
    id: &str,
    input: &str
) -> Result<String> {
    let stored_agent = get_agent(core, id).await?;

    stored_agent.agent.execute(input).await
        .with_context(|| format!("Failed to execute agent {}", id))
}

pub async fn execute_agent_inline(agent: AgentNode, input: &str) -> Result<String> {
    agent.execute(input).await
        .context("Failed to execute inline agent")
}