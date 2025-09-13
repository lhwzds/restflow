use crate::node::agent::AgentNode;
use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredAgent {
    pub id: String,
    pub name: String,
    pub agent: AgentNode,
}
const AGENT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("agents");

pub struct AgentStorage {
    db: Arc<Database>,
}

impl AgentStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Create table
        let write_txn = db.begin_write()?;
        write_txn.open_table(AGENT_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }
    pub fn insert_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
        let stored_agent = StoredAgent {
            id: Uuid::new_v4().to_string(),
            name,
            agent,
        };
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(AGENT_TABLE)?;
            let json_bytes = serde_json::to_vec(&stored_agent)?;
            table.insert(stored_agent.id.as_str(), json_bytes.as_slice())?;
        }
        write_txn.commit()?;

        Ok(stored_agent)
    }

    pub fn get_agent(&self, id: String) -> Result<Option<StoredAgent>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(AGENT_TABLE)?;
        if let Some(value) = table.get(id.as_str())? {
            let agent: StoredAgent = serde_json::from_slice(value.value())?;
            Ok(Some(agent))
        } else {
            Ok(None)
        }
    }

    pub fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(AGENT_TABLE)?;
        let mut agents = Vec::new();
        for item in table.iter()? {
            let (_, value) = item?;
            let agent: StoredAgent = serde_json::from_slice(value.value())?;
            agents.push(agent);
        }
        Ok(agents)
    }

    pub fn update_agent(
        &self,
        id: String,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<Option<StoredAgent>> {
        let mut existing_agent = self
            .get_agent(id.clone())?
            .ok_or_else(|| anyhow::anyhow!("Agent {}not found", id))?;
        if let Some(new_name) = name {
            existing_agent.name = new_name;
        };

        if let Some(new_agent) = agent {
            existing_agent.agent = new_agent;
        };

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(AGENT_TABLE)?;
            let json_bytes = serde_json::to_vec(&existing_agent)?;
            table.insert(existing_agent.id.as_str(), json_bytes.as_slice())?;
        }
        write_txn.commit()?;

        Ok(Some(existing_agent))
    }

    pub fn delete_agent(&self, id: String) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let deleted = {
            let mut table = write_txn.open_table(AGENT_TABLE)?;
            table.remove(id.as_str())?.is_some()
        };
        write_txn.commit()?;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_agent_node() -> AgentNode {
        AgentNode {
            model: "gpt-4.1".to_string(),
            prompt: "You are a helpful assistant".to_string(),
            temperature: 0.7,
            api_key: Some("test_key".to_string()),
            tools: Some(vec!["add".to_string()]),
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
            .insert_agent("Test Agent".to_string(), agent_node)
            .unwrap();

        assert!(!stored.id.is_empty());
        assert_eq!(stored.name, "Test Agent");

        let retrieved = storage.get_agent(stored.id.clone()).unwrap();
        assert!(retrieved.is_some());

        let agent = retrieved.unwrap();
        assert_eq!(agent.name, "Test Agent");
        assert_eq!(agent.agent.model, "gpt-4.1");
    }

    #[test]
    fn test_list_agents() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        storage
            .insert_agent("Agent 1".to_string(), create_test_agent_node())
            .unwrap();
        storage
            .insert_agent("Agent 2".to_string(), create_test_agent_node())
            .unwrap();
        storage
            .insert_agent("Agent 3".to_string(), create_test_agent_node())
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
            .insert_agent("Original Name".to_string(), create_test_agent_node())
            .unwrap();
        let updated = storage
            .update_agent(stored.id.clone(), Some("Updated Name".to_string()), None)
            .unwrap()
            .unwrap();

        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.agent.model, "gpt-4.1");

        let mut new_agent_node = create_test_agent_node();
        new_agent_node.temperature = 0.9;

        let updated2 = storage
            .update_agent(stored.id.clone(), None, Some(new_agent_node))
            .unwrap()
            .unwrap();

        assert_eq!(updated2.name, "Updated Name");
        assert_eq!(updated2.agent.temperature, 0.9);
    }

    #[test]
    fn test_delete_agent() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let stored = storage
            .insert_agent("To Delete".to_string(), create_test_agent_node())
            .unwrap();
        let deleted = storage.delete_agent(stored.id.clone()).unwrap();
        assert!(deleted);

        let retrieved = storage.get_agent(stored.id.clone()).unwrap();
        assert!(retrieved.is_none());

        let deleted_again = storage.delete_agent(stored.id).unwrap();
        assert!(!deleted_again);
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
