//! KV store storage - global key-value store for AI agents.

use crate::{SimpleStorage, define_simple_storage};
use anyhow::Result;
use redb::{ReadableDatabase, ReadableTable};

define_simple_storage! {
    /// KV store storage with byte-level API.
    pub struct KvStoreStorage { table: "kv_store" }
}

impl KvStoreStorage {
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
