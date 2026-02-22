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

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::tools::SessionStore;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (SessionStorageAdapter, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let chat_storage = ChatSessionStorage::new(db.clone()).unwrap();
        let agent_storage = AgentStorage::new(db).unwrap();
        (SessionStorageAdapter::new(chat_storage, agent_storage), temp_dir)
    }

    fn create_default_agent(adapter: &SessionStorageAdapter) -> String {
        let agent = crate::models::AgentNode::default();
        let created = adapter.agent_storage.create_agent("test-agent".to_string(), agent).unwrap();
        created.id
    }

    #[test]
    fn test_list_sessions_empty() {
        let (adapter, _dir) = setup();
        let filter = SessionListFilter {
            agent_id: None,
            skill_id: None,
            include_messages: None,
        };
        let result = adapter.list_sessions(filter).unwrap();
        let sessions = result.as_array().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_create_and_get_session() {
        let (adapter, _dir) = setup();
        let agent_id = create_default_agent(&adapter);
        let request = SessionCreateRequest {
            agent_id: agent_id.clone(),
            model: "gpt-4".to_string(),
            name: Some("Test Session".to_string()),
            skill_id: None,
            retention: None,
        };
        let created = adapter.create_session(request).unwrap();
        let session_id = created["id"].as_str().unwrap();

        let fetched = adapter.get_session(session_id).unwrap();
        assert_eq!(fetched["name"], "Test Session");
        assert_eq!(fetched["model"], "gpt-4");
    }

    #[test]
    fn test_delete_session() {
        let (adapter, _dir) = setup();
        let agent_id = create_default_agent(&adapter);
        let request = SessionCreateRequest {
            agent_id,
            model: "gpt-4".to_string(),
            name: None,
            skill_id: None,
            retention: None,
        };
        let created = adapter.create_session(request).unwrap();
        let session_id = created["id"].as_str().unwrap().to_string();

        let result = adapter.delete_session(&session_id).unwrap();
        assert_eq!(result["deleted"], true);
    }

    #[test]
    fn test_get_nonexistent_session_fails() {
        let (adapter, _dir) = setup();
        let result = adapter.get_session("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_search_sessions() {
        let (adapter, _dir) = setup();
        let agent_id = create_default_agent(&adapter);
        let request = SessionCreateRequest {
            agent_id: agent_id.clone(),
            model: "gpt-4".to_string(),
            name: Some("Meeting Notes".to_string()),
            skill_id: None,
            retention: None,
        };
        adapter.create_session(request).unwrap();

        let query = SessionSearchQuery {
            query: "meeting".to_string(),
            agent_id: None,
            skill_id: None,
            limit: None,
        };
        let result = adapter.search_sessions(query).unwrap();
        let sessions = result.as_array().unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn test_cleanup_sessions() {
        let (adapter, _dir) = setup();
        let result = adapter.cleanup_sessions().unwrap();
        assert!(result.is_object());
    }
}
