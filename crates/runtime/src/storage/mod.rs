//! Storage aggregation for local files plus short-lived secret storage.

pub mod redb_lease;

use anyhow::Result;
use std::path::{Path, PathBuf};

pub use crate::{
    AgentDefaults, AgentSettings, ApiDefaults, ApiSettings, CliConfig, ConfigDocument,
    ConfigSourcePathInfo, ConfigStorage, RegistryDefaults, RegistrySettings, RuntimeDefaults,
    RuntimeSettings, Secret, SecretStorage, SecretStorageConfig, SystemConfig,
    effective_config_sources, load_cli_config, load_global_cli_config, write_cli_config,
};

pub use redb_lease::RedbLeaseProvider;

use crate::AgentStorage;
use crate::session_log::FileSessionStore;

/// Central storage manager for file-backed local state and secrets.
pub struct Storage {
    #[cfg(test)]
    db_path: PathBuf,
    pub config: ConfigStorage,
    pub agents: AgentStorage,
    pub secrets: SecretStorage,
    pub file_sessions: FileSessionStore,
}

impl Storage {
    /// Create a new storage instance at the given path.
    pub fn new(path: &str) -> Result<Self> {
        let secret_config = SecretStorageConfig::default();
        Self::with_secret_config(path, secret_config)
    }

    /// Create a new storage instance with custom secret storage configuration.
    pub fn with_secret_config(path: &str, secret_config: SecretStorageConfig) -> Result<Self> {
        let db_path = PathBuf::from(path);

        let config = ConfigStorage;
        let agents = AgentStorage::new_file_backed()?;
        let secrets = SecretStorage::with_config_path(db_path.clone(), secret_config)?;
        let file_sessions = FileSessionStore::new(session_store_path(path))?;

        Ok(Self {
            #[cfg(test)]
            db_path,
            config,
            agents,
            secrets,
            file_sessions,
        })
    }

    #[cfg(test)]
    pub fn get_db(&self) -> std::sync::Arc<redb::Database> {
        std::sync::Arc::new(redb::Database::create(&self.db_path).unwrap())
    }
}

fn session_store_path(db_path: &str) -> PathBuf {
    Path::new(db_path).with_file_name("sessions")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_storage_does_not_hold_redb_open_after_startup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("restflow.db");
        let _storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let _second_open = redb::Database::create(&db_path).unwrap();
    }
}
