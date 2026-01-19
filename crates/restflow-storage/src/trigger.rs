//! Trigger storage - byte-level API for active trigger persistence.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

pub const ACTIVE_TRIGGERS_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("active_triggers");

/// Low-level trigger storage with byte-level API
pub struct TriggerStorage {
    db: Arc<Database>,
}

impl TriggerStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Store raw trigger data
    pub fn put_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw trigger data by ID
    pub fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;

        if let Some(value) = table.get(id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all raw trigger data
    pub fn list_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;

        let mut triggers = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            triggers.push((key.value().to_string(), value.value().to_vec()));
        }

        Ok(triggers)
    }

    /// Delete trigger by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;
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
