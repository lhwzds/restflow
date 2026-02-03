//! Skill metadata storage - byte-level API for skill metadata persistence.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level skill metadata storage with byte-level API
    pub struct SkillMetaStorage { table: "skills_meta" }
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
        let storage = SkillMetaStorage::new(db).unwrap();

        let data = b"test skill meta data";
        storage.put_raw("skill-meta-001", data).unwrap();

        let retrieved = storage.get_raw("skill-meta-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_list_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SkillMetaStorage::new(db).unwrap();

        storage.put_raw("skill-meta-001", b"data1").unwrap();
        storage.put_raw("skill-meta-002", b"data2").unwrap();

        let items = storage.list_raw().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_exists_and_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SkillMetaStorage::new(db).unwrap();

        assert!(!storage.exists("skill-meta-001").unwrap());

        storage.put_raw("skill-meta-001", b"data").unwrap();
        assert!(storage.exists("skill-meta-001").unwrap());

        let deleted = storage.delete("skill-meta-001").unwrap();
        assert!(deleted);
        assert!(!storage.exists("skill-meta-001").unwrap());
    }
}
