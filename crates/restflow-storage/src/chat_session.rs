//! Chat session storage - byte-level API for chat session persistence.
//!
//! Provides low-level storage for chat sessions used in the SkillWorkspace.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::sync::Arc;

const CHAT_SESSIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("chat_sessions");

/// Low-level chat session storage with byte-level API.
///
/// Chat sessions store conversation history with agents, including messages,
/// execution details, and metadata.
#[derive(Debug, Clone)]
pub struct ChatSessionStorage {
    db: Arc<Database>,
}

impl ChatSessionStorage {
    /// Create a new chat session storage instance.
    ///
    /// Creates the required table if it doesn't exist.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(CHAT_SESSIONS_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Store raw session data.
    ///
    /// # Arguments
    /// * `id` - Unique session identifier
    /// * `data` - Serialized session data (typically JSON)
    pub fn put_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CHAT_SESSIONS_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw session data by ID.
    ///
    /// Returns `None` if the session doesn't exist.
    pub fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHAT_SESSIONS_TABLE)?;

        if let Some(data) = table.get(id)? {
            Ok(Some(data.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all raw session data.
    ///
    /// Returns a vector of (id, data) tuples for all stored sessions.
    pub fn list_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHAT_SESSIONS_TABLE)?;

        let mut sessions = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            sessions.push((key.value().to_string(), value.value().to_vec()));
        }

        Ok(sessions)
    }

    /// Check if a session exists.
    pub fn exists(&self, id: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHAT_SESSIONS_TABLE)?;
        Ok(table.get(id)?.is_some())
    }

    /// Delete a session by ID.
    ///
    /// Returns `true` if the session existed and was deleted.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(CHAT_SESSIONS_TABLE)?;
            table.remove(id)?.is_some()
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Count total number of sessions.
    pub fn count(&self) -> Result<usize> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHAT_SESSIONS_TABLE)?;
        Ok(table.len()? as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_new_creates_table() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db);
        assert!(storage.is_ok());
    }

    #[test]
    fn test_put_and_get_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        let data = b"test session data";
        storage.put_raw("session-001", data).unwrap();

        let retrieved = storage.get_raw("session-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_get_nonexistent() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        let retrieved = storage.get_raw("nonexistent").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_list_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        storage.put_raw("session-001", b"data1").unwrap();
        storage.put_raw("session-002", b"data2").unwrap();

        let sessions = storage.list_raw().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_list_raw_empty() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        let sessions = storage.list_raw().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_exists() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        assert!(!storage.exists("session-001").unwrap());

        storage.put_raw("session-001", b"data").unwrap();
        assert!(storage.exists("session-001").unwrap());
    }

    #[test]
    fn test_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        storage.put_raw("session-001", b"data").unwrap();
        assert!(storage.exists("session-001").unwrap());

        let deleted = storage.delete("session-001").unwrap();
        assert!(deleted);
        assert!(!storage.exists("session-001").unwrap());
    }

    #[test]
    fn test_delete_nonexistent() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        let deleted = storage.delete("nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_count() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        assert_eq!(storage.count().unwrap(), 0);

        storage.put_raw("session-001", b"data1").unwrap();
        assert_eq!(storage.count().unwrap(), 1);

        storage.put_raw("session-002", b"data2").unwrap();
        assert_eq!(storage.count().unwrap(), 2);

        storage.delete("session-001").unwrap();
        assert_eq!(storage.count().unwrap(), 1);
    }

    #[test]
    fn test_update_existing() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        storage.put_raw("session-001", b"original").unwrap();
        storage.put_raw("session-001", b"updated").unwrap();

        let retrieved = storage.get_raw("session-001").unwrap().unwrap();
        assert_eq!(retrieved, b"updated");
        assert_eq!(storage.count().unwrap(), 1);
    }
}
