//! Chat session storage - byte-level API for chat session persistence.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const CHAT_SESSIONS_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("chat_sessions");

/// Low-level chat session storage with byte-level API
#[derive(Debug, Clone)]
pub struct ChatSessionStorage {
    db: Arc<Database>,
}

impl ChatSessionStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(CHAT_SESSIONS_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Store raw session data
    pub fn put_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CHAT_SESSIONS_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw session data by ID
    pub fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHAT_SESSIONS_TABLE)?;

        if let Some(data) = table.get(id)? {
            Ok(Some(data.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all raw session data
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

    /// Check if session exists
    pub fn exists(&self, id: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHAT_SESSIONS_TABLE)?;
        Ok(table.get(id)?.is_some())
    }

    /// Delete session by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(CHAT_SESSIONS_TABLE)?;
            table.remove(id)?.is_some()
        };
        write_txn.commit()?;
        Ok(existed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
    fn test_exists_and_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        assert!(!storage.exists("session-001").unwrap());

        storage.put_raw("session-001", b"data").unwrap();
        assert!(storage.exists("session-001").unwrap());

        let deleted = storage.delete("session-001").unwrap();
        assert!(deleted);
        assert!(!storage.exists("session-001").unwrap());
    }
}
