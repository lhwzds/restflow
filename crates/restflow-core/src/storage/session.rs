//! Aggregated session-related storage wrappers.
//!
//! This groups the chat session, channel binding, and tool trace stores behind
//! a single typed entrypoint so higher-level services do not have to wire each
//! store independently.

use crate::storage::{ChannelSessionBindingStorage, ChatSessionStorage, ToolTraceStorage};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SessionStorage {
    pub chat_sessions: ChatSessionStorage,
    pub channel_session_bindings: ChannelSessionBindingStorage,
    pub tool_traces: ToolTraceStorage,
}

impl SessionStorage {
    pub fn new(
        chat_sessions: ChatSessionStorage,
        channel_session_bindings: ChannelSessionBindingStorage,
        tool_traces: ToolTraceStorage,
    ) -> Self {
        Self {
            chat_sessions,
            channel_session_bindings,
            tool_traces,
        }
    }

    pub fn cleanup_artifacts(&self, session_id: &str) -> Result<()> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChannelSessionBinding, ToolTrace};
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (SessionStorage, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-storage.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SessionStorage::new(
            ChatSessionStorage::new(db.clone()).unwrap(),
            ChannelSessionBindingStorage::new(db.clone()).unwrap(),
            ToolTraceStorage::new(db).unwrap(),
        );
        (storage, dir)
    }

    #[test]
    fn cleanup_artifacts_removes_bindings_and_traces() {
        let (storage, _dir) = setup();
        let session = crate::models::ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        storage.chat_sessions.create(&session).unwrap();
        storage
            .channel_session_bindings
            .upsert(&ChannelSessionBinding::new(
                "telegram",
                None,
                "conversation-1",
                &session.id,
            ))
            .unwrap();
        storage
            .tool_traces
            .append(&ToolTrace::turn_started(&session.id, "turn-1"))
            .unwrap();

        storage.cleanup_artifacts(&session.id).unwrap();

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
}
