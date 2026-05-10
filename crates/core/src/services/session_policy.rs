use crate::session_log::{FileSession, FileSessionStore};
use crate::storage::Storage;
use anyhow::Result;
use types::{ChatSession, ChatSessionSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPolicyError {
    NotWorkspaceManaged {
        session_id: String,
        owner: ChatSessionSource,
        operation: &'static str,
    },
}

impl SessionPolicyError {
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::NotWorkspaceManaged { .. } => 403,
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
        }
    }
}

impl std::error::Error for SessionPolicyError {}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SessionPolicyCleanupStats {
    pub scanned: usize,
    pub deleted: usize,
    pub skipped_non_workspace: usize,
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
    sessions: FileSessionStore,
}

impl SessionPolicy {
    pub fn new(sessions: FileSessionStore) -> Self {
        Self { sessions }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        Self::new(storage.file_sessions.clone())
    }

    pub fn effective_source(&self, session: &ChatSession) -> Result<EffectiveSessionSource> {
        if session.source_channel == Some(ChatSessionSource::Background) {
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

        Ok(())
    }

    pub fn archive_workspace_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self
            .sessions
            .get(session_id)?
            .map(|session| session.to_chat_session())
        else {
            return Ok(false);
        };

        self.ensure_workspace_operation_allowed(&session, "archived")?;
        if session.is_archived() {
            return Ok(false);
        }
        let mut session = session;
        session.archive();
        self.write_session(&session)?;
        Ok(true)
    }

    pub fn delete_workspace_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self
            .sessions
            .get(session_id)?
            .map(|session| session.to_chat_session())
        else {
            return Ok(false);
        };

        self.ensure_workspace_operation_allowed(&session, "deleted")?;
        self.sessions.delete(session_id)
    }

    pub fn cleanup_workspace_sessions_older_than(
        &self,
        older_than_ms: i64,
    ) -> Result<SessionPolicyCleanupStats> {
        let sessions = self.list_sessions()?;
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
        let sessions = self.list_sessions()?;
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

    fn list_sessions(&self) -> Result<Vec<ChatSession>> {
        self.sessions
            .list()?
            .into_iter()
            .map(|session| Ok(session.to_chat_session()))
            .collect()
    }

    fn write_session(&self, session: &ChatSession) -> Result<()> {
        let existing = self.sessions.get(&session.id)?;
        let file_session = FileSession::merge_chat_session(existing.as_ref(), session);
        self.sessions.write_session(&file_session, true)?;
        Ok(())
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
    use crate::session_log::FileSessionStore;
    use crate::storage::Storage;
    use tempfile::tempdir;
    use types::ChatSessionSource;

    fn create_workspace_session(file_sessions: &FileSessionStore, agent_id: &str) -> ChatSession {
        let mut session = ChatSession::new(agent_id.to_string(), "gpt-5".to_string());
        session.source_channel = Some(ChatSessionSource::Workspace);
        let file_session = FileSession::from_chat_session(&session);
        file_sessions.write_session(&file_session, true).unwrap();
        session
    }

    #[test]
    fn cleanup_workspace_sessions_only_deletes_eligible_sessions() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-policy-cleanup.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();

        let mut old_workspace = create_workspace_session(&storage.file_sessions, "agent-1");
        old_workspace.updated_at = 1;
        storage
            .file_sessions
            .write_session(&FileSession::from_chat_session(&old_workspace), true)
            .unwrap();

        let policy = SessionPolicy::from_storage(&storage);
        let stats = policy.cleanup_workspace_sessions_older_than(10).unwrap();

        assert_eq!(stats.deleted, 1);
        assert_eq!(stats.skipped_non_workspace, 0);
        assert!(
            storage
                .file_sessions
                .get(&old_workspace.id)
                .unwrap()
                .is_none()
        );
    }
}
