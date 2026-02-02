//! Shared space storage - global key-value store for AI agents.

use anyhow::Result;
use redb::{Database, ReadableTable, TableDefinition};
use std::sync::Arc;

const SHARED_SPACE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("shared_space");

#[derive(Clone)]
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

    /// Store raw bytes
    pub fn put_raw(&self, key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SHARED_SPACE_TABLE)?;
            table.insert(key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw bytes
    pub fn get_raw(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SHARED_SPACE_TABLE)?;
        Ok(table.get(key)?.map(|v| v.value().to_vec()))
    }

    /// Delete entry
    pub fn delete(&self, key: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(SHARED_SPACE_TABLE)?;
            table.remove(key)?.is_some()
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// List all keys with optional prefix filter
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

    /// List all entries (key + raw data)
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
}
