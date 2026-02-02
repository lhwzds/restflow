//! Chat session storage - byte-level API for chat session persistence.
//!
//! Provides low-level storage for chat sessions used in the SkillWorkspace.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level chat session storage with byte-level API.
    ///
    /// Chat sessions store conversation history with agents, including messages,
    /// execution details, and metadata.
    pub struct ChatSessionStorage { table: "chat_sessions" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::sync::Arc;
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

        let sessions = storage.list_raw(None).unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_list_raw_empty() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChatSessionStorage::new(db).unwrap();

        let sessions = storage.list_raw(None).unwrap();
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
