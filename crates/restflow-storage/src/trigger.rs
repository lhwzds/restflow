//! Trigger storage - byte-level API for trigger persistence.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level trigger storage with byte-level API
    pub struct TriggerStorage { table: "active_triggers" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimpleStorage;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_put_and_get_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = TriggerStorage::new(db).unwrap();

        let data = b"test trigger data";
        storage.put_raw("trigger-001", data).unwrap();

        let retrieved = storage.get_raw("trigger-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_list_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = TriggerStorage::new(db).unwrap();

        storage.put_raw("trigger-001", b"data1").unwrap();
        storage.put_raw("trigger-002", b"data2").unwrap();

        let triggers = storage.list_raw().unwrap();
        assert_eq!(triggers.len(), 2);
    }

    #[test]
    fn test_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = TriggerStorage::new(db).unwrap();

        storage.put_raw("trigger-001", b"data").unwrap();

        let deleted = storage.delete("trigger-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_raw("trigger-001").unwrap();
        assert!(retrieved.is_none());
    }
}
