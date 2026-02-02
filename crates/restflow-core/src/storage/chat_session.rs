//! Typed chat session storage wrapper.

use anyhow::Result;
use restflow_storage::ChatSessionStorage as RawChatSessionStorage;
use std::sync::Arc;

use crate::models::ChatSession;

/// Typed chat session storage wrapper around restflow-storage::ChatSessionStorage.
#[derive(Debug, Clone)]
pub struct ChatSessionStorage {
    inner: RawChatSessionStorage,
}

impl ChatSessionStorage {
    pub fn new(db: Arc<redb::Database>) -> Result<Self> {
        Ok(Self {
            inner: RawChatSessionStorage::new(db)?,
        })
    }

    /// Store a chat session (upsert)
    pub fn upsert(&self, session: &ChatSession) -> Result<()> {
        let json = serde_json::to_vec(session)?;
        self.inner.put_raw(&session.id, &json)
    }

    /// Get a chat session by ID
    pub fn get(&self, id: &str) -> Result<Option<ChatSession>> {
        if let Some(data) = self.inner.get_raw(id)? {
            let session: ChatSession = serde_json::from_slice(&data)?;
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    /// List all chat sessions
    pub fn list(&self) -> Result<Vec<ChatSession>> {
        let raw_sessions = self.inner.list_raw()?;
        let mut sessions = Vec::new();
        for (_, bytes) in raw_sessions {
            let session: ChatSession = serde_json::from_slice(&bytes)?;
            sessions.push(session);
        }
        sessions.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
        Ok(sessions)
    }

    /// Delete a session by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.inner.delete(id)
    }

    /// Check if a session exists
    pub fn exists(&self, id: &str) -> Result<bool> {
        self.inner.exists(id)
    }

    /// Access raw storage for advanced use cases
    pub fn raw(&self) -> &RawChatSessionStorage {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_upsert_and_get() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        let session = ChatSession::new("agent-1".to_string(), "model-1".to_string());
        storage.upsert(&session).unwrap();

        let loaded = storage.get(&session.id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, session.id);
    }
}
