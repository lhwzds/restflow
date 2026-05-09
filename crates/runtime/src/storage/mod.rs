//! Storage layer with typed wrappers around RestFlow persistence.
//!
//! This module provides type-safe access to the storage layer by wrapping
//! lower-level persistence APIs with Rust types from our models.

pub mod agent;
pub mod chat_session;
pub mod redb_lease;
pub mod simple_storage;
pub mod task_runtime;
pub mod terminal_session;

use anyhow::Result;
use std::path::{Path, PathBuf};

pub use crate::{
    AgentDefaults, AgentSettings, ApiDefaults, ApiSettings, CliConfig, ConfigDocument,
    ConfigSourcePathInfo, ConfigStorage, RegistryDefaults, RegistrySettings, RuntimeDefaults,
    RuntimeSettings, Secret, SecretStorage, SecretStorageConfig, SystemConfig,
    effective_config_sources, load_cli_config, load_global_cli_config, write_cli_config,
};

pub use agent::AgentStorage;
pub use chat_session::ChatSessionStorage;
pub use redb_lease::RedbLeaseProvider;
pub use simple_storage::{AuthProfileRawStorage as AuthProfileStorage, SimpleStorage};
pub use task_runtime::TaskStorage;
pub use terminal_session::TerminalSessionStorage;

/// Central storage manager that initializes all storage subsystems.
///
/// Provides typed access to all storage components through wrapper types
/// that convert between Rust models and byte-level storage.
pub struct Storage {
    #[cfg(test)]
    db_path: PathBuf,
    namespace: usize,
    pub config: ConfigStorage,
    pub agents: AgentStorage,
    pub tasks: TaskStorage,
    pub secrets: SecretStorage,
    pub terminal_sessions: TerminalSessionStorage,
    pub chat_sessions: ChatSessionStorage,
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
        let namespace = simple_storage::namespace_for_path(&db_path);

        let config = ConfigStorage;
        let agents = AgentStorage::new_namespace(namespace)?;
        let tasks = TaskStorage::new_file_backed_namespace(namespace, task_store_path(path))?;
        let secrets = SecretStorage::with_config_path(db_path.clone(), secret_config)?;
        let terminal_sessions = TerminalSessionStorage::new_namespace(namespace)?;
        let chat_sessions = ChatSessionStorage::new_namespace(namespace)?;

        Ok(Self {
            #[cfg(test)]
            db_path,
            namespace,
            config,
            agents,
            tasks,
            secrets,
            terminal_sessions,
            chat_sessions,
        })
    }

    pub fn namespace(&self) -> usize {
        self.namespace
    }

    #[cfg(test)]
    pub fn get_db(&self) -> std::sync::Arc<redb::Database> {
        std::sync::Arc::new(redb::Database::create(&self.db_path).unwrap())
    }
}

fn task_store_path(db_path: &str) -> PathBuf {
    Path::new(db_path).with_file_name("restflow.tasks.json")
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
