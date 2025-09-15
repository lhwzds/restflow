use anyhow::Result;
use redb::{Database, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const CONFIG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("system_config");

// KISS: Default configuration constants
const DEFAULT_WORKER_COUNT: usize = 4;
const DEFAULT_TASK_TIMEOUT_SECONDS: u64 = 300;  // 5 minutes
const DEFAULT_STALL_TIMEOUT_SECONDS: u64 = 300; // 5 minutes  
const DEFAULT_MAX_RETRIES: u32 = 3;
const MIN_WORKER_COUNT: usize = 1;
const MIN_TIMEOUT_SECONDS: u64 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub worker_count: usize,
    pub task_timeout_seconds: u64,
    pub stall_timeout_seconds: u64,
    pub max_retries: u32,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            worker_count: DEFAULT_WORKER_COUNT,
            task_timeout_seconds: DEFAULT_TASK_TIMEOUT_SECONDS,
            stall_timeout_seconds: DEFAULT_STALL_TIMEOUT_SECONDS,
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }
}

impl SystemConfig {
    /// KISS: Validate configuration values
    pub fn validate(&self) -> Result<()> {
        if self.worker_count < MIN_WORKER_COUNT {
            return Err(anyhow::anyhow!(
                "Worker count must be at least {}", MIN_WORKER_COUNT
            ));
        }
        
        if self.task_timeout_seconds < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "Task timeout must be at least {} seconds", MIN_TIMEOUT_SECONDS
            ));
        }
        
        if self.stall_timeout_seconds < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "Stall timeout must be at least {} seconds", MIN_TIMEOUT_SECONDS
            ));
        }
        
        if self.max_retries == 0 {
            return Err(anyhow::anyhow!("Max retries must be at least 1"));
        }
        
        Ok(())
    }
}

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

    /// Get a specific config value
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

        // Should have default config after initialization
        let config = storage.get_config().unwrap();
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(config.worker_count, DEFAULT_WORKER_COUNT);
        assert_eq!(config.task_timeout_seconds, DEFAULT_TASK_TIMEOUT_SECONDS);
        assert_eq!(config.stall_timeout_seconds, DEFAULT_STALL_TIMEOUT_SECONDS);
        assert_eq!(config.max_retries, DEFAULT_MAX_RETRIES);
    }

    #[test]
    fn test_update_config() {
        let (storage, _temp_dir) = setup_test_storage();

        let new_config = SystemConfig {
            worker_count: 8,
            task_timeout_seconds: 600,
            stall_timeout_seconds: 600,
            max_retries: 5,
        };

        storage.update_config(new_config.clone()).unwrap();

        let retrieved = storage.get_config().unwrap().unwrap();
        assert_eq!(retrieved.worker_count, 8);
        assert_eq!(retrieved.task_timeout_seconds, 600);
        assert_eq!(retrieved.stall_timeout_seconds, 600);
        assert_eq!(retrieved.max_retries, 5);
    }

    #[test]
    fn test_config_validation() {
        let valid_config = SystemConfig {
            worker_count: 2,
            task_timeout_seconds: 30,
            stall_timeout_seconds: 30,
            max_retries: 1,
        };
        assert!(valid_config.validate().is_ok());
    }

    #[test]
    fn test_invalid_worker_count() {
        let (storage, _temp_dir) = setup_test_storage();

        let invalid_config = SystemConfig {
            worker_count: 0,  // Invalid: less than MIN_WORKER_COUNT
            task_timeout_seconds: 300,
            stall_timeout_seconds: 300,
            max_retries: 3,
        };

        let result = storage.update_config(invalid_config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Worker count must be at least"));
    }

    #[test]
    fn test_invalid_task_timeout() {
        let (storage, _temp_dir) = setup_test_storage();

        let invalid_config = SystemConfig {
            worker_count: 4,
            task_timeout_seconds: 5,  // Invalid: less than MIN_TIMEOUT_SECONDS
            stall_timeout_seconds: 300,
            max_retries: 3,
        };

        let result = storage.update_config(invalid_config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Task timeout must be at least"));
    }

    #[test]
    fn test_invalid_stall_timeout() {
        let (storage, _temp_dir) = setup_test_storage();

        let invalid_config = SystemConfig {
            worker_count: 4,
            task_timeout_seconds: 300,
            stall_timeout_seconds: 5,  // Invalid: less than MIN_TIMEOUT_SECONDS
            max_retries: 3,
        };

        let result = storage.update_config(invalid_config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Stall timeout must be at least"));
    }

    #[test]
    fn test_invalid_retries() {
        let (storage, _temp_dir) = setup_test_storage();

        let invalid_config = SystemConfig {
            worker_count: 4,
            task_timeout_seconds: 300,
            stall_timeout_seconds: 300,
            max_retries: 0,  // Invalid: must be at least 1
        };

        let result = storage.update_config(invalid_config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Max retries must be at least 1"));
    }

    #[test]
    fn test_get_worker_count() {
        let (storage, _temp_dir) = setup_test_storage();

        // Should get default worker count
        let count = storage.get_worker_count().unwrap();
        assert_eq!(count, DEFAULT_WORKER_COUNT);

        // Update and verify
        let new_config = SystemConfig {
            worker_count: 10,
            task_timeout_seconds: 300,
            stall_timeout_seconds: 300,
            max_retries: 3,
        };
        storage.update_config(new_config).unwrap();

        let count = storage.get_worker_count().unwrap();
        assert_eq!(count, 10);
    }

    #[test]
    fn test_set_worker_count() {
        let (storage, _temp_dir) = setup_test_storage();

        // Set new worker count
        storage.set_worker_count(6).unwrap();

        let config = storage.get_config().unwrap().unwrap();
        assert_eq!(config.worker_count, 6);

        // Try to set invalid count (should be clamped to minimum)
        storage.set_worker_count(0).unwrap();

        let config = storage.get_config().unwrap().unwrap();
        assert_eq!(config.worker_count, MIN_WORKER_COUNT);
    }

    #[test]
    fn test_config_persistence() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create and update config
        {
            let db = Arc::new(Database::create(&db_path).unwrap());
            let storage = ConfigStorage::new(db).unwrap();

            let new_config = SystemConfig {
                worker_count: 12,
                task_timeout_seconds: 900,
                stall_timeout_seconds: 900,
                max_retries: 10,
            };
            storage.update_config(new_config).unwrap();
        }

        // Open database again and verify config persisted
        {
            let db = Arc::new(Database::open(&db_path).unwrap());
            let storage = ConfigStorage::new(db).unwrap();

            let config = storage.get_config().unwrap().unwrap();
            assert_eq!(config.worker_count, 12);
            assert_eq!(config.task_timeout_seconds, 900);
            assert_eq!(config.stall_timeout_seconds, 900);
            assert_eq!(config.max_retries, 10);
        }
    }
}