//! Workflow storage - byte-level API for workflow persistence.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level workflow storage with byte-level API
    pub struct WorkflowStorage { table: "workflows" }
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
        let storage = WorkflowStorage::new(db).unwrap();

        let data = b"test workflow data";
        storage.put_raw("workflow-001", data).unwrap();

        let retrieved = storage.get_raw("workflow-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_list_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = WorkflowStorage::new(db).unwrap();

        storage.put_raw("workflow-001", b"data1").unwrap();
        storage.put_raw("workflow-002", b"data2").unwrap();

        let workflows = storage.list_raw().unwrap();
        assert_eq!(workflows.len(), 2);
    }

    #[test]
    fn test_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = WorkflowStorage::new(db).unwrap();

        storage.put_raw("workflow-001", b"data").unwrap();

        let deleted = storage.delete("workflow-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_raw("workflow-001").unwrap();
        assert!(retrieved.is_none());
    }
}
