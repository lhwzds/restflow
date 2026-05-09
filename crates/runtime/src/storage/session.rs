//! Aggregated session-related storage wrappers.
//!
//! This groups chat session persistence behind a single typed entrypoint.

use crate::models::ChatSession;
use crate::storage::ChatSessionStorage;
use anyhow::Result;

#[derive(Clone)]
pub struct SessionStorage {
    pub chat_sessions: ChatSessionStorage,
}

impl SessionStorage {
    pub fn new(chat_sessions: ChatSessionStorage) -> Self {
        Self { chat_sessions }
    }

    pub fn cleanup_artifacts(&self, session_id: &str) -> Result<()> {
        let _ = session_id;
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
}
