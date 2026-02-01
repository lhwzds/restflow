//! Agent service layer
//!
//! Agent execution is handled by restflow-ai's AgentExecutor.
//! Use restflow-server's /api/agents endpoints for agent execution.

use crate::{AppCore, models::AgentNode, storage::agent::StoredAgent};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AIModel, ApiKeyConfig};
    use tempfile::tempdir;

    async fn create_test_core() -> Arc<AppCore> {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap())
    }

    fn create_test_agent_node(prompt: &str) -> AgentNode {
        AgentNode {
            model: AIModel::ClaudeSonnet4_5,
            prompt: Some(prompt.to_string()),
            temperature: Some(0.7),
            api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
            tools: Some(vec!["add".to_string()]),
        }
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        let core = create_test_core().await;
        let agents = list_agents(&core).await.unwrap();
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_create_and_get_agent() {
        let core = create_test_core().await;

        let agent_node = create_test_agent_node("You are a helpful assistant");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        assert!(!created.id.is_empty());
        assert_eq!(created.name, "Test Agent");
        assert_eq!(
            created.agent.prompt,
            Some("You are a helpful assistant".to_string())
        );

        let retrieved = get_agent(&core, &created.id).await.unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, "Test Agent");
    }

    #[tokio::test]
    async fn test_list_agents_multiple() {
        let core = create_test_core().await;

        let agent1 = create_test_agent_node("Agent 1 prompt");
        let agent2 = create_test_agent_node("Agent 2 prompt");
        let agent3 = create_test_agent_node("Agent 3 prompt");

        create_agent(&core, "Agent 1".to_string(), agent1)
            .await
            .unwrap();
        create_agent(&core, "Agent 2".to_string(), agent2)
            .await
            .unwrap();
        create_agent(&core, "Agent 3".to_string(), agent3)
            .await
            .unwrap();

        let agents = list_agents(&core).await.unwrap();
        assert_eq!(agents.len(), 3);

        let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
        assert!(names.contains(&"Agent 1".to_string()));
        assert!(names.contains(&"Agent 2".to_string()));
        assert!(names.contains(&"Agent 3".to_string()));
    }

    #[tokio::test]
    async fn test_update_agent_name() {
        let core = create_test_core().await;

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "Original Name".to_string(), agent_node)
            .await
            .unwrap();

        let updated = update_agent(&core, &created.id, Some("Updated Name".to_string()), None)
            .await
            .unwrap();

        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.agent.prompt, Some("Test prompt".to_string()));
    }

    #[tokio::test]
    async fn test_update_agent_config() {
        let core = create_test_core().await;

        let agent_node = create_test_agent_node("Original prompt");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        let mut new_agent_node = create_test_agent_node("Updated prompt");
        new_agent_node.temperature = Some(0.9);
        new_agent_node.model = Some(AIModel::Gpt5Mini);

        let updated = update_agent(&core, &created.id, None, Some(new_agent_node))
            .await
            .unwrap();

        assert_eq!(updated.name, "Test Agent"); // Name unchanged
        assert_eq!(updated.agent.prompt, Some("Updated prompt".to_string()));
        assert_eq!(updated.agent.temperature, Some(0.9));
        assert_eq!(updated.agent.model, AIModel::Gpt5Mini);
    }

    #[tokio::test]
    async fn test_delete_agent() {
        let core = create_test_core().await;

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "To Delete".to_string(), agent_node)
            .await
            .unwrap();

        // Verify it exists
        let retrieved = get_agent(&core, &created.id).await;
        assert!(retrieved.is_ok());

        // Delete it
        delete_agent(&core, &created.id).await.unwrap();

        // Verify it's gone
        let result = get_agent(&core, &created.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_nonexistent_agent_fails() {
        let core = create_test_core().await;

        let result = get_agent(&core, "nonexistent-id").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_create_agent_generates_uuid() {
        let core = create_test_core().await;

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        // Verify ID is a valid UUID format
        assert!(!created.id.is_empty());
        assert!(created.id.contains('-')); // UUIDs contain hyphens
        assert_eq!(created.id.len(), 36); // Standard UUID length
    }

    #[tokio::test]
    async fn test_create_agent_sets_timestamps() {
        let core = create_test_core().await;

        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Verify timestamps are set and within reasonable bounds
        assert!(created.created_at.is_some());
        assert!(created.updated_at.is_some());

        let created_at = created.created_at.unwrap();
        let updated_at = created.updated_at.unwrap();

        assert!(created_at >= before && created_at <= after);
        assert!(updated_at >= before && updated_at <= after);
        assert_eq!(created_at, updated_at); // Should be same on creation
    }

    #[tokio::test]
    async fn test_update_agent_updates_timestamp() {
        let core = create_test_core().await;

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        // Small delay to ensure timestamp difference
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let updated = update_agent(&core, &created.id, Some("Updated Name".to_string()), None)
            .await
            .unwrap();

        // Updated timestamp should be newer
        assert!(updated.updated_at.unwrap() > created.updated_at.unwrap());
        // Created timestamp should remain the same
        assert_eq!(updated.created_at, created.created_at);
    }
}
