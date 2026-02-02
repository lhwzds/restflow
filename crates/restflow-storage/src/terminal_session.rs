//! Terminal session storage - byte-level API for terminal session persistence.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level terminal session storage with byte-level API
    pub struct TerminalSessionStorage { table: "terminal_sessions" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_put_and_get_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = TerminalSessionStorage::new(db).unwrap();

        let data = b"test terminal session data";
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
        let storage = TerminalSessionStorage::new(db).unwrap();

        storage.put_raw("session-001", b"data1").unwrap();
        storage.put_raw("session-002", b"data2").unwrap();

        let sessions = storage.list_raw().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = TerminalSessionStorage::new(db).unwrap();

        storage.put_raw("session-001", b"data").unwrap();

        let deleted = storage.delete("session-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_raw("session-001").unwrap();
        assert!(retrieved.is_none());
    }
}
