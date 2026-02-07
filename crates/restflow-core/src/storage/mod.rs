//! Storage layer with typed wrappers around restflow-storage.
//!
//! This module provides type-safe access to the storage layer by wrapping
//! the byte-level APIs from restflow-storage with Rust types from our models.

pub mod agent;
pub mod agent_task;
pub mod chat_session;
pub mod execution_history;
pub mod hook;
pub mod memory;
pub mod shared_space;
pub mod skill;
pub mod terminal_session;
pub mod trigger;

use anyhow::Result;
use redb::Database;
use std::sync::Arc;

// Re-export types that are self-contained in restflow-storage
pub use restflow_storage::{
    ConfigStorage, Secret, SecretStorage, SecretStorageConfig, SystemConfig,
};

pub use agent::AgentStorage;
pub use agent_task::AgentTaskStorage;
pub use chat_session::ChatSessionStorage;
pub use execution_history::ExecutionHistoryStorage;
pub use hook::HookStorage;
pub use memory::MemoryStorage;
pub use shared_space::SharedSpaceStorage;
pub use skill::SkillStorage;
pub use terminal_session::TerminalSessionStorage;
pub use trigger::TriggerStorage;

/// Central storage manager that initializes all storage subsystems.
///
/// Provides typed access to all storage components through wrapper types
/// that convert between Rust models and byte-level storage.
pub struct Storage {
    db: Arc<Database>,
    pub config: ConfigStorage,
    pub triggers: TriggerStorage,
    pub agents: AgentStorage,
    pub agent_tasks: AgentTaskStorage,
    pub secrets: SecretStorage,
    pub skills: SkillStorage,
    pub shared_space: SharedSpaceStorage,
    pub terminal_sessions: TerminalSessionStorage,
    pub execution_history: ExecutionHistoryStorage,
    pub memory: MemoryStorage,
    pub chat_sessions: ChatSessionStorage,
    pub hooks: HookStorage,
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

        let config = ConfigStorage::new(db.clone())?;
        let triggers = TriggerStorage::new(db.clone())?;
        let agents = AgentStorage::new(db.clone())?;
        let agent_tasks = AgentTaskStorage::new(db.clone())?;
        let secrets = SecretStorage::with_config(db.clone(), secret_config)?;
        let skills = SkillStorage::new(db.clone())?;
        let shared_space_raw = restflow_storage::SharedSpaceStorage::new(db.clone())?;
        let shared_space = SharedSpaceStorage::new(shared_space_raw);
        let terminal_sessions = TerminalSessionStorage::new(db.clone())?;
        let execution_history = ExecutionHistoryStorage::new(db.clone())?;
        let memory = MemoryStorage::new(db.clone())?;
        let chat_sessions = ChatSessionStorage::new(db.clone())?;
        let hooks = HookStorage::new(db.clone())?;

        Ok(Self {
            db,
            config,
            triggers,
            agents,
            agent_tasks,
            secrets,
            skills,
            shared_space,
            terminal_sessions,
            execution_history,
            memory,
            chat_sessions,
            hooks,
        })
    }

    /// Get a reference to the underlying database
    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}
