//! AgentStore adapter backed by AgentStorage.

use crate::storage::{AgentStorage, BackgroundAgentStorage, SecretStorage};
use crate::storage::skill::SkillStorage;
use restflow_ai::tools::{AgentCreateRequest, AgentStore, AgentUpdateRequest};
use restflow_tools::ToolError;
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
            for skill_id in skills {
                let normalized = skill_id.trim();
                if normalized.is_empty() {
                    errors.push(crate::models::ValidationError::new(
                        "skills",
                        "skill ID must not be empty",
                    ));
                    continue;
                }
                match self.skills.exists(normalized) {
                    Ok(true) => {}
                    Ok(false) => errors.push(crate::models::ValidationError::new(
                        "skills",
                        format!("unknown skill: {}", normalized),
                    )),
                    Err(err) => errors.push(crate::models::ValidationError::new(
                        "skills",
                        format!("failed to verify skill '{}': {}", normalized, err),
                    )),
                }
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
        let agents = self
            .storage
            .list_agents()
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(agents).map_err(ToolError::from)
    }

    fn get_agent(&self, id: &str) -> restflow_tools::Result<Value> {
        let agent = self
            .storage
            .get_agent(id.to_string())
            .map_err(|e| ToolError::Tool(e.to_string()))?
            .ok_or_else(|| ToolError::Tool(format!("Agent {} not found", id)))?;
        serde_json::to_value(agent).map_err(ToolError::from)
    }

    fn create_agent(
        &self,
        request: AgentCreateRequest,
    ) -> restflow_tools::Result<Value> {
        let agent = Self::parse_agent_node(request.agent)?;
        self.validate_agent_node(&agent)?;
        let created = self
            .storage
            .create_agent(request.name, agent)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(created).map_err(ToolError::from)
    }

    fn update_agent(
        &self,
        request: AgentUpdateRequest,
    ) -> restflow_tools::Result<Value> {
        let agent = match request.agent {
            Some(value) => {
                let node = Self::parse_agent_node(value)?;
                self.validate_agent_node(&node)?;
                Some(node)
            }
            None => None,
        };
        let updated = self
            .storage
            .update_agent(request.id, request.name, agent)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(updated).map_err(ToolError::from)
    }

    fn delete_agent(&self, id: &str) -> restflow_tools::Result<Value> {
        let active_tasks = self
            .background_agent_storage
            .list_active_tasks_by_agent_id(id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        if !active_tasks.is_empty() {
            let task_names = active_tasks
                .iter()
                .map(|task| task.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ToolError::Tool(format!(
                "Cannot delete agent {}: active background tasks exist ({})",
                id, task_names
            )));
        }

        self.storage
            .delete_agent(id.to_string())
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": true }))
    }
}
