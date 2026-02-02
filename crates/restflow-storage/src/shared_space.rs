//! Shared space storage - byte-level API for shared space persistence.

use crate::SimpleStorage;
use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const SHARED_SPACE_TABLE: TableDefinition<'static, &'static str, &'static [u8]> =
    TableDefinition::new("shared_space");

#[derive(Debug, Clone)]
pub struct SharedSpaceStorage {
    db: Arc<Database>,
}

impl SharedSpaceStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(SHARED_SPACE_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    pub fn put_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        <Self as SimpleStorage>::put_raw(self, id, data)
    }

    pub fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        <Self as SimpleStorage>::get_raw(self, id)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        <Self as SimpleStorage>::delete(self, id)
    }

    pub fn exists(&self, id: &str) -> Result<bool> {
        <Self as SimpleStorage>::exists(self, id)
    }

    pub fn count(&self) -> Result<usize> {
        <Self as SimpleStorage>::count(self)
    }

    /// List all entries (key + raw data) with optional prefix filtering.
    pub fn list_raw(&self, prefix: Option<&str>) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SHARED_SPACE_TABLE)?;
        let mut entries = Vec::new();

        for entry in table.iter()? {
            let (key, value) = entry?;
            let key_str = key.value();
            if prefix.is_none() || key_str.starts_with(prefix.unwrap()) {
                entries.push((key_str.to_string(), value.value().to_vec()));
            }
        }

        Ok(entries)
    }

    /// List all keys with optional prefix filtering.
    pub fn list_keys(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SHARED_SPACE_TABLE)?;
        let mut keys = Vec::new();

        for entry in table.iter()? {
            let (key, _) = entry?;
            let key_str = key.value();
            if prefix.is_none() || key_str.starts_with(prefix.unwrap()) {
                keys.push(key_str.to_string());
            }
        }

        Ok(keys)
    }
}

impl SimpleStorage for SharedSpaceStorage {
    const TABLE: TableDefinition<'static, &'static str, &'static [u8]> = SHARED_SPACE_TABLE;

    fn db(&self) -> &Arc<Database> {
        &self.db
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

        let spaces = storage.list_raw(None).unwrap();
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
