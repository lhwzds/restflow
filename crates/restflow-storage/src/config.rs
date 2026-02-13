//! System configuration storage.

use anyhow::Result;
use redb::{Database, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const CONFIG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("system_config");

// Default configuration constants
const DEFAULT_WORKER_COUNT: usize = 4;
const DEFAULT_TASK_TIMEOUT_SECONDS: u64 = 300; // 5 minutes
const DEFAULT_STALL_TIMEOUT_SECONDS: u64 = 300; // 5 minutes
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_CHAT_SESSION_RETENTION_DAYS: u32 = 30;
const DEFAULT_BACKGROUND_TASK_RETENTION_DAYS: u32 = 7;
const DEFAULT_CHECKPOINT_RETENTION_DAYS: u32 = 3;
const DEFAULT_MEMORY_CHUNK_RETENTION_DAYS: u32 = 90;
const MIN_RETENTION_DAYS: u32 = 1;
const MIN_WORKER_COUNT: usize = 1;
const MIN_TIMEOUT_SECONDS: u64 = 10;

/// System configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemConfig {
    pub worker_count: usize,
    pub task_timeout_seconds: u64,
    pub stall_timeout_seconds: u64,
    pub max_retries: u32,
    pub chat_session_retention_days: u32,
    pub background_task_retention_days: u32,
    pub checkpoint_retention_days: u32,
    pub memory_chunk_retention_days: u32,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            worker_count: DEFAULT_WORKER_COUNT,
            task_timeout_seconds: DEFAULT_TASK_TIMEOUT_SECONDS,
            stall_timeout_seconds: DEFAULT_STALL_TIMEOUT_SECONDS,
            max_retries: DEFAULT_MAX_RETRIES,
            chat_session_retention_days: DEFAULT_CHAT_SESSION_RETENTION_DAYS,
            background_task_retention_days: DEFAULT_BACKGROUND_TASK_RETENTION_DAYS,
            checkpoint_retention_days: DEFAULT_CHECKPOINT_RETENTION_DAYS,
            memory_chunk_retention_days: DEFAULT_MEMORY_CHUNK_RETENTION_DAYS,
        }
    }
}

impl SystemConfig {
    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        if self.worker_count < MIN_WORKER_COUNT {
            return Err(anyhow::anyhow!(
                "Worker count must be at least {}",
                MIN_WORKER_COUNT
            ));
        }

        if self.task_timeout_seconds < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "Task timeout must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }

        if self.stall_timeout_seconds < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "Stall timeout must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }

        if self.max_retries == 0 {
            return Err(anyhow::anyhow!("Max retries must be at least 1"));
        }

        if self.chat_session_retention_days != 0
            && self.chat_session_retention_days < MIN_RETENTION_DAYS
        {
            return Err(anyhow::anyhow!(
                "Chat session retention must be 0 (forever) or at least {} day",
                MIN_RETENTION_DAYS
            ));
        }

        if self.background_task_retention_days < MIN_RETENTION_DAYS {
            return Err(anyhow::anyhow!(
                "Background task retention must be at least {} day",
                MIN_RETENTION_DAYS
            ));
        }

        if self.checkpoint_retention_days < MIN_RETENTION_DAYS {
            return Err(anyhow::anyhow!(
                "Checkpoint retention must be at least {} day",
                MIN_RETENTION_DAYS
            ));
        }

        if self.memory_chunk_retention_days != 0
            && self.memory_chunk_retention_days < MIN_RETENTION_DAYS
        {
            return Err(anyhow::anyhow!(
                "Memory chunk retention must be 0 (forever) or at least {} day",
                MIN_RETENTION_DAYS
            ));
        }

        Ok(())
    }
}

/// Configuration storage
#[derive(Clone)]
pub struct ConfigStorage {
    db: Arc<Database>,
}

impl ConfigStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Create table
        let write_txn = db.begin_write()?;
        write_txn.open_table(CONFIG_TABLE)?;
        write_txn.commit()?;

        let storage = Self { db };

        // Set default config if not exists
        if storage.get_config()?.is_none() {
            storage.update_config(SystemConfig::default())?;
        }

        Ok(storage)
    }

    /// Get system configuration
    pub fn get_config(&self) -> Result<Option<SystemConfig>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CONFIG_TABLE)?;

        if let Some(data) = table.get("system")? {
            let config: SystemConfig = serde_json::from_slice(data.value())?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Update system configuration
    pub fn update_config(&self, config: SystemConfig) -> Result<()> {
        // Validate before saving
        config.validate()?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CONFIG_TABLE)?;
            let serialized = serde_json::to_vec(&config)?;
            table.insert("system", serialized.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get worker count
    pub fn get_worker_count(&self) -> Result<usize> {
        Ok(self.get_config()?.unwrap_or_default().worker_count)
    }

    /// Update worker count
    pub fn set_worker_count(&self, count: usize) -> Result<()> {
        let mut config = self.get_config()?.unwrap_or_default();
        config.worker_count = count.max(MIN_WORKER_COUNT);
        self.update_config(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_test_storage() -> (ConfigStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ConfigStorage::new(db).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_default_config() {
        let (storage, _temp_dir) = setup_test_storage();

        let config = storage.get_config().unwrap();
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(config.worker_count, DEFAULT_WORKER_COUNT);
        assert_eq!(config.task_timeout_seconds, DEFAULT_TASK_TIMEOUT_SECONDS);
    }

    #[test]
    fn test_update_config() {
        let (storage, _temp_dir) = setup_test_storage();

        let new_config = SystemConfig {
            worker_count: 8,
            task_timeout_seconds: 600,
            stall_timeout_seconds: 600,
            max_retries: 5,
            chat_session_retention_days: 45,
            background_task_retention_days: 14,
            checkpoint_retention_days: 5,
            memory_chunk_retention_days: 120,
        };

        storage.update_config(new_config).unwrap();

        let retrieved = storage.get_config().unwrap().unwrap();
        assert_eq!(retrieved.worker_count, 8);
        assert_eq!(retrieved.task_timeout_seconds, 600);
    }

    #[test]
    fn test_config_validation() {
        let valid_config = SystemConfig {
            worker_count: 2,
            task_timeout_seconds: 30,
            stall_timeout_seconds: 30,
            max_retries: 1,
            chat_session_retention_days: 30,
            background_task_retention_days: 7,
            checkpoint_retention_days: 3,
            memory_chunk_retention_days: 90,
        };
        assert!(valid_config.validate().is_ok());
    }

    #[test]
    fn test_invalid_worker_count() {
        let (storage, _temp_dir) = setup_test_storage();

        let invalid_config = SystemConfig {
            worker_count: 0,
            task_timeout_seconds: 300,
            stall_timeout_seconds: 300,
            max_retries: 3,
            chat_session_retention_days: 30,
            background_task_retention_days: 7,
            checkpoint_retention_days: 3,
            memory_chunk_retention_days: 90,
        };

        let result = storage.update_config(invalid_config);
        assert!(result.is_err());
    }
}
