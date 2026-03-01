use crate::models::{BackgroundAgent, ChatSession, ChatSessionSource};
use crate::storage::{
    BackgroundAgentStorage, ChannelSessionBindingStorage, ChatSessionStorage, Storage,
    ToolTraceStorage,
};
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLifecycleError {
    NotWorkspaceManaged {
        session_id: String,
        owner: ChatSessionSource,
        operation: &'static str,
    },
    BoundToBackgroundTask {
        session_id: String,
        task_id: String,
        task_name: String,
        operation: &'static str,
    },
}

impl SessionLifecycleError {
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::NotWorkspaceManaged { .. } => 403,
            Self::BoundToBackgroundTask { .. } => 409,
        }
    }
}

impl std::fmt::Display for SessionLifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotWorkspaceManaged {
                session_id,
                owner,
                operation,
            } => write!(
                f,
                "Session {} is managed by {:?} and cannot be {} from workspace",
                session_id, owner, operation
            ),
            Self::BoundToBackgroundTask {
                session_id,
                task_id,
                task_name,
                operation,
            } => write!(
                f,
                "Session {} is bound to background task {} ({}) and cannot be {}",
                session_id, task_id, task_name, operation
            ),
        }
    }
}

impl std::error::Error for SessionLifecycleError {}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SessionLifecycleCleanupStats {
    pub scanned: usize,
    pub deleted: usize,
    pub skipped_non_workspace: usize,
    pub skipped_bound_background: usize,
    pub skipped_not_expired: usize,
    pub skipped_no_retention: usize,
    pub failed: usize,
    pub bytes_freed: u64,
}

#[derive(Clone)]
pub struct SessionLifecycleService {
    chat_sessions: ChatSessionStorage,
    channel_session_bindings: ChannelSessionBindingStorage,
    tool_traces: ToolTraceStorage,
    background_agents: BackgroundAgentStorage,
}

impl SessionLifecycleService {
    pub fn new(
        chat_sessions: ChatSessionStorage,
        channel_session_bindings: ChannelSessionBindingStorage,
        tool_traces: ToolTraceStorage,
        background_agents: BackgroundAgentStorage,
    ) -> Self {
        Self {
            chat_sessions,
            channel_session_bindings,
            tool_traces,
            background_agents,
        }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        Self::new(
            storage.chat_sessions.clone(),
            storage.channel_session_bindings.clone(),
            storage.tool_traces.clone(),
            storage.background_agents.clone(),
        )
    }

    pub fn management_owner(&self, session: &ChatSession) -> Result<Option<ChatSessionSource>> {
        let bindings = self.channel_session_bindings.list_by_session(&session.id)?;
        if let Some(binding) = bindings.first() {
            let owner = match binding.channel.trim().to_ascii_lowercase().as_str() {
                "telegram" => ChatSessionSource::Telegram,
                "discord" => ChatSessionSource::Discord,
                "slack" => ChatSessionSource::Slack,
                _ => ChatSessionSource::ExternalLegacy,
            };
            return Ok(Some(owner));
        }

        let owner = match session.source_channel {
            Some(ChatSessionSource::Workspace) | None => None,
            Some(source) => Some(source),
        };
        Ok(owner)
    }

    pub fn is_workspace_managed(&self, session: &ChatSession) -> Result<bool> {
        Ok(self.management_owner(session)?.is_none())
    }

    fn normalize_session_id(session_id: &str) -> Option<String> {
        let trimmed = session_id.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn background_task_by_session_map(&self) -> Result<HashMap<String, BackgroundAgent>> {
        let mut map = HashMap::new();
        for task in self.background_agents.list_tasks()? {
            if let Some(session_id) = Self::normalize_session_id(&task.chat_session_id) {
                map.insert(session_id, task);
            }
        }
        Ok(map)
    }

    pub fn bound_background_task(&self, session_id: &str) -> Result<Option<BackgroundAgent>> {
        let Some(session_id) = Self::normalize_session_id(session_id) else {
            return Ok(None);
        };
        Ok(self
            .background_task_by_session_map()?
            .remove(session_id.as_str()))
    }

    pub fn ensure_workspace_operation_allowed(
        &self,
        session: &ChatSession,
        operation: &'static str,
    ) -> Result<()> {
        if let Some(owner) = self.management_owner(session)? {
            return Err(SessionLifecycleError::NotWorkspaceManaged {
                session_id: session.id.clone(),
                owner,
                operation,
            }
            .into());
        }

        if let Some(task) = self.bound_background_task(&session.id)? {
            return Err(SessionLifecycleError::BoundToBackgroundTask {
                session_id: session.id.clone(),
                task_id: task.id,
                task_name: task.name,
                operation,
            }
            .into());
        }

        Ok(())
    }

    pub fn cleanup_session_artifacts(&self, session_id: &str) -> Result<()> {
        let bindings = self.channel_session_bindings.list_by_session(session_id)?;
        for binding in bindings {
            self.channel_session_bindings.remove_by_route(
                &binding.channel,
                binding.account_id.as_deref(),
                &binding.conversation_id,
            )?;
        }
        self.tool_traces.delete_by_session(session_id)?;
        Ok(())
    }

    pub fn archive_workspace_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self.chat_sessions.get(session_id)? else {
            return Ok(false);
        };

        self.ensure_workspace_operation_allowed(&session, "archived")?;
        self.chat_sessions.archive(session_id)
    }

