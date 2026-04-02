//! Aggregated session-related storage wrappers.
//!
//! This groups the chat session, channel binding, and execution trace stores behind
//! a single typed entrypoint so higher-level services do not have to wire each
//! store independently.

use crate::models::{ChannelSessionBinding, ChatSession, ChatSessionSource};
use crate::storage::{ChannelSessionBindingStorage, ChatSessionStorage, ExecutionTraceStorage};
use anyhow::Result;

#[derive(Clone)]
pub struct SessionStorage {
    pub chat_sessions: ChatSessionStorage,
    pub channel_session_bindings: ChannelSessionBindingStorage,
    pub execution_traces: ExecutionTraceStorage,
}

impl SessionStorage {
    pub fn new(
        chat_sessions: ChatSessionStorage,
        channel_session_bindings: ChannelSessionBindingStorage,
        execution_traces: ExecutionTraceStorage,
    ) -> Self {
        Self {
            chat_sessions,
            channel_session_bindings,
            execution_traces,
        }
    }

    pub fn cleanup_artifacts(&self, session_id: &str) -> Result<()> {
        self.remove_bindings_by_session(session_id)?;
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

    pub fn list_bindings_by_session(&self, session_id: &str) -> Result<Vec<ChannelSessionBinding>> {
        self.channel_session_bindings.list_by_session(session_id)
    }

    pub fn upsert_binding(&self, binding: &ChannelSessionBinding) -> Result<()> {
        self.channel_session_bindings.upsert(binding)
    }

    pub fn get_binding_by_route(
        &self,
        channel: &str,
        account_id: Option<&str>,
        conversation_id: &str,
    ) -> Result<Option<ChannelSessionBinding>> {
        self.channel_session_bindings
            .get_by_route(channel, account_id, conversation_id)
    }

    pub fn remove_binding_by_route(
        &self,
        channel: &str,
        account_id: Option<&str>,
        conversation_id: &str,
    ) -> Result<bool> {
        self.channel_session_bindings
            .remove_by_route(channel, account_id, conversation_id)
    }

    pub fn remove_bindings_by_session(&self, session_id: &str) -> Result<usize> {
        let bindings = self.list_bindings_by_session(session_id)?;
        let count = bindings.len();
        for binding in bindings {
            self.remove_binding_by_route(
                &binding.channel,
                binding.account_id.as_deref(),
                &binding.conversation_id,
            )?;
        }
        Ok(count)
    }

    pub fn delete_traces_by_session(&self, session_id: &str) -> Result<usize> {
        self.execution_traces.delete_by_session(session_id)
    }

    pub fn ensure_binding_from_legacy_source(
        &self,
        session: &ChatSession,
    ) -> Result<Option<(ChatSessionSource, String)>> {
        let source = match session.source_channel {
            Some(ChatSessionSource::Workspace) | None => return Ok(None),
            Some(source) => source,
        };
        let Some(conversation_id) = session
            .source_conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
        else {
            return Ok(None);
        };

        let Some(channel_key) = channel_key_from_source(source) else {
            return Ok(Some((source, conversation_id)));
        };

        let binding = ChannelSessionBinding::new(channel_key, None, &conversation_id, &session.id);
        self.upsert_binding(&binding)?;
        Ok(Some((source, conversation_id)))
    }

    pub fn switch_bindings(
        &self,
        from_session_id: &str,
        to_session_id: &str,
    ) -> Result<Vec<ChannelSessionBinding>> {
        let bindings = self.list_bindings_by_session(from_session_id)?;
        for binding in &bindings {
            let rebound = ChannelSessionBinding::new(
                binding.channel.clone(),
                binding.account_id.clone(),
                binding.conversation_id.clone(),
                to_session_id,
            );
            self.upsert_binding(&rebound)?;
        }
        Ok(bindings)
    }
}

fn channel_key_from_source(source: ChatSessionSource) -> Option<&'static str> {
    match source {
        ChatSessionSource::Telegram => Some("telegram"),
        ChatSessionSource::Discord => Some("discord"),
        ChatSessionSource::Slack => Some("slack"),
        ChatSessionSource::Workspace | ChatSessionSource::ExternalLegacy => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChannelSessionBinding, LifecycleTrace};
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
            ExecutionTraceStorage::new(db).unwrap(),
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
                    &restflow_telemetry::RestflowTrace::new(
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
                .channel_session_bindings
                .list_by_session(&session.id)
                .unwrap()
                .is_empty()
        );
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
