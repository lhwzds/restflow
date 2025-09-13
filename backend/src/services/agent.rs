use crate::{AppCore, node::agent::AgentNode, storage::agent::StoredAgent};
use std::sync::Arc;

pub async fn list_agents(core: &Arc<AppCore>) -> Result<Vec<StoredAgent>, String> {
    core.storage.agents.list_agents()
        .map_err(|e| format!("Failed to list agents: {}", e))
}

pub async fn get_agent(core: &Arc<AppCore>, id: &str) -> Result<StoredAgent, String> {
    core.storage.agents.get_agent(id.to_string())
        .map_err(|e| format!("Failed to get agent: {}", e))?
        .ok_or_else(|| format!("Agent {} not found", id))
}

pub async fn create_agent(core: &Arc<AppCore>, name: String, agent: AgentNode) -> Result<StoredAgent, String> {
    core.storage.agents.insert_agent(name, agent)
        .map_err(|e| format!("Failed to create agent: {}", e))
}

pub async fn update_agent(
    core: &Arc<AppCore>, 
    id: &str, 
    name: Option<String>, 
    agent: Option<AgentNode>
) -> Result<StoredAgent, String> {
    core.storage.agents.update_agent(id.to_string(), name, agent)
        .map_err(|e| format!("Failed to update agent: {}", e))?
        .ok_or_else(|| format!("Agent {} not found", id))
}

pub async fn delete_agent(core: &Arc<AppCore>, id: &str) -> Result<bool, String> {
    core.storage.agents.delete_agent(id.to_string())
        .map_err(|e| format!("Failed to delete agent: {}", e))
}

pub async fn execute_agent(
    core: &Arc<AppCore>, 
    id: &str, 
    input: &str
) -> Result<String, String> {
    let stored_agent = get_agent(core, id).await?;
    
    stored_agent.agent.execute(input).await
        .map_err(|e| format!("Failed to execute agent: {}", e))
}

pub async fn execute_agent_inline(agent: AgentNode, input: &str) -> Result<String, String> {
    agent.execute(input).await
        .map_err(|e| format!("Failed to execute agent: {}", e))
}