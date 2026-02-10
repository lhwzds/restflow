//! Typed agent storage wrapper.

use crate::models::AgentNode;
use crate::prompt_files;
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
#[derive(Clone)]
pub struct AgentStorage {
    inner: restflow_storage::AgentStorage,
}

impl AgentStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::AgentStorage::new(db)?,
        })
    }

    pub fn create_agent(&self, name: String, mut agent: AgentNode) -> Result<StoredAgent> {
        let now = time_utils::now_ms();
        let id = Uuid::new_v4().to_string();

        // Prompt content is file-backed under ~/.restflow/agents/{id}.md, not stored in DB.
        let prompt_override = agent.prompt.take();
        prompt_files::ensure_agent_prompt_file(&id, prompt_override.as_deref())?;

        let stored_agent = StoredAgent {
            id,
            name,
            agent,
            created_at: Some(now),
            updated_at: Some(now),
        };

        self.persist_without_prompt(&stored_agent)?;

        self.hydrate_prompt_from_file(stored_agent)
    }

    pub fn get_agent(&self, id: String) -> Result<Option<StoredAgent>> {
        if let Some(bytes) = self.inner.get_raw(&id)? {
            let agent: StoredAgent = serde_json::from_slice(&bytes)?;
            Ok(Some(self.hydrate_prompt_from_file(agent)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        let agents = self.inner.list_raw()?;
        let mut result = Vec::new();
        for (_, bytes) in agents {
            let agent: StoredAgent = serde_json::from_slice(&bytes)?;
            result.push(self.hydrate_prompt_from_file(agent)?);
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

        if let Some(mut new_agent) = agent {
            let prompt_override = new_agent.prompt.take();
            if prompt_override.is_some() {
                prompt_files::ensure_agent_prompt_file(
                    &existing_agent.id,
                    prompt_override.as_deref(),
                )?;
            }
            existing_agent.agent = new_agent;
        }

        let now = time_utils::now_ms();
        existing_agent.updated_at = Some(now);

        self.persist_without_prompt(&existing_agent)?;

        self.hydrate_prompt_from_file(existing_agent)
    }

    pub fn delete_agent(&self, id: String) -> Result<()> {
        if !self.inner.delete(&id)? {
            return Err(anyhow::anyhow!("Agent {} not found", id));
        }
        let _ = prompt_files::delete_agent_prompt_file(&id);
        Ok(())
    }

    fn hydrate_prompt_from_file(&self, mut stored: StoredAgent) -> Result<StoredAgent> {
        stored.agent.prompt = prompt_files::load_agent_prompt(&stored.id)?;
        Ok(stored)
    }

    fn persist_without_prompt(&self, stored: &StoredAgent) -> Result<()> {
        let mut scrubbed = stored.clone();
        scrubbed.agent.prompt = None;
        let json_bytes = serde_json::to_vec(&scrubbed)?;
        self.inner.put_raw(&scrubbed.id, &json_bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AIModel;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    const AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn create_test_agent_node() -> AgentNode {
        use crate::models::ApiKeyConfig;

        AgentNode {
            model: Some(AIModel::ClaudeSonnet4_5),
            prompt: Some("You are a helpful assistant".to_string()),
            temperature: Some(0.7),
            codex_cli_reasoning_effort: None,
            codex_cli_execution_mode: None,
            api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
            tools: Some(vec!["add".to_string()]),
            skills: None,
            skill_variables: None,
        }
    }

    #[test]
    fn test_insert_and_get_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
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
        assert!(prompts_dir.join(format!("{}.md", stored.id)).exists());
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_list_agents() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
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
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_update_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
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
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_update_name_does_not_rehydrate_prompt_into_db() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
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
        assert!(updated.agent.prompt.is_some());

        let raw = storage.inner.get_raw(&stored.id).unwrap().unwrap();
        let persisted: StoredAgent = serde_json::from_slice(&raw).unwrap();
        assert!(persisted.agent.prompt.is_none());

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_delete_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
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
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_get_nonexistent_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let result = storage.get_agent("nonexistent".to_string()).unwrap();
        assert!(result.is_none());
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_update_nonexistent_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
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
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }
}
