//! Daemon runtime state persistence.
//!
//! Stores lightweight runtime state for daemon components such as
//! Telegram polling offsets.

use crate::{SimpleStorage, define_simple_storage};
use anyhow::Result;

define_simple_storage! {
    /// Daemon runtime key-value state.
    pub struct DaemonStateStorage { table: "daemon_state" }
}

impl DaemonStateStorage {
    /// Persist an i64 value under the provided key.
    pub fn set_i64(&self, key: &str, value: i64) -> Result<()> {
        self.put_raw(key, &value.to_le_bytes())
    }

    /// Read an i64 value for key. Returns 0 when key is absent or malformed.
    pub fn get_i64(&self, key: &str) -> Result<i64> {
        match self.get_raw(key)? {
            Some(bytes) if bytes.len() == 8 => {
                let mut arr = [0_u8; 8];
                arr.copy_from_slice(&bytes);
                Ok(i64::from_le_bytes(arr))
            }
            _ => Ok(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_set_and_get_i64() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("daemon_state.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = DaemonStateStorage::new(db).unwrap();

        storage.set_i64("telegram_last_update_id", 42).unwrap();

        let value = storage.get_i64("telegram_last_update_id").unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_get_i64_missing_key_returns_zero() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("daemon_state_missing.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = DaemonStateStorage::new(db).unwrap();

        let value = storage.get_i64("missing").unwrap();
        assert_eq!(value, 0);
    }
}
