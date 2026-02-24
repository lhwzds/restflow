//! AgentStore adapter backed by AgentStorage.

use crate::storage::skill::SkillStorage;
use crate::storage::{AgentStorage, BackgroundAgentStorage, SecretStorage};
use restflow_tools::ToolError;
use restflow_traits::store::{AgentCreateRequest, AgentStore, AgentUpdateRequest};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct AgentStoreAdapter {
    storage: AgentStorage,
    skills: SkillStorage,
    secrets: SecretStorage,
    background_agent_storage: BackgroundAgentStorage,
    known_tools: Arc<RwLock<HashSet<String>>>,
}

impl AgentStoreAdapter {
    pub fn new(
        storage: AgentStorage,
        skills: SkillStorage,
        secrets: SecretStorage,
        background_agent_storage: BackgroundAgentStorage,
        known_tools: Arc<RwLock<HashSet<String>>>,
    ) -> Self {
        Self {
            storage,
            skills,
            secrets,
            background_agent_storage,
            known_tools,
        }
    }

    fn parse_agent_node(value: Value) -> Result<crate::models::AgentNode, ToolError> {
        serde_json::from_value(value)
            .map_err(|e| ToolError::Tool(format!("Invalid agent payload: {}", e)))
    }

    fn validate_agent_node(&self, agent: &crate::models::AgentNode) -> Result<(), ToolError> {
        if let Err(errors) = agent.validate() {
            return Err(ToolError::Tool(crate::models::encode_validation_error(
                errors,
            )));
        }

        let mut errors = Vec::new();
        if let Some(tools) = &agent.tools {
            for tool_name in tools {
                let normalized = tool_name.trim();
                if normalized.is_empty() {
                    errors.push(crate::models::ValidationError::new(
                        "tools",
                        "tool name must not be empty",
                    ));
                    continue;
                }
                let is_known = self
                    .known_tools
                    .read()
                    .map(|set| set.contains(normalized))
                    .unwrap_or(false);
                if !is_known {
                    errors.push(crate::models::ValidationError::new(
                        "tools",
                        format!("unknown tool: {}", normalized),
                    ));
                }
            }
        }

        if let Some(skills) = &agent.skills {
            let skill_ids: Vec<&str> = skills
                .iter()
                .map(|s| s.trim())
                .filter(|s| {
                    if s.is_empty() {
                        errors.push(crate::models::ValidationError::new(
                            "skills",
                            "skill ID must not be empty",
                        ));
                        false
                    } else {
                        true
                    }
                })
                .collect();
            match self.skills.exists_many(&skill_ids) {
                Ok(existing) => {
                    for &id in &skill_ids {
                        if !existing.contains(id) {
                            errors.push(crate::models::ValidationError::new(
                                "skills",
                                format!("unknown skill: {}", id),
                            ));
                        }
                    }
                }
                Err(err) => errors.push(crate::models::ValidationError::new(
                    "skills",
                    format!("failed to verify skills: {}", err),
                )),
            }
        }

        if let Some(crate::models::ApiKeyConfig::Secret(secret_name)) = &agent.api_key_config {
            let normalized = secret_name.trim();
            if !normalized.is_empty() {
                match self.secrets.has_secret(normalized) {
                    Ok(true) => {}
                    Ok(false) => errors.push(crate::models::ValidationError::new(
                        "api_key_config",
                        format!("secret not found: {}", normalized),
                    )),
                    Err(err) => errors.push(crate::models::ValidationError::new(
                        "api_key_config",
                        format!("failed to verify secret '{}': {}", normalized, err),
                    )),
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ToolError::Tool(crate::models::encode_validation_error(
                errors,
            )))
        }
    }
}

impl AgentStore for AgentStoreAdapter {
    fn list_agents(&self) -> restflow_tools::Result<Value> {
        let agents = self.storage.list_agents()?;
        serde_json::to_value(agents).map_err(ToolError::from)
    }

    fn get_agent(&self, id: &str) -> restflow_tools::Result<Value> {
        let agent = self
            .storage
            .get_agent(id.to_string())?
            .ok_or_else(|| ToolError::Tool(format!("Agent {} not found", id)))?;
        serde_json::to_value(agent).map_err(ToolError::from)
    }

    fn create_agent(&self, request: AgentCreateRequest) -> restflow_tools::Result<Value> {
        let agent = Self::parse_agent_node(request.agent)?;
        self.validate_agent_node(&agent)?;
        let created = self.storage.create_agent(request.name, agent)?;
        serde_json::to_value(created).map_err(ToolError::from)
    }

    fn update_agent(&self, request: AgentUpdateRequest) -> restflow_tools::Result<Value> {
        let agent = match request.agent {
            Some(value) => {
                let node = Self::parse_agent_node(value)?;
                self.validate_agent_node(&node)?;
                Some(node)
            }
            None => None,
        };
        let updated = self.storage.update_agent(request.id, request.name, agent)?;
        serde_json::to_value(updated).map_err(ToolError::from)
    }

