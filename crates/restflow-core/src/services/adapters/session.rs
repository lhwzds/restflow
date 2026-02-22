//! SessionStore adapter backed by ChatSessionStorage.

use crate::storage::{AgentStorage, ChatSessionStorage};
use restflow_ai::tools::{SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore};
use restflow_tools::ToolError;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct SessionStorageAdapter {
    storage: ChatSessionStorage,
    agent_storage: AgentStorage,
}

impl SessionStorageAdapter {
    pub fn new(storage: ChatSessionStorage, agent_storage: AgentStorage) -> Self {
        Self {
            storage,
            agent_storage,
        }
    }
}

impl SessionStore for SessionStorageAdapter {
    fn list_sessions(&self, filter: SessionListFilter) -> restflow_tools::Result<Value> {
        let sessions = if let Some(agent_id) = &filter.agent_id {
            self.storage
                .list_by_agent(agent_id)
                .map_err(|e| ToolError::Tool(e.to_string()))?
        } else if let Some(skill_id) = &filter.skill_id {
            self.storage
                .list_by_skill(skill_id)
                .map_err(|e| ToolError::Tool(e.to_string()))?
        } else {
            self.storage
                .list()
                .map_err(|e| ToolError::Tool(e.to_string()))?
        };

        if filter.include_messages.unwrap_or(false) {
            serde_json::to_value(sessions).map_err(ToolError::from)
        } else {
            let summaries = self
                .storage
                .list_summaries()
                .map_err(|e| ToolError::Tool(e.to_string()))?;
            serde_json::to_value(summaries).map_err(ToolError::from)
        }
    }

    fn get_session(&self, id: &str) -> restflow_tools::Result<Value> {
        let session = self
            .storage
            .get(id)
            .map_err(|e| ToolError::Tool(e.to_string()))?
            .ok_or_else(|| ToolError::Tool(format!("Session {} not found", id)))?;
        serde_json::to_value(session).map_err(ToolError::from)
    }

    fn create_session(&self, request: SessionCreateRequest) -> restflow_tools::Result<Value> {
        let resolved_agent_id = self
            .agent_storage
            .resolve_existing_agent_id(&request.agent_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        let mut session = crate::models::ChatSession::new(resolved_agent_id, request.model);
        if let Some(name) = request.name {
            session = session.with_name(name);
        }
        if let Some(skill_id) = request.skill_id {
            session = session.with_skill(skill_id);
        }
        if let Some(retention) = request.retention {
            session = session.with_retention(retention);
        }
        self.storage
            .create(&session)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(session).map_err(ToolError::from)
    }

    fn delete_session(&self, id: &str) -> restflow_tools::Result<Value> {
        let deleted = self
            .storage
            .delete(id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": deleted }))
    }

    fn search_sessions(&self, query: SessionSearchQuery) -> restflow_tools::Result<Value> {
        let sessions = if let Some(agent_id) = &query.agent_id {
            self.storage
                .list_by_agent(agent_id)
                .map_err(|e| ToolError::Tool(e.to_string()))?
        } else {
            self.storage
                .list()
                .map_err(|e| ToolError::Tool(e.to_string()))?
        };

        let keyword = query.query.to_lowercase();
        let limit = query.limit.unwrap_or(20) as usize;

        let matched: Vec<_> = sessions
            .into_iter()
            .filter(|s| {
                let name_match = s.name.to_lowercase().contains(&keyword);
                let msg_match = s
                    .messages
                    .iter()
                    .any(|m| m.content.to_lowercase().contains(&keyword));
                name_match || msg_match
            })
            .take(limit)
            .collect();

        serde_json::to_value(matched).map_err(ToolError::from)
    }

    fn cleanup_sessions(&self) -> restflow_tools::Result<Value> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let stats = self
            .storage
            .cleanup_by_session_retention(now_ms)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(stats).map_err(ToolError::from)
    }
}
