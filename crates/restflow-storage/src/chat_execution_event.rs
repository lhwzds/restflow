//! Chat execution event storage - byte-level API for tool-call timeline persistence.
//!
//! Stores append-only execution events (tool start/end, turn status) for chat sessions.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level chat execution event storage with byte-level API.
    ///
    /// Events are append-only records used for timeline visualization and debugging.
    pub struct ChatExecutionEventStorage { table: "chat_execution_events" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimpleStorage;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_new_creates_table() {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        let storage = ChatExecutionEventStorage::new(db);
        assert!(storage.is_ok());
    }

    #[test]
    fn test_put_and_get_raw() {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        let storage = ChatExecutionEventStorage::new(db).expect("storage");

        storage
            .put_raw("session-1:event-1", br#"{"ok":true}"#)
            .expect("put");
        let value = storage.get_raw("session-1:event-1").expect("get");
        assert_eq!(value, Some(br#"{"ok":true}"#.to_vec()));
    }

    #[test]
    fn test_list_raw() {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        let storage = ChatExecutionEventStorage::new(db).expect("storage");

        storage.put_raw("a", b"1").expect("put a");
        storage.put_raw("b", b"2").expect("put b");

        let items = storage.list_raw().expect("list");
        assert_eq!(items.len(), 2);
    }
}
