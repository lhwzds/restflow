//! Shared space storage - global key-value store for AI agents.

use crate::{define_simple_storage, SimpleStorage};
use anyhow::Result;
use redb::{ReadableDatabase, ReadableTable};

// TODO: Consider adding prefix-aware helper methods to SimpleStorage if more modules need it.

define_simple_storage! {
    /// Shared space storage with byte-level API.
    pub struct SharedSpaceStorage { table: "shared_space" }
}

impl SharedSpaceStorage {
    /// List all keys with optional prefix filter.
    pub fn list_keys(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let read_txn = self.db().begin_read()?;
        let table = read_txn.open_table(<Self as SimpleStorage>::TABLE)?;
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

    /// List all entries (key + raw data) with optional prefix filter.
    pub fn list_raw(&self, prefix: Option<&str>) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db().begin_read()?;
        let table = read_txn.open_table(<Self as SimpleStorage>::TABLE)?;
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
