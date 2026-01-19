//! Skill storage - byte-level API for skill persistence.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const SKILLS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("skills");

/// Low-level skill storage with byte-level API
#[derive(Debug, Clone)]
pub struct SkillStorage {
    db: Arc<Database>,
}

impl SkillStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(SKILLS_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Store raw skill data
    pub fn put_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SKILLS_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw skill data by ID
    pub fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SKILLS_TABLE)?;

        if let Some(data) = table.get(id)? {
            Ok(Some(data.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all raw skill data
    pub fn list_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SKILLS_TABLE)?;

        let mut skills = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            skills.push((key.value().to_string(), value.value().to_vec()));
        }

        Ok(skills)
    }

    /// Check if skill exists
    pub fn exists(&self, id: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SKILLS_TABLE)?;
        Ok(table.get(id)?.is_some())
    }

    /// Delete skill by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(SKILLS_TABLE)?;
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
}
