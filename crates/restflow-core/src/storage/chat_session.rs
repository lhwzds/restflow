//! Typed chat session storage wrapper.
//!
//! Provides type-safe access to chat session storage, wrapping the byte-level
//! API from restflow-storage with our Rust models.

use crate::models::{AIModel, ChatSession, ChatSessionSummary};
use anyhow::Result;
use redb::Database;
use restflow_storage::SimpleStorage;
use std::sync::Arc;

/// Typed chat session storage wrapper around restflow-storage::ChatSessionStorage.
///
/// Provides CRUD operations for chat sessions with automatic JSON serialization.
#[derive(Debug, Clone)]
pub struct ChatSessionStorage {
    inner: restflow_storage::ChatSessionStorage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SessionRetentionCleanupStats {
    pub scanned: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub failed: usize,
    pub bytes_freed: u64,
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

fn normalize_session_model_id(model: &str) -> String {
    AIModel::normalize_model_id(model).unwrap_or_else(|| model.trim().to_string())
}

fn normalize_session_model(session: &mut ChatSession) -> bool {
    let normalized = normalize_session_model_id(&session.model);
    if normalized == session.model {
        return false;
    }
    session.model = normalized;
    true
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
        let mut normalized = session.clone();
        normalize_session_model(&mut normalized);

        if self.inner.exists(&normalized.id)? {
            return Err(anyhow::anyhow!(
                "Chat session {} already exists",
                normalized.id
            ));
        }
        let json = serde_json::to_string(&normalized)?;
        self.inner.put_raw(&normalized.id, json.as_bytes())
    }

    /// Get a chat session by ID.
    pub fn get(&self, id: &str) -> Result<Option<ChatSession>> {
        if let Some(bytes) = self.inner.get_raw(id)? {
            let json = std::str::from_utf8(&bytes)?;
            let mut session: ChatSession = serde_json::from_str(json)?;
            if normalize_session_model(&mut session) {
                let normalized_json = serde_json::to_string(&session)?;
                self.inner.put_raw(id, normalized_json.as_bytes())?;
            }
            Ok(Some(session))
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
        let mut normalized_updates: Vec<(String, String)> = Vec::new();
        for (id, bytes) in raw_sessions {
            let json = std::str::from_utf8(&bytes)?;
            let mut session: ChatSession = serde_json::from_str(json)?;
            if normalize_session_model(&mut session) {
                normalized_updates.push((id, serde_json::to_string(&session)?));
            }
            sessions.push(session);
        }

        for (id, normalized_json) in normalized_updates {
            self.inner.put_raw(&id, normalized_json.as_bytes())?;
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
        let mut normalized = session.clone();
        normalize_session_model(&mut normalized);

        if !self.inner.exists(&normalized.id)? {
            return Err(anyhow::anyhow!("Chat session {} not found", normalized.id));
        }
        let json = serde_json::to_string(&normalized)?;
        self.inner.put_raw(&normalized.id, json.as_bytes())
    }

    /// Save a chat session (create or update).
    pub fn save(&self, session: &ChatSession) -> Result<()> {
        let mut normalized = session.clone();
        normalize_session_model(&mut normalized);
        let json = serde_json::to_string(&normalized)?;
        self.inner.put_raw(&normalized.id, json.as_bytes())
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

    /// Delete expired sessions using retention-days policy.
    ///
    /// `retention_days = 0` disables cleanup.
    pub fn cleanup_expired(&self, retention_days: u32, now_ms: i64) -> Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }

        let cutoff = now_ms - (retention_days as i64) * 24 * 60 * 60 * 1000;
        self.delete_older_than(cutoff)
    }

    /// Delete sessions that exceed their own per-session retention policy.
    ///
    /// Supported values: `1h`, `1d`, `7d`, `30d`.
    /// Missing policy means "keep forever".
    pub fn cleanup_by_session_retention(
        &self,
        now_ms: i64,
    ) -> Result<SessionRetentionCleanupStats> {
        let sessions = self.list()?;
        let mut stats = SessionRetentionCleanupStats {
            scanned: sessions.len(),
            ..SessionRetentionCleanupStats::default()
        };

        for session in sessions {
            let Some(retention) = session.retention.as_deref() else {
                stats.skipped += 1;
                continue;
            };

            let Some(retention_ms) = parse_retention_to_ms(retention) else {
                stats.failed += 1;
                continue;
            };

            let expires_at = session.updated_at.saturating_add(retention_ms);
            if now_ms >= expires_at {
                let serialized_len = serde_json::to_vec(&session)
                    .map(|bytes| bytes.len() as u64)
                    .unwrap_or(0);
                self.delete(&session.id)?;
                stats.deleted += 1;
                stats.bytes_freed += serialized_len;
            } else {
                stats.skipped += 1;
            }
        }

        Ok(stats)
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
        assert_eq!(retrieved.model, "claude-sonnet-4-5");
    }

    #[test]
    fn test_create_normalizes_model_id() {
        let (storage, _temp_dir) = setup();

        let session = ChatSession::new("agent-1".to_string(), "MiniMax-M2.5".to_string());
        let id = session.id.clone();
        storage.create(&session).unwrap();

        let retrieved = storage.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.model, "minimax-m2-5");
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
    fn test_list_normalizes_and_persists_legacy_model_id() {
        let (storage, _temp_dir) = setup();
        let mut legacy = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        legacy.model = "MiniMax-M2.5".to_string();
        let id = legacy.id.clone();

        let raw = serde_json::to_string(&legacy).unwrap();
        storage.inner.put_raw(&id, raw.as_bytes()).unwrap();

        let sessions = storage.list().unwrap();
        let listed = sessions.into_iter().find(|s| s.id == id).unwrap();
        assert_eq!(listed.model, "minimax-m2-5");

        let persisted = storage.get(&id).unwrap().unwrap();
        assert_eq!(persisted.model, "minimax-m2-5");
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

    #[test]
    fn test_cleanup_expired_deletes_old_only() {
        let (storage, _temp_dir) = setup();
        let now = chrono::Utc::now().timestamp_millis();

        let mut expired = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        expired.updated_at = now - (31 * 24 * 60 * 60 * 1000);

        let mut recent = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        recent.updated_at = now - (2 * 24 * 60 * 60 * 1000);

        storage.save(&expired).unwrap();
        storage.save(&recent).unwrap();

        let deleted = storage.cleanup_expired(30, now).unwrap();
        assert_eq!(deleted, 1);
        assert!(!storage.exists(&expired.id).unwrap());
        assert!(storage.exists(&recent.id).unwrap());
    }

    #[test]
    fn test_parse_retention_to_ms() {
        assert_eq!(parse_retention_to_ms("1h"), Some(60 * 60 * 1000));
        assert_eq!(parse_retention_to_ms("1d"), Some(24 * 60 * 60 * 1000));
        assert_eq!(parse_retention_to_ms("7d"), Some(7 * 24 * 60 * 60 * 1000));
        assert_eq!(parse_retention_to_ms("30d"), Some(30 * 24 * 60 * 60 * 1000));
        assert_eq!(parse_retention_to_ms("invalid"), None);
    }

    #[test]
    fn test_cleanup_by_session_retention() {
        let (storage, _temp_dir) = setup();
        let now = chrono::Utc::now().timestamp_millis();

        let mut expired = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_retention("1d");
        expired.updated_at = now - (2 * 24 * 60 * 60 * 1000);

        let mut recent = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_retention("7d");
        recent.updated_at = now - (2 * 24 * 60 * 60 * 1000);

        let invalid = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_retention("2w");
        let no_retention = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());

        storage.save(&expired).unwrap();
        storage.save(&recent).unwrap();
        storage.save(&invalid).unwrap();
        storage.save(&no_retention).unwrap();

        let stats = storage.cleanup_by_session_retention(now).unwrap();
        assert_eq!(stats.scanned, 4);
        assert_eq!(stats.deleted, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.skipped, 2);
        assert!(stats.bytes_freed > 0);
        assert!(!storage.exists(&expired.id).unwrap());
        assert!(storage.exists(&recent.id).unwrap());
    }
}
