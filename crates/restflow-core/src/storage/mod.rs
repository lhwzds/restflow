//! Storage layer with typed wrappers around restflow-storage.
//!
//! This module provides type-safe access to the storage layer by wrapping
//! the byte-level APIs from restflow-storage with Rust types from our models.

pub mod agent;
pub mod channel_session_binding;
pub mod chat_session;
pub mod checkpoint;
pub mod execution_trace;
pub mod memory;
pub mod session;
pub mod task_runtime;
pub mod terminal_session;

use anyhow::Result;
use redb::{Database, TableHandle};
use restflow_storage::MemoryIndex;
use std::path::Path;
use std::sync::Arc;

// Re-export types that are self-contained in restflow-storage
pub use restflow_storage::{
    AgentDefaults, AgentSettings, ApiDefaults, ApiSettings, ChannelDefaults, ChannelSettings,
    CliConfig, ConfigStorage, DaemonStateStorage, PairingStorage, RegistryDefaults,
    RegistrySettings, RuntimeDefaults, RuntimeSettings, Secret, SecretStorage, SecretStorageConfig,
    SystemConfig,
};

pub use agent::AgentStorage;
pub use channel_session_binding::ChannelSessionBindingStorage;
pub use chat_session::ChatSessionStorage;
pub use checkpoint::CheckpointStorage;
pub use execution_trace::ExecutionTraceStorage;
pub use memory::MemoryStorage;
pub use session::SessionStorage;
pub use task_runtime::TaskStorage;
pub use terminal_session::TerminalSessionStorage;

/// Central storage manager that initializes all storage subsystems.
///
/// Provides typed access to all storage components through wrapper types
/// that convert between Rust models and byte-level storage.
pub struct Storage {
    db: Arc<Database>,
    pub config: ConfigStorage,
    pub agents: AgentStorage,
    pub tasks: TaskStorage,
    pub secrets: SecretStorage,
    pub daemon_state: DaemonStateStorage,
    pub terminal_sessions: TerminalSessionStorage,
    pub memory: MemoryStorage,
    pub chat_sessions: ChatSessionStorage,
    pub channel_session_bindings: ChannelSessionBindingStorage,
    pub sessions: SessionStorage,
    pub checkpoints: CheckpointStorage,
    pub pairing: PairingStorage,
    /// Primary execution trace storage.
    pub execution_traces: ExecutionTraceStorage,
}

impl Storage {
    /// Create a new storage instance at the given path.
    pub fn new(path: &str) -> Result<Self> {
        let secret_config = SecretStorageConfig::default();
        Self::with_secret_config(path, secret_config)
    }

    /// Create a new storage instance with custom secret storage configuration.
    pub fn with_secret_config(path: &str, secret_config: SecretStorageConfig) -> Result<Self> {
        let db = Arc::new(Database::create(path)?);
        purge_non_secret_redb_tables(&db)?;

        let config = ConfigStorage::new(db.clone())?;
        let agents = AgentStorage::new(db.clone())?;
        let tasks = TaskStorage::new(db.clone())?;
        let secrets = SecretStorage::with_config(db.clone(), secret_config)?;
        let daemon_state = DaemonStateStorage::new(db.clone())?;
        let terminal_sessions = TerminalSessionStorage::new(db.clone())?;
        let index = if path == ":memory:" {
            Some(Arc::new(MemoryIndex::in_memory()?))
        } else {
            let db_path = Path::new(path);
            let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
            let stem = db_path
                .file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or("restflow");
            let index_path = parent.join(format!("{stem}.memory-index"));
            Some(Arc::new(MemoryIndex::open(&index_path)?))
        };
        let memory = MemoryStorage::with_index(db.clone(), index)?;
        memory.rebuild_text_index_if_empty()?;
        let chat_sessions = ChatSessionStorage::new(db.clone())?;
        let channel_session_bindings = ChannelSessionBindingStorage::new(db.clone())?;
        let sessions = SessionStorage::new(
            chat_sessions.clone(),
            channel_session_bindings.clone(),
            ExecutionTraceStorage::new(db.clone())?,
        );
        let checkpoints = CheckpointStorage::new(db.clone())?;
        let pairing = PairingStorage::new(db.clone())?;
        let execution_traces = ExecutionTraceStorage::new(db.clone())?;

        Ok(Self {
            db,
            config,
            agents,
            tasks,
            secrets,
            daemon_state,
            terminal_sessions,
            memory,
            chat_sessions,
            channel_session_bindings,
            sessions,
            checkpoints,
            pairing,
            execution_traces,
        })
    }

    /// Get a reference to the underlying database.
    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}

fn purge_non_secret_redb_tables(db: &Arc<Database>) -> Result<()> {
    const SECRETS_TABLE: &str = "secrets";

    let write_txn = db.begin_write()?;
    let normal_tables = write_txn.list_tables()?.collect::<Vec<_>>();
    for table in normal_tables {
        if table.name() != SECRETS_TABLE {
            write_txn.delete_table(table)?;
        }
    }

    let multimap_tables = write_txn.list_multimap_tables()?.collect::<Vec<_>>();
    for table in multimap_tables {
        write_txn.delete_multimap_table(table)?;
    }

    write_txn.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::ReadableDatabase;

    #[test]
    fn fresh_storage_initializes_only_secret_table() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("restflow.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let read_txn = storage.get_db().begin_read().unwrap();
        let tables = read_txn
            .list_tables()
            .unwrap()
            .map(|table| table.name().to_string())
            .collect::<Vec<_>>();

        assert_eq!(tables, vec!["secrets".to_string()]);
    }
}
