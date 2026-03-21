//! SessionStore adapter backed by ChatSessionStorage.

use crate::models::ChatSessionSource;
use crate::services::session::SessionService;
use crate::storage::{AgentStorage, BackgroundAgentStorage, SessionStorage};
use restflow_tools::ToolError;
use restflow_traits::store::{
    SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore,
};
use serde_json::{Value, json};

#[derive(Clone)]
pub struct SessionStorageAdapter {
    sessions: SessionStorage,
    agent_storage: AgentStorage,
    background_agent_storage: BackgroundAgentStorage,
}

impl SessionStorageAdapter {
    pub fn new(
        sessions: SessionStorage,
        agent_storage: AgentStorage,
        background_agent_storage: BackgroundAgentStorage,
    ) -> Self {
        Self {
            sessions,
            agent_storage,
            background_agent_storage,
        }
    }

    fn session_service(&self) -> SessionService {
        SessionService::new(
            self.sessions.clone(),
            Some(self.agent_storage.clone()),
            self.background_agent_storage.clone(),
            None,
        )
    }
}

impl SessionStore for SessionStorageAdapter {
    fn list_sessions(&self, filter: SessionListFilter) -> restflow_tools::Result<Value> {
        let include_archived = filter.include_archived.unwrap_or(false);
        let sessions = match (&filter.agent_id, &filter.skill_id, include_archived) {
            (Some(agent_id), _, true) => self.sessions.chat_sessions.list_by_agent_all(agent_id)?,
            (Some(agent_id), _, false) => self.sessions.chat_sessions.list_by_agent(agent_id)?,
            (None, Some(skill_id), true) => {
                self.sessions.chat_sessions.list_by_skill_all(skill_id)?
            }
            (None, Some(skill_id), false) => self.sessions.chat_sessions.list_by_skill(skill_id)?,
            (None, None, true) => self.sessions.chat_sessions.list_all()?,
            (None, None, false) => self.sessions.chat_sessions.list()?,
        };

        if filter.include_messages.unwrap_or(false) {
            Ok(serde_json::to_value(sessions)?)
        } else {
            let summaries = sessions
                .iter()
                .map(crate::models::ChatSessionSummary::from)
                .collect::<Vec<_>>();
            Ok(serde_json::to_value(summaries)?)
        }
    }

    fn get_session(&self, id: &str) -> restflow_tools::Result<Value> {
        let session = self
            .sessions
            .chat_sessions
            .get(id)?
            .ok_or_else(|| ToolError::Tool(format!("Session {} not found", id)))?;
        Ok(serde_json::to_value(session)?)
    }

    fn create_session(&self, request: SessionCreateRequest) -> restflow_tools::Result<Value> {
        let resolved_agent_id = self
            .agent_storage
            .resolve_existing_agent_id(&request.agent_id)?;
        let mut session = crate::models::ChatSession::new(resolved_agent_id, request.model);
        session.source_channel = Some(ChatSessionSource::Workspace);
        if let Some(name) = request.name {
            session = session.with_name(name);
        }
        if let Some(skill_id) = request.skill_id {
            session = session.with_skill(skill_id);
        }
        if let Some(retention) = request.retention {
            session = session.with_retention(retention);
        }
        self.sessions.chat_sessions.create(&session)?;
        Ok(serde_json::to_value(session)?)
    }

    fn archive_session(&self, id: &str) -> restflow_tools::Result<Value> {
        let archived = self.session_service().archive_workspace_session(id)?;
        Ok(json!({ "id": id, "archived": archived }))
    }

    fn unarchive_session(&self, id: &str) -> restflow_tools::Result<Value> {
        let unarchived = self.sessions.chat_sessions.unarchive(id)?;
        Ok(json!({ "id": id, "unarchived": unarchived }))
    }

    fn purge_session(&self, id: &str) -> restflow_tools::Result<Value> {
        let purged = self.session_service().delete_workspace_session(id)?;
        Ok(json!({ "id": id, "purged": purged }))
    }

    fn delete_session(&self, id: &str) -> restflow_tools::Result<Value> {
        self.purge_session(id)
    }

