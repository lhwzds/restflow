//! Skill storage - byte-level API for skill persistence.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level skill storage with byte-level API
    pub struct SkillStorage { table: "skills" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimpleStorage;
    use redb::Database;
    use std::sync::Arc;
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn test_put_and_get_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SkillStorage::new(db).unwrap();

        let data = b"test skill data";
        storage.put_raw("skill-001", data).unwrap();

        let retrieved = storage.get_raw("skill-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_list_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SkillStorage::new(db).unwrap();

        storage.put_raw("skill-001", b"data1").unwrap();
        storage.put_raw("skill-002", b"data2").unwrap();

        let skills = storage.list_raw().unwrap();
        assert_eq!(skills.len(), 2);
    }

    #[test]
    fn test_exists_and_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SkillStorage::new(db).unwrap();

        assert!(!storage.exists("skill-001").unwrap());

        storage.put_raw("skill-001", b"data").unwrap();
        assert!(storage.exists("skill-001").unwrap());

        let deleted = storage.delete("skill-001").unwrap();
        assert!(deleted);
        assert!(!storage.exists("skill-001").unwrap());
    }

    #[test]
    fn test_insert_if_absent() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SkillStorage::new(db).unwrap();

        // First insert should succeed
        let inserted = storage.insert_if_absent("skill-001", b"data1").unwrap();
        assert!(inserted, "First insert should return true");

        // Second insert should fail (key already exists)
        let inserted = storage.insert_if_absent("skill-001", b"data2").unwrap();
        assert!(!inserted, "Second insert should return false");

        // Verify original data is preserved
        let retrieved = storage.get_raw("skill-001").unwrap().unwrap();
        assert_eq!(retrieved, b"data1", "Original data should be preserved");
    }

    /// Regression test for TOCTOU race condition prevention
    /// This test verifies that concurrent insert_if_absent calls for the same key
    /// result in exactly one success, preventing the race condition described in:
    /// ISSUE_KEY: crates/restflow-core/src/storage/skill.rs:create:check-then-act-race
    #[test]
    fn test_insert_if_absent_concurrent_no_race() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = Arc::new(SkillStorage::new(db).unwrap());

        let key = "concurrent-key";
        let storage_clone = Arc::clone(&storage);
        let storage_clone2 = Arc::clone(&storage);

        // Spawn two threads that both try to insert the same key
        let handle1 = thread::spawn(move || storage_clone.insert_if_absent(key, b"thread1"));
        let handle2 = thread::spawn(move || storage_clone2.insert_if_absent(key, b"thread2"));

        let result1 = handle1.join().unwrap().unwrap();
        let result2 = handle2.join().unwrap().unwrap();

        // Exactly one should return true (inserted), one should return false (existed)
        let success_count = [result1, result2].iter().filter(|&&x| x).count();
        assert_eq!(
            success_count, 1,
            "Exactly one insert_if_absent should succeed, got {} successes",
            success_count
        );

        // Verify the key exists
        assert!(storage.exists(key).unwrap());

        // Verify exactly one value is stored (not corrupted or missing)
        let value = storage.get_raw(key).unwrap().unwrap();
        assert!(
            value == b"thread1" || value == b"thread2",
            "Value should be from one of the threads"
        );
    }
}
