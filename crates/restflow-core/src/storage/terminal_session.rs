//! Typed terminal session storage wrapper.

use crate::models::TerminalSession;
use anyhow::Result;
use redb::Database;
use std::sync::Arc;

/// Typed terminal session storage wrapper around restflow-storage::TerminalSessionStorage.
#[derive(Debug, Clone)]
pub struct TerminalSessionStorage {
    inner: restflow_storage::TerminalSessionStorage,
}

impl TerminalSessionStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::TerminalSessionStorage::new(db)?,
        })
    }

    /// Create a new terminal session (fails if already exists)
    pub fn create(&self, session: &TerminalSession) -> Result<()> {
        if self.inner.exists(&session.id)? {
            return Err(anyhow::anyhow!(
                "Terminal session {} already exists",
                session.id
            ));
        }
        let json = serde_json::to_string(session)?;
        self.inner.put_raw(&session.id, json.as_bytes())
    }

    /// Get a terminal session by ID
    pub fn get(&self, id: &str) -> Result<Option<TerminalSession>> {
        if let Some(bytes) = self.inner.get_raw(id)? {
            let json = std::str::from_utf8(&bytes)?;
            Ok(Some(serde_json::from_str(json)?))
        } else {
            Ok(None)
        }
    }

    /// List all terminal sessions
    pub fn list(&self) -> Result<Vec<TerminalSession>> {
        let raw_sessions = self.inner.list_raw()?;
        let mut sessions = Vec::new();
        for (_, bytes) in raw_sessions {
            let json = std::str::from_utf8(&bytes)?;
            let session: TerminalSession = serde_json::from_str(json)?;
            sessions.push(session);
        }

        // Sort by created_at ascending (oldest first)
        sessions.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(sessions)
    }

    /// Update an existing terminal session
    pub fn update(&self, id: &str, session: &TerminalSession) -> Result<()> {
        if !self.inner.exists(id)? {
            return Err(anyhow::anyhow!("Terminal session {} not found", id));
        }
        let json = serde_json::to_string(session)?;
        self.inner.put_raw(id, json.as_bytes())
    }

    /// Delete a terminal session
    pub fn delete(&self, id: &str) -> Result<()> {
        self.inner.delete(id)?;
        Ok(())
    }

    /// Check if a terminal session exists
    pub fn exists(&self, id: &str) -> Result<bool> {
        self.inner.exists(id)
    }

    /// Mark all running sessions as stopped.
    /// This should be called on app startup to clean up stale sessions
    /// from previous runs where the PTY processes no longer exist.
    pub fn mark_all_stopped(&self) -> Result<usize> {
        let sessions = self.list()?;
        let mut count = 0;

        for mut session in sessions {
            if session.is_running() {
                session.set_stopped(session.history.clone());
                self.update(&session.id, &session)?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Get the next available terminal name (Terminal 1, Terminal 2, etc.)
    pub fn get_next_name(&self) -> Result<String> {
        let sessions = self.list()?;
        let pattern = regex::Regex::new(r"^Terminal (\d+)$").unwrap();

        let max_num = sessions
            .iter()
            .filter_map(|s| pattern.captures(&s.name))
            .filter_map(|caps| caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok()))
            .max()
            .unwrap_or(0);

        Ok(format!("Terminal {}", max_num + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (TerminalSessionStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = TerminalSessionStorage::new(db).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_create_and_get() {
        let (storage, _temp_dir) = setup();

        let session = TerminalSession::new("terminal-001".to_string(), "Terminal 1".to_string());

        storage.create(&session).unwrap();

        let retrieved = storage.get("terminal-001").unwrap().unwrap();
        assert_eq!(retrieved.id, "terminal-001");
        assert_eq!(retrieved.name, "Terminal 1");
    }

    #[test]
    fn test_create_duplicate_fails() {
        let (storage, _temp_dir) = setup();

        let session = TerminalSession::new("terminal-001".to_string(), "Terminal 1".to_string());

        storage.create(&session).unwrap();
        let result = storage.create(&session);
        assert!(result.is_err());
    }

    #[test]
    fn test_list() {
        let (storage, _temp_dir) = setup();

        let session1 = TerminalSession::new("terminal-001".to_string(), "Terminal 1".to_string());
        let session2 = TerminalSession::new("terminal-002".to_string(), "Terminal 2".to_string());

        storage.create(&session1).unwrap();
        storage.create(&session2).unwrap();

        let sessions = storage.list().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_update() {
        let (storage, _temp_dir) = setup();

        let mut session =
            TerminalSession::new("terminal-001".to_string(), "Terminal 1".to_string());

        storage.create(&session).unwrap();

        session.rename("My Custom Terminal".to_string());

        storage.update("terminal-001", &session).unwrap();

        let retrieved = storage.get("terminal-001").unwrap().unwrap();
        assert_eq!(retrieved.name, "My Custom Terminal");
    }

    #[test]
    fn test_delete() {
        let (storage, _temp_dir) = setup();

        let session = TerminalSession::new("terminal-001".to_string(), "Terminal 1".to_string());

        storage.create(&session).unwrap();
        assert!(storage.exists("terminal-001").unwrap());

        storage.delete("terminal-001").unwrap();
        assert!(!storage.exists("terminal-001").unwrap());
    }

    #[test]
    fn test_mark_all_stopped() {
        let (storage, _temp_dir) = setup();

        // Create some sessions
        let session1 = TerminalSession::new("terminal-001".to_string(), "Terminal 1".to_string());
        let mut session2 = TerminalSession::new("terminal-002".to_string(), "Terminal 2".to_string());
        session2.set_stopped(None); // Already stopped

        storage.create(&session1).unwrap();
        storage.create(&session2).unwrap();

        // Verify initial state
        assert!(storage.get("terminal-001").unwrap().unwrap().is_running());
        assert!(!storage.get("terminal-002").unwrap().unwrap().is_running());

        // Mark all as stopped
        let count = storage.mark_all_stopped().unwrap();
        assert_eq!(count, 1); // Only 1 was running

        // Verify all are now stopped
        assert!(!storage.get("terminal-001").unwrap().unwrap().is_running());
        assert!(!storage.get("terminal-002").unwrap().unwrap().is_running());
    }

    #[test]
    fn test_get_next_name() {
        let (storage, _temp_dir) = setup();

        // Empty - should be "Terminal 1"
        assert_eq!(storage.get_next_name().unwrap(), "Terminal 1");

        // Add Terminal 1
        let session1 = TerminalSession::new("terminal-001".to_string(), "Terminal 1".to_string());
        storage.create(&session1).unwrap();
        assert_eq!(storage.get_next_name().unwrap(), "Terminal 2");

        // Add Terminal 3 (skip 2)
        let session3 = TerminalSession::new("terminal-003".to_string(), "Terminal 3".to_string());
        storage.create(&session3).unwrap();
        assert_eq!(storage.get_next_name().unwrap(), "Terminal 4");

        // Add custom name (should not affect numbering)
        let session_custom =
            TerminalSession::new("terminal-custom".to_string(), "My Terminal".to_string());
        storage.create(&session_custom).unwrap();
        assert_eq!(storage.get_next_name().unwrap(), "Terminal 4");
    }
}
