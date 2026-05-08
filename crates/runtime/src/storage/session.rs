//! Aggregated session-related storage wrappers.
//!
//! This groups the chat session and execution trace stores behind a single
//! typed entrypoint so higher-level services do not have to wire each store
//! independently.

use crate::models::ChatSession;
use crate::storage::{ChatSessionStorage, ExecutionTraceStorage};
use anyhow::Result;

#[derive(Clone)]
pub struct SessionStorage {
    pub chat_sessions: ChatSessionStorage,
    pub execution_traces: ExecutionTraceStorage,
}

impl SessionStorage {
    pub fn new(chat_sessions: ChatSessionStorage, execution_traces: ExecutionTraceStorage) -> Self {
        Self {
            chat_sessions,
            execution_traces,
        }
    }

    pub fn cleanup_artifacts(&self, session_id: &str) -> Result<()> {
        self.delete_traces_by_session(session_id)?;
        Ok(())
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<ChatSession>> {
        self.chat_sessions.get(session_id)
    }

    pub fn create_session(&self, session: &ChatSession) -> Result<()> {
        self.chat_sessions.create(session)
    }

    pub fn update_session(&self, session: &ChatSession) -> Result<()> {
        self.chat_sessions.update(session)
    }

    pub fn save_session(&self, session: &ChatSession) -> Result<()> {
        self.chat_sessions.save(session)
    }

    pub fn list_sessions(&self) -> Result<Vec<ChatSession>> {
        self.chat_sessions.list()
    }

    pub fn list_sessions_all(&self) -> Result<Vec<ChatSession>> {
        self.chat_sessions.list_all()
    }

    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        self.chat_sessions.delete(session_id)
    }

    pub fn archive_session(&self, session_id: &str) -> Result<bool> {
        self.chat_sessions.archive(session_id)
    }

    pub fn unarchive_session(&self, session_id: &str) -> Result<bool> {
        self.chat_sessions.unarchive(session_id)
    }

    pub fn delete_traces_by_session(&self, session_id: &str) -> Result<usize> {
        self.execution_traces.delete_by_session(session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LifecycleTrace;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (SessionStorage, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-storage.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SessionStorage::new(
            ChatSessionStorage::new(db.clone()).unwrap(),
            ExecutionTraceStorage::new(db).unwrap(),
        );
        (storage, dir)
    }

    #[test]
    fn cleanup_artifacts_removes_traces() {
        let (storage, _dir) = setup();
        let session = crate::models::ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        storage.chat_sessions.create(&session).unwrap();
        storage
            .execution_traces
            .store(
                &crate::models::execution_trace_builders::with_trace_context(
                    crate::models::execution_trace_builders::lifecycle(
                        &session.id,
                        "agent-1",
                        LifecycleTrace {
                            status: "running".to_string(),
                            message: None,
                            error: None,
                            ai_duration_ms: None,
                        },
                    ),
                    &ai::telemetry::RestflowTrace::new(
                        "turn-1",
                        &session.id,
                        &session.id,
                        "agent-1",
                    ),
                ),
            )
            .unwrap();

        storage.cleanup_artifacts(&session.id).unwrap();

        assert!(
            storage
                .execution_traces
                .query(&crate::models::ExecutionTraceQuery {
                    session_id: Some(session.id.clone()),
                    limit: Some(10),
                    ..Default::default()
                })
                .unwrap()
                .is_empty()
        );
    }
}
