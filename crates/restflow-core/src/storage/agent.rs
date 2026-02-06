//! Typed agent storage wrapper.

use crate::models::AgentNode;
use anyhow::Result;
use redb::Database;
use restflow_storage::SimpleStorage;
use restflow_storage::time_utils;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;
use uuid::Uuid;

/// Stored agent with metadata
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[ts(export)]
pub struct StoredAgent {
    pub id: String,
    pub name: String,
    pub agent: AgentNode,
    #[ts(optional, type = "number")]
    pub created_at: Option<i64>,
    #[ts(optional, type = "number")]
    pub updated_at: Option<i64>,
}

/// Typed agent storage wrapper around restflow-storage::AgentStorage.
pub struct AgentStorage {
    inner: restflow_storage::AgentStorage,
}

impl AgentStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::AgentStorage::new(db)?,
        })
    }

    pub fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
        let now = time_utils::now_ms();

        let stored_agent = StoredAgent {
            id: Uuid::new_v4().to_string(),
            name,
            agent,
            created_at: Some(now),
            updated_at: Some(now),
        };

        let json_bytes = serde_json::to_vec(&stored_agent)?;
        self.inner.put_raw(&stored_agent.id, &json_bytes)?;

        Ok(stored_agent)
    }

    pub fn get_agent(&self, id: String) -> Result<Option<StoredAgent>> {
        if let Some(bytes) = self.inner.get_raw(&id)? {
            let agent: StoredAgent = serde_json::from_slice(&bytes)?;
            Ok(Some(agent))
        } else {
            Ok(None)
        }
    }

    pub fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        let agents = self.inner.list_raw()?;
        let mut result = Vec::new();
        for (_, bytes) in agents {
            let agent: StoredAgent = serde_json::from_slice(&bytes)?;
            result.push(agent);
        }
        Ok(result)
    }

    pub fn update_agent(
        &self,
        id: String,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<StoredAgent> {
        let mut existing_agent = self
            .get_agent(id.clone())?
            .ok_or_else(|| anyhow::anyhow!("Agent {} not found", id))?;

        if let Some(new_name) = name {
            existing_agent.name = new_name;
        }

        if let Some(new_agent) = agent {
            existing_agent.agent = new_agent;
        }

        let now = time_utils::now_ms();
        existing_agent.updated_at = Some(now);

        let json_bytes = serde_json::to_vec(&existing_agent)?;
        self.inner.put_raw(&existing_agent.id, &json_bytes)?;

        Ok(existing_agent)
    }

    pub fn delete_agent(&self, id: String) -> Result<()> {
        if !self.inner.delete(&id)? {
            return Err(anyhow::anyhow!("Agent {} not found", id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AIModel;
    use tempfile::tempdir;

    fn create_test_agent_node() -> AgentNode {
        use crate::models::ApiKeyConfig;

        AgentNode {
            model: Some(AIModel::ClaudeSonnet4_5),
            prompt: Some("You are a helpful assistant".to_string()),
            temperature: Some(0.7),
            api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
            tools: Some(vec!["add".to_string()]),
            skills: None,
            skill_variables: None,
        }
    }

    #[test]
    fn test_insert_and_get_agent() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let agent_node = create_test_agent_node();
        let stored = storage
            .create_agent("Test Agent".to_string(), agent_node)
            .unwrap();

        assert!(!stored.id.is_empty());
        assert_eq!(stored.name, "Test Agent");

        let retrieved = storage.get_agent(stored.id.clone()).unwrap();
        assert!(retrieved.is_some());

        let agent = retrieved.unwrap();
        assert_eq!(agent.name, "Test Agent");
        assert_eq!(agent.agent.model, Some(AIModel::ClaudeSonnet4_5));
    }

    #[test]
    fn test_list_agents() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        storage
            .create_agent("Agent 1".to_string(), create_test_agent_node())
            .unwrap();
        storage
            .create_agent("Agent 2".to_string(), create_test_agent_node())
            .unwrap();
        storage
            .create_agent("Agent 3".to_string(), create_test_agent_node())
            .unwrap();

        let agents = storage.list_agents().unwrap();
        assert_eq!(agents.len(), 3);

        let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
        assert!(names.contains(&"Agent 1".to_string()));
        assert!(names.contains(&"Agent 2".to_string()));
        assert!(names.contains(&"Agent 3".to_string()));
    }

    #[test]
    fn test_update_agent() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let stored = storage
            .create_agent("Original Name".to_string(), create_test_agent_node())
            .unwrap();
        let updated = storage
            .update_agent(stored.id.clone(), Some("Updated Name".to_string()), None)
            .unwrap();

        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.agent.model, Some(AIModel::ClaudeSonnet4_5));

        let mut new_agent_node = create_test_agent_node();
        new_agent_node.temperature = Some(0.9);

        let updated2 = storage
            .update_agent(stored.id.clone(), None, Some(new_agent_node))
            .unwrap();

        assert_eq!(updated2.name, "Updated Name");
        assert_eq!(updated2.agent.temperature, Some(0.9));
    }

    #[test]
    fn test_delete_agent() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let stored = storage
            .create_agent("To Delete".to_string(), create_test_agent_node())
            .unwrap();
        storage.delete_agent(stored.id.clone()).unwrap();

        let retrieved = storage.get_agent(stored.id.clone()).unwrap();
        assert!(retrieved.is_none());

        let deleted_again = storage.delete_agent(stored.id);
        assert!(deleted_again.is_err());
        assert!(deleted_again.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_nonexistent_agent() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let result = storage.get_agent("nonexistent".to_string()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_nonexistent_agent() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let result = storage.update_agent(
            "nonexistent".to_string(),
            Some("New Name".to_string()),
            None,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
