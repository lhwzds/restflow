//! Shared space storage - byte-level API for shared space persistence.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level shared space storage with byte-level API
    pub struct SharedSpaceStorage { table: "shared_spaces" }
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
        let storage = SharedSpaceStorage::new(db).unwrap();

        let data = b"test shared space data";
        storage.put_raw("space-001", data).unwrap();

        let retrieved = storage.get_raw("space-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_list_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SharedSpaceStorage::new(db).unwrap();

        storage.put_raw("space-001", b"data1").unwrap();
        storage.put_raw("space-002", b"data2").unwrap();

        let spaces = storage.list_raw().unwrap();
        assert_eq!(spaces.len(), 2);
    }

    #[test]
    fn test_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SharedSpaceStorage::new(db).unwrap();

        storage.put_raw("space-001", b"data").unwrap();

        let deleted = storage.delete("space-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_raw("space-001").unwrap();
        assert!(retrieved.is_none());
    }
}
