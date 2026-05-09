use crate::models::{ChatSession, ChatSessionSource, Task};
use crate::storage::{ChatSessionStorage, Storage, TaskStorage};
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPolicyError {
    NotWorkspaceManaged {
        session_id: String,
        owner: ChatSessionSource,
        operation: &'static str,
    },
    BoundToTask {
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
            Self::BoundToTask { .. } => 409,
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
            Self::BoundToTask {
                session_id,
                task_id,
                task_name,
                operation,
            } => write!(
                f,
                "Session {} is bound to task {} ({}) and cannot be {}",
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
    pub skipped_bound_task: usize,
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
    sessions: ChatSessionStorage,
    tasks: TaskStorage,
}

impl SessionPolicy {
    pub fn new(sessions: ChatSessionStorage, tasks: TaskStorage) -> Self {
        Self { sessions, tasks }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        Self::new(storage.chat_sessions.clone(), storage.tasks.clone())
    }

    fn normalize_session_id(session_id: &str) -> Option<String> {
        let trimmed = session_id.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn task_by_session_map(&self) -> Result<HashMap<String, Task>> {
        let mut map = HashMap::new();
        for task in self.tasks.list_tasks()? {
            if let Some(session_id) = Self::normalize_session_id(&task.chat_session_id) {
                map.insert(session_id, task);
            }
        }
        Ok(map)
    }

    pub fn effective_source(&self, session: &ChatSession) -> Result<EffectiveSessionSource> {
        if self.bound_task(&session.id)?.is_some()
            || session.source_channel == Some(ChatSessionSource::Background)
        {
            return Ok(EffectiveSessionSource {
                source: ChatSessionSource::Background,
                conversation_id: session
                    .source_conversation_id
                    .clone()
                    .or_else(|| Some(session.id.clone())),
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

    pub fn bound_task(&self, session_id: &str) -> Result<Option<Task>> {
        let Some(session_id) = Self::normalize_session_id(session_id) else {
            return Ok(None);
        };
        Ok(self.task_by_session_map()?.remove(session_id.as_str()))
    }

    pub fn ensure_workspace_operation_allowed(
        &self,
        session: &ChatSession,
        operation: &'static str,
    ) -> Result<()> {
        if let Some(task) = self.bound_task(&session.id)? {
            return Err(SessionPolicyError::BoundToTask {
                session_id: session.id.clone(),
                task_id: task.id,
                task_name: task.name,
                operation,
            }
            .into());
        }

        if let Some(owner) = self.management_owner(session)? {
            return Err(SessionPolicyError::NotWorkspaceManaged {
                session_id: session.id.clone(),
                owner,
                operation,
            }
            .into());
        }

        Ok(())
    }

    pub fn archive_workspace_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self.sessions.get(session_id)? else {
            return Ok(false);
        };

        self.ensure_workspace_operation_allowed(&session, "archived")?;
        self.sessions.archive(session_id)
    }

    pub fn delete_workspace_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self.sessions.get(session_id)? else {
            return Ok(false);
        };

        self.ensure_workspace_operation_allowed(&session, "deleted")?;
        self.sessions.delete(session_id)
    }

    pub fn cleanup_workspace_sessions_older_than(
        &self,
        older_than_ms: i64,
    ) -> Result<SessionPolicyCleanupStats> {
        let sessions = self.sessions.list_all()?;
        let task_map = self.task_by_session_map()?;
        let mut stats = SessionPolicyCleanupStats {
            scanned: sessions.len(),
            ..SessionPolicyCleanupStats::default()
        };

        for session in sessions {
            if session.updated_at >= older_than_ms {
                stats.skipped_not_expired += 1;
                continue;
            }

            if task_map.contains_key(&session.id) {
                stats.skipped_bound_task += 1;
                continue;
            }

            if !self.is_workspace_managed(&session)? {
                stats.skipped_non_workspace += 1;
                continue;
            }

            let serialized_len = serde_json::to_vec(&session)
                .map(|bytes| bytes.len() as u64)
                .unwrap_or(0);
            if self.sessions.delete(&session.id)? {
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
        let sessions = self.sessions.list_all()?;
        let task_map = self.task_by_session_map()?;
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

            if task_map.contains_key(&session.id) {
                stats.skipped_bound_task += 1;
                continue;
            }

            if !self.is_workspace_managed(&session)? {
                stats.skipped_non_workspace += 1;
                continue;
            }

            let serialized_len = serde_json::to_vec(&session)
                .map(|bytes| bytes.len() as u64)
                .unwrap_or(0);
            if self.sessions.delete(&session.id)? {
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
    use crate::models::{ChatSessionSource, TaskSpec};
    use crate::storage::{ChatSessionStorage, Storage, TaskStorage};
    use tempfile::tempdir;

    fn create_workspace_session(chat_sessions: &ChatSessionStorage, agent_id: &str) -> ChatSession {
        let mut session = ChatSession::new(agent_id.to_string(), "gpt-5".to_string());
        session.source_channel = Some(ChatSessionSource::Workspace);
        chat_sessions.create(&session).unwrap();
        session
    }

    fn create_task(tasks: &TaskStorage, name: &str, session_id: &str) {
        tasks
            .create_task_from_spec(TaskSpec {
                name: name.to_string(),
                agent_id: "agent-1".to_string(),
                chat_session_id: Some(session_id.to_string()),
                description: None,
                input: Some("run".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();
    }

    #[test]
    fn delete_workspace_session_rejects_background_bound_sessions() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-policy-bound.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let session = create_workspace_session(&storage.chat_sessions, "agent-1");
        create_task(&storage.tasks, "bound-task", &session.id);

        let policy = SessionPolicy::from_storage(&storage);
        let error = policy
            .delete_workspace_session(&session.id)
            .expect_err("bound session should be rejected");
        let error = error.downcast::<SessionPolicyError>().unwrap();
        assert!(matches!(error, SessionPolicyError::BoundToTask { .. }));
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
        create_task(&storage.tasks, "bound-task", &bound_workspace.id);

        let policy = SessionPolicy::from_storage(&storage);
        let stats = policy.cleanup_workspace_sessions_older_than(10).unwrap();

        assert_eq!(stats.deleted, 1);
        assert_eq!(stats.skipped_non_workspace, 0);
        assert_eq!(stats.skipped_bound_task, 1);
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
    }
}