    fn delete_agent(&self, id: &str) -> restflow_tools::Result<Value> {
        if let Some(task_names) =
            crate::services::agent::check_agent_has_active_tasks(&self.background_agent_storage, id)
                .map_err(|e| ToolError::Tool(e.to_string()))?
        {
            return Err(ToolError::Tool(format!(
                "Cannot delete agent {}: active background tasks exist ({})",
                id, task_names
            )));
        }

        self.storage.delete_agent(id.to_string())?;
        Ok(json!({ "id": id, "deleted": true }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::store::AgentStore;
    use std::sync::{Arc, Mutex, OnceLock};
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn setup() -> (
        AgentStoreAdapter,
        tempfile::TempDir,
        std::sync::MutexGuard<'static, ()>,
    ) {
        let guard = env_lock();
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());

        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let prev_dir = std::env::var_os("RESTFLOW_DIR");
        let prev_key = std::env::var_os("RESTFLOW_MASTER_KEY");
        unsafe {
            std::env::set_var("RESTFLOW_DIR", &state_dir);
            std::env::remove_var("RESTFLOW_MASTER_KEY");
        }

        let agent_storage = AgentStorage::new(db.clone()).unwrap();
        let skill_storage = SkillStorage::new(db.clone()).unwrap();
        let secret_storage = SecretStorage::with_config(
            db.clone(),
            restflow_storage::SecretStorageConfig {
                allow_insecure_file_permissions: true,
            },
        )
        .unwrap();
        let bg_storage = BackgroundAgentStorage::new(db).unwrap();
        let known_tools = Arc::new(RwLock::new(HashSet::from([
            "bash".to_string(),
            "http".to_string(),
        ])));

        // Restore env vars immediately
        unsafe {
            match prev_dir {
                Some(v) => std::env::set_var("RESTFLOW_DIR", v),
                None => std::env::remove_var("RESTFLOW_DIR"),
            }
            match prev_key {
                Some(v) => std::env::set_var("RESTFLOW_MASTER_KEY", v),
                None => std::env::remove_var("RESTFLOW_MASTER_KEY"),
            }
        }

        (
            AgentStoreAdapter::new(
                agent_storage,
                skill_storage,
                secret_storage,
                bg_storage,
                known_tools,
            ),
            temp_dir,
            guard,
        )
    }

    #[test]
    fn test_create_and_list_agents() {
        let (adapter, _dir, _guard) = setup();
        let agent_json = serde_json::to_value(crate::models::AgentNode::default()).unwrap();
        let request = AgentCreateRequest {
            name: "Test Agent".to_string(),
            agent: agent_json,
        };
        adapter.create_agent(request).unwrap();

        let list = adapter.list_agents().unwrap();
        let agents = list.as_array().unwrap();
        assert!(!agents.is_empty());
    }

    #[test]
    fn test_get_agent() {
        let (adapter, _dir, _guard) = setup();
        let agent_json = serde_json::to_value(crate::models::AgentNode::default()).unwrap();
        let created = adapter
            .create_agent(AgentCreateRequest {
                name: "Getter".to_string(),
                agent: agent_json,
            })
            .unwrap();
        let id = created["id"].as_str().unwrap();

        let fetched = adapter.get_agent(id).unwrap();
        assert_eq!(fetched["name"], "Getter");
    }

    #[test]
    fn test_get_nonexistent_agent_fails() {
        let (adapter, _dir, _guard) = setup();
        let result = adapter.get_agent("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_agent() {
        let (adapter, _dir, _guard) = setup();
        let agent_json = serde_json::to_value(crate::models::AgentNode::default()).unwrap();
        let created = adapter
            .create_agent(AgentCreateRequest {
                name: "To Delete".to_string(),
                agent: agent_json,
            })
            .unwrap();
        let id = created["id"].as_str().unwrap();

        let result = adapter.delete_agent(id).unwrap();
        assert_eq!(result["deleted"], true);
    }

    #[test]
    fn test_update_agent_name() {
        let (adapter, _dir, _guard) = setup();
        let agent_json = serde_json::to_value(crate::models::AgentNode::default()).unwrap();
        let created = adapter
            .create_agent(AgentCreateRequest {
                name: "Original".to_string(),
                agent: agent_json,
            })
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();

        let updated = adapter
            .update_agent(AgentUpdateRequest {
                id,
                name: Some("Renamed".to_string()),
                agent: None,
            })
            .unwrap();
        assert_eq!(updated["name"], "Renamed");
    }

    #[test]
    fn test_validate_unknown_tool_rejected() {
        let (adapter, _dir, _guard) = setup();
        let mut agent = crate::models::AgentNode::default();
        agent.tools = Some(vec!["nonexistent_tool".to_string()]);
        let agent_json = serde_json::to_value(agent).unwrap();

        let result = adapter.create_agent(AgentCreateRequest {
            name: "Bad Tools".to_string(),
            agent: agent_json,
        });
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("unknown tool"));
    }
}