    pub fn delete_workspace_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self.chat_sessions.get(session_id)? else {
            return Ok(false);
        };

        self.ensure_workspace_operation_allowed(&session, "deleted")?;
        let deleted = self.chat_sessions.delete(session_id)?;
        if deleted {
            self.cleanup_session_artifacts(session_id)?;
        }
        Ok(deleted)
    }

    pub fn cleanup_workspace_sessions_older_than(
        &self,
        older_than_ms: i64,
    ) -> Result<SessionLifecycleCleanupStats> {
        let sessions = self.chat_sessions.list_all()?;
        let task_map = self.background_task_by_session_map()?;
        let mut stats = SessionLifecycleCleanupStats {
            scanned: sessions.len(),
            ..SessionLifecycleCleanupStats::default()
        };

        for session in sessions {
            if session.updated_at >= older_than_ms {
                stats.skipped_not_expired += 1;
                continue;
            }

            if !self.is_workspace_managed(&session)? {
                stats.skipped_non_workspace += 1;
                continue;
            }

            if task_map.contains_key(&session.id) {
                stats.skipped_bound_background += 1;
                continue;
            }

            let serialized_len = serde_json::to_vec(&session)
                .map(|bytes| bytes.len() as u64)
                .unwrap_or(0);
            if self.chat_sessions.delete(&session.id)? {
                self.cleanup_session_artifacts(&session.id)?;
                stats.deleted += 1;
                stats.bytes_freed += serialized_len;
            }
        }

        Ok(stats)
    }

    pub fn cleanup_workspace_sessions_by_retention(
        &self,
        now_ms: i64,
    ) -> Result<SessionLifecycleCleanupStats> {
        let sessions = self.chat_sessions.list_all()?;
        let task_map = self.background_task_by_session_map()?;
        let mut stats = SessionLifecycleCleanupStats {
            scanned: sessions.len(),
            ..SessionLifecycleCleanupStats::default()
        };

        for session in sessions {
            let Some(retention) = session.retention.as_deref() else {
                stats.skipped_no_retention += 1;
                continue;
            };

            let Some(retention_ms) = parse_retention_to_ms(retention) else {
                stats.failed += 1;
                continue;
            };

            let expires_at = session.updated_at.saturating_add(retention_ms);
            if now_ms < expires_at {
                stats.skipped_not_expired += 1;
                continue;
            }

            if !self.is_workspace_managed(&session)? {
                stats.skipped_non_workspace += 1;
                continue;
            }

            if task_map.contains_key(&session.id) {
                stats.skipped_bound_background += 1;
                continue;
            }

            let serialized_len = serde_json::to_vec(&session)
                .map(|bytes| bytes.len() as u64)
                .unwrap_or(0);
            if self.chat_sessions.delete(&session.id)? {
                self.cleanup_session_artifacts(&session.id)?;
                stats.deleted += 1;
                stats.bytes_freed += serialized_len;
            }
        }

        Ok(stats)
    }
}

