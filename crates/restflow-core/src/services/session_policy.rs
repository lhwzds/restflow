use crate::models::{BackgroundAgent, ChatSession, ChatSessionSource};
use crate::storage::{BackgroundAgentStorage, SessionStorage, Storage};
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPolicyError {
    NotWorkspaceManaged {
        session_id: String,
        owner: ChatSessionSource,
        operation: &'static str,
    },
    NotExternallyManaged {
        session_id: String,
        operation: &'static str,
    },
    MissingExternalRoute {
        session_id: String,
        operation: &'static str,
    },
    BoundToBackgroundTask {
        session_id: String,
        task_id: String,
        task_name: String,
        operation: &'static str,
    },
}

impl SessionPolicyError {
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::NotWorkspaceManaged { .. } => 403,
            Self::NotExternallyManaged { .. } | Self::MissingExternalRoute { .. } => 400,
            Self::BoundToBackgroundTask { .. } => 409,
        }
    }
}

impl std::fmt::Display for SessionPolicyError {
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
            Self::NotExternallyManaged {
                session_id,
                operation,
            } => write!(
                f,
                "Session {} is not externally managed and cannot be {}",
                session_id, operation
            ),
            Self::MissingExternalRoute {
                session_id,
                operation,
            } => write!(
                f,
                "Session {} is missing an external route and cannot be {}",
                session_id, operation
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

impl std::error::Error for SessionPolicyError {}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SessionPolicyCleanupStats {
    pub scanned: usize,
    pub deleted: usize,
    pub skipped_non_workspace: usize,
    pub skipped_bound_background: usize,
    pub skipped_not_expired: usize,
    pub skipped_no_retention: usize,
    pub failed: usize,
    pub bytes_freed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveSessionSource {
    pub source: ChatSessionSource,
    pub conversation_id: Option<String>,
}

#[derive(Clone)]
pub struct SessionPolicy {
    sessions: SessionStorage,
    background_agents: BackgroundAgentStorage,
}

impl SessionPolicy {
    pub fn new(sessions: SessionStorage, background_agents: BackgroundAgentStorage) -> Self {
        Self {
            sessions,
            background_agents,
        }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        Self::new(storage.sessions.clone(), storage.background_agents.clone())
    }

    fn parse_binding_channel_source(channel: &str) -> Option<ChatSessionSource> {
        match channel.trim().to_ascii_lowercase().as_str() {
            "telegram" => Some(ChatSessionSource::Telegram),
            "discord" => Some(ChatSessionSource::Discord),
            "slack" => Some(ChatSessionSource::Slack),
            _ => None,
        }
    }

    fn normalize_session_id(session_id: &str) -> Option<String> {
        let trimmed = session_id.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn resolve_legacy_external_route(session: &ChatSession) -> Option<(ChatSessionSource, String)> {
        let source = match session.source_channel {
            Some(ChatSessionSource::Workspace) | None => return None,
            Some(source) => source,
        };
        let conversation_id = session
            .source_conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())?
            .to_string();
        Some((source, conversation_id))
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

    pub fn effective_source(&self, session: &ChatSession) -> Result<EffectiveSessionSource> {
        let bindings = self.sessions.list_bindings_by_session(&session.id)?;
        if let Some(binding) = bindings.first() {
            let source = Self::parse_binding_channel_source(&binding.channel)
                .unwrap_or(ChatSessionSource::ExternalLegacy);
            return Ok(EffectiveSessionSource {
                source,
                conversation_id: Some(binding.conversation_id.clone()),
            });
        }

        if let Some((source, conversation_id)) = Self::resolve_legacy_external_route(session) {
            return Ok(EffectiveSessionSource {
                source,
                conversation_id: Some(conversation_id),
            });
        }

        Ok(EffectiveSessionSource {
            source: ChatSessionSource::Workspace,
            conversation_id: None,
        })
    }

    pub fn management_owner(&self, session: &ChatSession) -> Result<Option<ChatSessionSource>> {
        let effective = self.effective_source(session)?;
        Ok(match effective.source {
            ChatSessionSource::Workspace => None,
            source => Some(source),
        })
    }

    pub fn is_workspace_managed(&self, session: &ChatSession) -> Result<bool> {
        Ok(self.management_owner(session)?.is_none())
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
            return Err(SessionPolicyError::NotWorkspaceManaged {
                session_id: session.id.clone(),
                owner,
                operation,
            }
            .into());
        }

        if let Some(task) = self.bound_background_task(&session.id)? {
            return Err(SessionPolicyError::BoundToBackgroundTask {
                session_id: session.id.clone(),
                task_id: task.id,
                task_name: task.name,
                operation,
            }
            .into());
        }

        Ok(())
    }

    pub fn ensure_external_rebuild_allowed(&self, session: &ChatSession) -> Result<()> {
        let effective = self.effective_source(session)?;
        if effective.source == ChatSessionSource::Workspace {
            return Err(SessionPolicyError::NotExternallyManaged {
                session_id: session.id.clone(),
                operation: "rebuilt",
            }
            .into());
        }
        if effective
            .conversation_id
            .as_deref()
            .map(str::trim)
            .is_none_or(|value| value.is_empty())
        {
            return Err(SessionPolicyError::MissingExternalRoute {
                session_id: session.id.clone(),
                operation: "rebuilt",
            }
            .into());
        }
        if let Some(task) = self.bound_background_task(&session.id)? {
            return Err(SessionPolicyError::BoundToBackgroundTask {
                session_id: session.id.clone(),
                task_id: task.id,
                task_name: task.name,
                operation: "rebuilt",
            }
            .into());
        }
        Ok(())
    }

    pub fn cleanup_session_artifacts(&self, session_id: &str) -> Result<()> {
        self.sessions.cleanup_artifacts(session_id)
    }

    pub fn archive_workspace_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self.sessions.get_session(session_id)? else {
            return Ok(false);
        };

        self.ensure_workspace_operation_allowed(&session, "archived")?;
        self.sessions.archive_session(session_id)
    }

    pub fn delete_workspace_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self.sessions.get_session(session_id)? else {
            return Ok(false);
        };

        self.ensure_workspace_operation_allowed(&session, "deleted")?;
        let deleted = self.sessions.delete_session(session_id)?;
        if deleted {
            self.cleanup_session_artifacts(session_id)?;
        }
        Ok(deleted)
    }

    pub fn cleanup_workspace_sessions_older_than(
        &self,
        older_than_ms: i64,
    ) -> Result<SessionPolicyCleanupStats> {
        let sessions = self.sessions.list_sessions_all()?;
        let task_map = self.background_task_by_session_map()?;
        let mut stats = SessionPolicyCleanupStats {
            scanned: sessions.len(),
            ..SessionPolicyCleanupStats::default()
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
            if self.sessions.delete_session(&session.id)? {
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
    ) -> Result<SessionPolicyCleanupStats> {
        let sessions = self.sessions.list_sessions_all()?;
        let task_map = self.background_task_by_session_map()?;
        let mut stats = SessionPolicyCleanupStats {
            scanned: sessions.len(),
            ..SessionPolicyCleanupStats::default()
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
            if self.sessions.delete_session(&session.id)? {
                self.cleanup_session_artifacts(&session.id)?;
                stats.deleted += 1;
                stats.bytes_freed += serialized_len;
            }
        }

        Ok(stats)
    }
}

fn parse_retention_to_ms(retention: &str) -> Option<i64> {
    let normalized = retention.trim().to_ascii_lowercase();
    match normalized.as_str() {
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
    use crate::models::{BackgroundAgentSpec, ChannelSessionBinding, ChatSessionSource};
    use crate::storage::{BackgroundAgentStorage, ChatSessionStorage, Storage};
    use tempfile::tempdir;

    fn create_workspace_session(chat_sessions: &ChatSessionStorage, agent_id: &str) -> ChatSession {
        let mut session = ChatSession::new(agent_id.to_string(), "gpt-5".to_string());
        session.source_channel = Some(ChatSessionSource::Workspace);
        chat_sessions.create(&session).unwrap();
        session
    }

    fn create_background_task(
        background_agents: &BackgroundAgentStorage,
        name: &str,
        session_id: &str,
    ) {
        background_agents
            .create_background_agent(BackgroundAgentSpec {
                name: name.to_string(),
                agent_id: "agent-1".to_string(),
                chat_session_id: Some(session_id.to_string()),
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
    }

    #[test]
    fn management_owner_prefers_binding_over_session_source() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-policy-owner.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Telegram, "legacy-chat");
        storage.chat_sessions.create(&session).unwrap();
        storage
            .channel_session_bindings
            .upsert(&ChannelSessionBinding::new(
                "discord",
                None,
                "binding-chat",
                &session.id,
            ))
            .unwrap();

        let policy = SessionPolicy::from_storage(&storage);
        assert_eq!(
            policy.management_owner(&session).unwrap(),
            Some(ChatSessionSource::Discord)
        );

        policy
            .effective_source(&session)
            .map(|effective| {
                assert_eq!(effective.source, ChatSessionSource::Discord);
                assert_eq!(effective.conversation_id.as_deref(), Some("binding-chat"));
            })
            .unwrap();

        session.source_channel = Some(ChatSessionSource::Workspace);
        session.source_conversation_id = None;
        assert_eq!(
            policy.management_owner(&session).unwrap(),
            Some(ChatSessionSource::Discord)
        );
    }

    #[test]
    fn ensure_external_rebuild_allowed_rejects_workspace_sessions() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-policy-rebuild.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let session = create_workspace_session(&storage.chat_sessions, "agent-1");
        let policy = SessionPolicy::from_storage(&storage);

        let error = policy
            .ensure_external_rebuild_allowed(&session)
            .expect_err("workspace session should not rebuild");
        let error = error.downcast::<SessionPolicyError>().unwrap();
        assert!(matches!(
            error,
            SessionPolicyError::NotExternallyManaged { .. }
        ));
    }

    #[test]
    fn delete_workspace_session_rejects_background_bound_sessions() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-policy-bound.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let session = create_workspace_session(&storage.chat_sessions, "agent-1");
        create_background_task(&storage.background_agents, "bound-task", &session.id);

        let policy = SessionPolicy::from_storage(&storage);
        let error = policy
            .delete_workspace_session(&session.id)
            .expect_err("bound session should be rejected");
        let error = error.downcast::<SessionPolicyError>().unwrap();
        assert!(matches!(
            error,
            SessionPolicyError::BoundToBackgroundTask { .. }
        ));
    }

    #[test]
    fn cleanup_workspace_sessions_only_deletes_eligible_sessions() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-policy-cleanup.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();

        let mut old_workspace = create_workspace_session(&storage.chat_sessions, "agent-1");
        old_workspace.updated_at = 1;
        storage.chat_sessions.update(&old_workspace).unwrap();

        let mut bound_workspace = create_workspace_session(&storage.chat_sessions, "agent-1");
        bound_workspace.updated_at = 1;
        storage.chat_sessions.update(&bound_workspace).unwrap();
        create_background_task(
            &storage.background_agents,
            "bound-task",
            &bound_workspace.id,
        );

        let mut external = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Telegram, "chat-123");
        external.updated_at = 1;
        storage.chat_sessions.create(&external).unwrap();

        let policy = SessionPolicy::from_storage(&storage);
        let stats = policy.cleanup_workspace_sessions_older_than(10).unwrap();

        assert_eq!(stats.deleted, 1);
        assert_eq!(stats.skipped_non_workspace, 1);
        assert_eq!(stats.skipped_bound_background, 1);
        assert!(
            storage
                .chat_sessions
                .get(&old_workspace.id)
                .unwrap()
                .is_none()
        );
        assert!(
            storage
                .chat_sessions
                .get(&bound_workspace.id)
                .unwrap()
                .is_some()
        );
        assert!(storage.chat_sessions.get(&external.id).unwrap().is_some());
    }
}