    fn search_sessions(&self, query: SessionSearchQuery) -> restflow_tools::Result<Value> {
        let include_archived = query.include_archived.unwrap_or(false);
        let mut sessions = match (&query.agent_id, include_archived) {
            (Some(agent_id), true) => self.sessions.chat_sessions.list_by_agent_all(agent_id)?,
            (Some(agent_id), false) => self.sessions.chat_sessions.list_by_agent(agent_id)?,
            (None, true) => self.sessions.chat_sessions.list_all()?,
            (None, false) => self.sessions.chat_sessions.list()?,
        };
        if let Some(skill_id) = &query.skill_id {
            sessions.retain(|session| session.skill_id.as_deref() == Some(skill_id.as_str()));
        }

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

        Ok(serde_json::to_value(matched)?)
    }

    fn cleanup_sessions(&self) -> restflow_tools::Result<Value> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let stats = self
            .session_service()
            .cleanup_workspace_sessions_by_retention(now_ms)?;
        Ok(serde_json::to_value(stats)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::store::SessionStore;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (SessionStorageAdapter, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let session_storage = SessionStorage::new(
            crate::storage::ChatSessionStorage::new(db.clone()).unwrap(),
            crate::storage::ChannelSessionBindingStorage::new(db.clone()).unwrap(),
            crate::storage::ExecutionTraceStorage::new(db.clone()).unwrap(),
        );
        let agent_storage = AgentStorage::new(db.clone()).unwrap();
        let background_agent_storage = BackgroundAgentStorage::new(db.clone()).unwrap();
        (
            SessionStorageAdapter::new(session_storage, agent_storage, background_agent_storage),
            temp_dir,
        )
    }

    fn create_default_agent(adapter: &SessionStorageAdapter) -> String {
        let agent = crate::models::AgentNode::default();
        let created = adapter
            .agent_storage
            .create_agent("test-agent".to_string(), agent)
            .unwrap();
        created.id
    }

    #[test]
    fn test_list_sessions_empty() {
        let (adapter, _dir) = setup();
        let filter = SessionListFilter {
            agent_id: None,
            skill_id: None,
            include_messages: None,
            include_archived: None,
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
        assert_eq!(result["purged"], true);
    }

    #[test]
    fn test_delete_session_rejects_background_bound_session() {
        let (adapter, _dir) = setup();
        let agent_id = create_default_agent(&adapter);
        let created = adapter
            .create_session(SessionCreateRequest {
                agent_id: agent_id.clone(),
                model: "gpt-4".to_string(),
                name: Some("Bound Session".to_string()),
                skill_id: None,
                retention: None,
            })
            .unwrap();
        let session_id = created["id"].as_str().unwrap().to_string();

        adapter
            .background_agent_storage
            .create_background_agent(crate::models::BackgroundAgentSpec {
                name: "Session Owner".to_string(),
                agent_id,
                chat_session_id: Some(session_id.clone()),
                description: None,
                input: Some("run".to_string()),
                input_template: None,
                schedule: crate::models::BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let err = adapter
            .delete_session(&session_id)
            .expect_err("bound session must not be deleted");
        assert!(err.to_string().contains("bound to background task"));
    }

    #[test]
    fn test_archive_and_unarchive_session() {
        let (adapter, _dir) = setup();
        let agent_id = create_default_agent(&adapter);
        let created = adapter
            .create_session(SessionCreateRequest {
                agent_id,
                model: "gpt-4".to_string(),
                name: Some("Archive Target".to_string()),
                skill_id: None,
                retention: None,
            })
            .unwrap();
        let session_id = created["id"].as_str().unwrap().to_string();

        let archive = adapter.archive_session(&session_id).unwrap();
        assert_eq!(archive["archived"], true);

        let active_list = adapter
            .list_sessions(SessionListFilter {
                agent_id: None,
                skill_id: None,
                include_messages: None,
                include_archived: Some(false),
            })
            .unwrap();
        assert_eq!(active_list.as_array().unwrap().len(), 0);

        let all_list = adapter
            .list_sessions(SessionListFilter {
                agent_id: None,
                skill_id: None,
                include_messages: None,
                include_archived: Some(true),
            })
            .unwrap();
        assert_eq!(all_list.as_array().unwrap().len(), 1);

        let unarchive = adapter.unarchive_session(&session_id).unwrap();
        assert_eq!(unarchive["unarchived"], true);
        let active_again = adapter
            .list_sessions(SessionListFilter {
                agent_id: None,
                skill_id: None,
                include_messages: None,
                include_archived: Some(false),
            })
            .unwrap();
        assert_eq!(active_again.as_array().unwrap().len(), 1);
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
            include_archived: None,
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
