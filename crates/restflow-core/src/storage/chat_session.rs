//! Typed chat session storage wrapper.
//!
//! Provides type-safe access to chat session storage, wrapping the byte-level
//! API from restflow-storage with our Rust models.

use crate::models::{ChatSession, ChatSessionSummary};
use anyhow::Result;
use redb::Database;
use std::sync::Arc;

/// Typed chat session storage wrapper around restflow-storage::ChatSessionStorage.
///
/// Provides CRUD operations for chat sessions with automatic JSON serialization.
#[derive(Debug, Clone)]
pub struct ChatSessionStorage {
    inner: restflow_storage::ChatSessionStorage,
}

impl ChatSessionStorage {
    /// Create a new chat session storage instance.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::ChatSessionStorage::new(db)?,
        })
    }

    /// Create a new chat session (fails if already exists).
    pub fn create(&self, session: &ChatSession) -> Result<()> {
        if self.inner.exists(&session.id)? {
            return Err(anyhow::anyhow!(
                "Chat session {} already exists",
                session.id
            ));
        }
        let json = serde_json::to_string(session)?;
        self.inner.put_raw(&session.id, json.as_bytes())
    }

    /// Get a chat session by ID.
    pub fn get(&self, id: &str) -> Result<Option<ChatSession>> {
        if let Some(bytes) = self.inner.get_raw(id)? {
            let json = std::str::from_utf8(&bytes)?;
            Ok(Some(serde_json::from_str(json)?))
        } else {
            Ok(None)
        }
    }

    /// List all chat sessions.
    ///
    /// Returns sessions sorted by updated_at descending (most recent first).
    pub fn list(&self) -> Result<Vec<ChatSession>> {
        let raw_sessions = self.inner.list_raw()?;
        let mut sessions = Vec::new();
        for (_, bytes) in raw_sessions {
            let json = std::str::from_utf8(&bytes)?;
            let session: ChatSession = serde_json::from_str(json)?;
            sessions.push(session);
        }

        // Sort by updated_at descending (most recent first)
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(sessions)
    }

    /// List all chat sessions as summaries.
    ///
    /// More efficient than list() when you don't need full message history.
    pub fn list_summaries(&self) -> Result<Vec<ChatSessionSummary>> {
        let sessions = self.list()?;
        Ok(sessions.iter().map(ChatSessionSummary::from).collect())
    }

    /// List chat sessions for a specific agent.
    pub fn list_by_agent(&self, agent_id: &str) -> Result<Vec<ChatSession>> {
        let sessions = self.list()?;
        Ok(sessions
            .into_iter()
            .filter(|s| s.agent_id == agent_id)
            .collect())
    }

    /// List chat sessions for a specific skill.
    pub fn list_by_skill(&self, skill_id: &str) -> Result<Vec<ChatSession>> {
        let sessions = self.list()?;
        Ok(sessions
            .into_iter()
            .filter(|s| s.skill_id.as_ref() == Some(&skill_id.to_string()))
            .collect())
    }

    /// Update an existing chat session.
    pub fn update(&self, session: &ChatSession) -> Result<()> {
        if !self.inner.exists(&session.id)? {
            return Err(anyhow::anyhow!("Chat session {} not found", session.id));
        }
        let json = serde_json::to_string(session)?;
        self.inner.put_raw(&session.id, json.as_bytes())
    }

    /// Save a chat session (create or update).
    pub fn save(&self, session: &ChatSession) -> Result<()> {
        let json = serde_json::to_string(session)?;
        self.inner.put_raw(&session.id, json.as_bytes())
    }

    /// Delete a chat session.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.inner.delete(id)
    }

    /// Check if a chat session exists.
    pub fn exists(&self, id: &str) -> Result<bool> {
        self.inner.exists(id)
    }

    /// Count total number of chat sessions.
    pub fn count(&self) -> Result<usize> {
        self.inner.count()
    }

    /// Delete all sessions older than the given timestamp.
    ///
    /// Returns the number of deleted sessions.
    pub fn delete_older_than(&self, timestamp_ms: i64) -> Result<usize> {
        let sessions = self.list()?;
        let mut deleted = 0;

        for session in sessions {
            if session.updated_at < timestamp_ms {
                self.delete(&session.id)?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChatMessage, ChatRole};
    use tempfile::tempdir;

    fn setup() -> (ChatSessionStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_create_and_get() {
        let (storage, _temp_dir) = setup();

        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_name("Test Chat");

        storage.create(&session).unwrap();

        let retrieved = storage.get(&session.id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Test Chat");
        assert_eq!(retrieved.agent_id, "agent-1");
        assert_eq!(retrieved.model, "claude-sonnet-4");
    }

    #[test]
    fn test_create_duplicate_fails() {
        let (storage, _temp_dir) = setup();

        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        let id = session.id.clone();

        storage.create(&session).unwrap();

        // Try to create again with same ID
        let mut session2 = ChatSession::new("agent-2".to_string(), "gpt-4".to_string());
        session2.id = id; // Force same ID

        let result = storage.create(&session2);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_nonexistent() {
        let (storage, _temp_dir) = setup();

        let result = storage.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list() {
        let (storage, _temp_dir) = setup();

        let session1 = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        let session2 = ChatSession::new("agent-2".to_string(), "gpt-4".to_string());

        storage.create(&session1).unwrap();
        storage.create(&session2).unwrap();

        let sessions = storage.list().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_list_sorted_by_updated_at() {
        let (storage, _temp_dir) = setup();

        let mut session1 = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session1.updated_at = 1000;

        let mut session2 = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session2.updated_at = 3000;

        let mut session3 = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session3.updated_at = 2000;

        storage.create(&session1).unwrap();
        storage.create(&session2).unwrap();
        storage.create(&session3).unwrap();

        let sessions = storage.list().unwrap();
        assert_eq!(sessions.len(), 3);
        // Should be sorted by updated_at descending
        assert_eq!(sessions[0].updated_at, 3000);
        assert_eq!(sessions[1].updated_at, 2000);
        assert_eq!(sessions[2].updated_at, 1000);
    }

    #[test]
    fn test_list_summaries() {
        let (storage, _temp_dir) = setup();

        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_name("Test Chat");
        session.add_message(ChatMessage::user("Hello!"));

        storage.create(&session).unwrap();

        let summaries = storage.list_summaries().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].name, "Test Chat");
        assert_eq!(summaries[0].message_count, 1);
        assert_eq!(
            summaries[0].last_message_preview,
            Some("Hello!".to_string())
        );
    }

    #[test]
    fn test_list_by_agent() {
        let (storage, _temp_dir) = setup();

        let session1 = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        let session2 = ChatSession::new("agent-2".to_string(), "gpt-4".to_string());
        let session3 = ChatSession::new("agent-1".to_string(), "claude-opus-4".to_string());

        storage.create(&session1).unwrap();
        storage.create(&session2).unwrap();
        storage.create(&session3).unwrap();

        let agent1_sessions = storage.list_by_agent("agent-1").unwrap();
        assert_eq!(agent1_sessions.len(), 2);
        assert!(agent1_sessions.iter().all(|s| s.agent_id == "agent-1"));
    }

    #[test]
    fn test_list_by_skill() {
        let (storage, _temp_dir) = setup();

        let session1 = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_skill("skill-1");
        let session2 = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_skill("skill-2");
        let session3 = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string()); // No skill

        storage.create(&session1).unwrap();
        storage.create(&session2).unwrap();
        storage.create(&session3).unwrap();

        let skill1_sessions = storage.list_by_skill("skill-1").unwrap();
        assert_eq!(skill1_sessions.len(), 1);
    }

    #[test]
    fn test_update() {
        let (storage, _temp_dir) = setup();

        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_name("Original Name");

        storage.create(&session).unwrap();

        session.rename("Updated Name");
        storage.update(&session).unwrap();

        let retrieved = storage.get(&session.id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
    }

    #[test]
    fn test_update_nonexistent_fails() {
        let (storage, _temp_dir) = setup();

        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());

        let result = storage.update(&session);
        assert!(result.is_err());
    }

    #[test]
    fn test_save_creates_or_updates() {
        let (storage, _temp_dir) = setup();

        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_name("Initial");

        // Save creates
        storage.save(&session).unwrap();
        let retrieved = storage.get(&session.id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Initial");

        // Save updates
        session.rename("Updated");
        storage.save(&session).unwrap();
        let retrieved = storage.get(&session.id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated");
    }

    #[test]
    fn test_delete() {
        let (storage, _temp_dir) = setup();

        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        let id = session.id.clone();

        storage.create(&session).unwrap();
        assert!(storage.exists(&id).unwrap());

        let deleted = storage.delete(&id).unwrap();
        assert!(deleted);
        assert!(!storage.exists(&id).unwrap());
    }

    #[test]
    fn test_delete_nonexistent() {
        let (storage, _temp_dir) = setup();

        let deleted = storage.delete("nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_count() {
        let (storage, _temp_dir) = setup();

        assert_eq!(storage.count().unwrap(), 0);

        storage
            .create(&ChatSession::new(
                "agent-1".to_string(),
                "claude-sonnet-4".to_string(),
            ))
            .unwrap();
        assert_eq!(storage.count().unwrap(), 1);

        storage
            .create(&ChatSession::new(
                "agent-2".to_string(),
                "gpt-4".to_string(),
            ))
            .unwrap();
        assert_eq!(storage.count().unwrap(), 2);
    }

    #[test]
    fn test_delete_older_than() {
        let (storage, _temp_dir) = setup();

        let mut old_session =
            ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        old_session.updated_at = 1000;

        let mut new_session =
            ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        new_session.updated_at = 3000;

        storage.create(&old_session).unwrap();
        storage.create(&new_session).unwrap();

        let deleted = storage.delete_older_than(2000).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(storage.count().unwrap(), 1);
    }

    #[test]
    fn test_session_with_messages() {
        let (storage, _temp_dir) = setup();

        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session.add_message(ChatMessage::user("Hello!"));
        session.add_message(ChatMessage::assistant("Hi there! How can I help?"));

        storage.create(&session).unwrap();

        let retrieved = storage.get(&session.id).unwrap().unwrap();
        assert_eq!(retrieved.messages.len(), 2);
        assert_eq!(retrieved.messages[0].role, ChatRole::User);
        assert_eq!(retrieved.messages[0].content, "Hello!");
        assert_eq!(retrieved.messages[1].role, ChatRole::Assistant);
    }
}
