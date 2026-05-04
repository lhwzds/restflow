//! Message mirroring for real-time conversation persistence.
//!
//! This module provides a simple interface to persist chat messages to
//! the session service as they are produced.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use crate::models::chat_session::{ChatMessage, ChatSession, MessageExecution};
use crate::services::session::SessionService;

/// Trait for mirroring messages to persistent storage.
#[async_trait]
pub trait MessageMirror: Send + Sync {
    /// Mirror an assistant message to the session.
    async fn mirror_assistant(
        &self,
        session_id: &str,
        content: &str,
        tokens: Option<u32>,
    ) -> Result<()>;

    /// Mirror a user message to the session.
    async fn mirror_user(&self, session_id: &str, content: &str) -> Result<()>;

    /// Create a new session and return its ID.
    async fn ensure_session(&self, agent_id: &str, model: &str) -> Result<String>;
}

/// SessionService-backed message mirror implementation.
pub struct ChatSessionMirror {
    service: Arc<SessionService>,
}

impl ChatSessionMirror {
    pub fn new(service: Arc<SessionService>) -> Self {
        Self { service }
    }

    fn load_session(&self, session_id: &str) -> Result<ChatSession> {
        self.service
            .get_session_view(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))
    }

    fn save_session(&self, session: &ChatSession) -> Result<()> {
        self.service.save_existing_session(session, "mirror")
    }
}

#[async_trait]
impl MessageMirror for ChatSessionMirror {
    async fn mirror_assistant(
        &self,
        session_id: &str,
        content: &str,
        tokens: Option<u32>,
    ) -> Result<()> {
        let mut session = self.load_session(session_id)?;
        let mut message = ChatMessage::assistant(content);

        if let Some(value) = tokens {
            let execution = MessageExecution::new().complete(0, value);
            message = message.with_execution(execution);
        }

        session.add_message(message);
        self.save_session(&session)?;

        tracing::debug!(
            session_id = %session_id,
            content_len = content.len(),
            "Mirrored assistant message"
        );

        Ok(())
    }

    async fn mirror_user(&self, session_id: &str, content: &str) -> Result<()> {
        let mut session = self.load_session(session_id)?;
        session.add_message(ChatMessage::user(content));
        self.save_session(&session)?;

        tracing::debug!(
            session_id = %session_id,
            content_len = content.len(),
            "Mirrored user message"
        );

        Ok(())
    }

    async fn ensure_session(&self, agent_id: &str, model: &str) -> Result<String> {
        let session = self.service.create_workspace_session(
            agent_id.to_string(),
            model.to_string(),
            None,
            None,
            None,
        )?;
        let session_id = session.id.clone();
        Ok(session_id)
    }
}

/// No-op mirror for when persistence is disabled.
pub struct NoopMirror;

#[async_trait]
impl MessageMirror for NoopMirror {
    async fn mirror_assistant(
        &self,
        _session_id: &str,
        _content: &str,
        _tokens: Option<u32>,
    ) -> Result<()> {
        Ok(())
    }

    async fn mirror_user(&self, _session_id: &str, _content: &str) -> Result<()> {
        Ok(())
    }

    async fn ensure_session(&self, _agent_id: &str, _model: &str) -> Result<String> {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;
    use tempfile::tempdir;

    fn setup_mirror() -> (ChatSessionMirror, Storage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let storage = Storage::new(temp_dir.path().join("test.db").to_str().unwrap()).unwrap();
        let service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        );
        (ChatSessionMirror::new(Arc::new(service)), storage, temp_dir)
    }

    #[tokio::test]
    async fn test_mirror_creates_messages() {
        let (mirror, storage, _temp_dir) = setup_mirror();

        let session_id = mirror
            .ensure_session("agent-1", "claude-sonnet")
            .await
            .unwrap();

        mirror.mirror_user(&session_id, "Hello!").await.unwrap();
        mirror
            .mirror_assistant(&session_id, "Hi there!", Some(50))
            .await
            .unwrap();

        let session = storage.chat_sessions.get(&session_id).unwrap().unwrap();
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].content, "Hello!");
        assert_eq!(session.messages[1].content, "Hi there!");
    }

    #[tokio::test]
    async fn test_mirror_updates_metadata() {
        let (mirror, storage, _temp_dir) = setup_mirror();

        let session_id = mirror
            .ensure_session("agent-1", "claude-sonnet")
            .await
            .unwrap();

        mirror
            .mirror_assistant(&session_id, "Response", Some(100))
            .await
            .unwrap();

        let session = storage.chat_sessions.get(&session_id).unwrap().unwrap();
        assert_eq!(session.metadata.total_tokens, 100);
        assert_eq!(session.metadata.message_count, 1);
    }

    #[tokio::test]
    async fn test_noop_mirror() {
        let mirror = NoopMirror;

        mirror.mirror_user("any", "test").await.unwrap();
        mirror.mirror_assistant("any", "test", None).await.unwrap();
        let session_id = mirror.ensure_session("agent", "model").await.unwrap();
        assert!(session_id.is_empty());
    }
}