fn parse_retention_to_ms(retention: &str) -> Option<i64> {
    match retention.trim().to_ascii_lowercase().as_str() {
        "1h" => Some(60 * 60 * 1000),
        "1d" => Some(24 * 60 * 60 * 1000),
        "7d" => Some(7 * 24 * 60 * 60 * 1000),
        "30d" => Some(30 * 24 * 60 * 60 * 1000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        BackgroundAgentSchedule, BackgroundAgentSpec, ChannelSessionBinding, ChatSession,
        ChatSessionSource,
    };
    use crate::storage::{BackgroundAgentStorage, ChatSessionStorage, Storage};
    use tempfile::tempdir;

    fn setup_storage() -> (Storage, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-lifecycle.db");
        (Storage::new(db_path.to_str().unwrap()).unwrap(), dir)
    }

    fn create_workspace_session(chat_sessions: &ChatSessionStorage, agent_id: &str) -> ChatSession {
        let mut session = ChatSession::new(agent_id.to_string(), "gpt-5".to_string());
        session.source_channel = Some(ChatSessionSource::Workspace);
        chat_sessions.create(&session).unwrap();
        session
    }

    fn create_background_task_for_session(
        background_agents: &BackgroundAgentStorage,
        session: &ChatSession,
    ) {
        background_agents
            .create_background_agent(BackgroundAgentSpec {
                name: "Bound Task".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: Some(session.id.clone()),
                description: None,
                input: Some("run".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::default(),
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
    }

    #[test]
    fn cleanup_session_artifacts_removes_bindings_and_traces() {
        let (storage, _dir) = setup_storage();
        let service = SessionLifecycleService::from_storage(&storage);

        let session = create_workspace_session(&storage.chat_sessions, "agent-1");
        storage
            .channel_session_bindings
            .upsert(&ChannelSessionBinding::new(
                "telegram",
                None,
                "chat-cleanup",
                &session.id,
            ))
            .unwrap();
        let trace = crate::models::ToolTrace::turn_started(&session.id, "turn-1");
        storage.tool_traces.append(&trace).unwrap();

        service.cleanup_session_artifacts(&session.id).unwrap();
        assert!(
            storage
                .channel_session_bindings
                .list_by_session(&session.id)
                .unwrap()
                .is_empty()
        );
        assert!(
            storage
                .tool_traces
                .list_by_session(&session.id, None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn delete_workspace_session_succeeds_for_unbound_session() {
        let (storage, _dir) = setup_storage();
        let service = SessionLifecycleService::from_storage(&storage);

        let session = create_workspace_session(&storage.chat_sessions, "agent-1");
        let deleted = service.delete_workspace_session(&session.id).unwrap();
        assert!(deleted);
        assert!(storage.chat_sessions.get(&session.id).unwrap().is_none());
    }

    #[test]
    fn delete_workspace_session_rejects_bound_background_task() {
        let (storage, _dir) = setup_storage();
        let service = SessionLifecycleService::from_storage(&storage);

        let session = create_workspace_session(&storage.chat_sessions, "agent-2");
        create_background_task_for_session(&storage.background_agents, &session);

        let err = service
            .delete_workspace_session(&session.id)
            .expect_err("should reject bound session");
        let lifecycle_err = err
            .downcast_ref::<SessionLifecycleError>()
            .expect("session lifecycle error");
        assert_eq!(lifecycle_err.status_code(), 409);
        assert!(err.to_string().contains("is bound to background task"));
    }

    #[test]
    fn archive_workspace_session_rejects_external_owner() {
        let (storage, _dir) = setup_storage();
        let service = SessionLifecycleService::from_storage(&storage);

        let session = ChatSession::new("agent-3".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Telegram, "chat-33");
        storage.chat_sessions.create(&session).unwrap();

        let err = service
            .archive_workspace_session(&session.id)
            .expect_err("should reject external session");
        let lifecycle_err = err
            .downcast_ref::<SessionLifecycleError>()
            .expect("session lifecycle error");
        assert_eq!(lifecycle_err.status_code(), 403);
    }

    #[test]
    fn cleanup_sessions_by_retention_skips_bound_sessions() {
        let (storage, _dir) = setup_storage();
        let service = SessionLifecycleService::from_storage(&storage);
        let now = chrono::Utc::now().timestamp_millis();

        let mut keep = create_workspace_session(&storage.chat_sessions, "agent-4");
        keep.retention = Some("1h".to_string());
        keep.updated_at = now - 2 * 60 * 60 * 1000;
        storage.chat_sessions.update(&keep).unwrap();
        create_background_task_for_session(&storage.background_agents, &keep);

        let mut purge = create_workspace_session(&storage.chat_sessions, "agent-5");
        purge.retention = Some("1h".to_string());
        purge.updated_at = now - 2 * 60 * 60 * 1000;
        storage.chat_sessions.update(&purge).unwrap();

        let stats = service
            .cleanup_workspace_sessions_by_retention(now)
            .unwrap();
        assert_eq!(stats.deleted, 1);
        assert_eq!(stats.skipped_bound_background, 1);
        assert!(storage.chat_sessions.get(&keep.id).unwrap().is_some());
        assert!(storage.chat_sessions.get(&purge.id).unwrap().is_none());
    }
}
