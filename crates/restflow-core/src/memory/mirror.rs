//! Message mirroring for real-time conversation persistence.
//!
//! Similar to Moltbot's appendAssistantMessageToSessionTranscript,
//! this module provides automatic saving of messages to ChatSession.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use crate::models::{ChatMessage, ChatSession, MessageExecution};
use crate::storage::ChatSessionStorage;

/// Trait for mirroring messages to persistent storage.
///
/// Implementations save conversation messages in real-time,
/// ensuring no message is lost even if the application crashes.
#[async_trait]
pub trait MessageMirror: Send + Sync {
    /// Mirror an assistant message to the session.
    async fn mirror_assistant(&self, session_id: &str, content: &str, tokens: Option<u32>)
        -> Result<()>;
    /// Mirror a user message to the session.
    async fn mirror_user(&self, session_id: &str, content: &str) -> Result<()>;
    /// Get or create a session for the given agent.
    async fn ensure_session(&self, agent_id: &str, model: &str) -> Result<String>;
}

/// ChatSession-backed message mirror implementation.
pub struct ChatSessionMirror {
    storage: Arc<ChatSessionStorage>,
}

impl ChatSessionMirror {
    pub fn new(storage: Arc<ChatSessionStorage>) -> Self {
        Self { storage }
    }

    fn load_session(&self, session_id: &str) -> Result<Option<ChatSession>> {
        self.storage.get(session_id)
    }

    fn save_session(&self, session: &ChatSession) -> Result<()> {
        self.storage.upsert(session)
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
        let mut session = self
            .load_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
        let mut message = ChatMessage::assistant(content);
        if let Some(total_tokens) = tokens {
            let exec = MessageExecution::new().complete(0, total_tokens);
            message = message.with_execution(exec);
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
        let mut session = self
            .load_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
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
        let session = ChatSession::new(agent_id.to_string(), model.to_string());
        let session_id = session.id.clone();
        self.save_session(&session)?;
        Ok(session_id)
    }
}

/// No-op mirror for when persistence is disabled.
pub struct NoopMirror;

#[async_trait]
impl MessageMirror for NoopMirror {
    async fn mirror_assistant(
        &self,
        _: &str,
        _: &str,
        _: Option<u32>,
    ) -> Result<()> {
        Ok(())
    }

    async fn mirror_user(&self, _: &str, _: &str) -> Result<()> {
        Ok(())
    }

    async fn ensure_session(&self, _: &str, _: &str) -> Result<String> {
        Ok(String::new())
    }
}
