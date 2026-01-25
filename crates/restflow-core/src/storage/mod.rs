//! Storage layer with typed wrappers around restflow-storage.
//!
//! This module provides type-safe access to the storage layer by wrapping
//! the byte-level APIs from restflow-storage with Rust types from our models.

pub mod agent;
pub mod execution_history;
pub mod skill;
pub mod terminal_session;
pub mod trigger;

use anyhow::Result;
use redb::Database;
use std::sync::Arc;

// Re-export types that are self-contained in restflow-storage
pub use restflow_storage::{ConfigStorage, Secret, SecretStorage, SystemConfig};

pub use agent::AgentStorage;
pub use execution_history::ExecutionHistoryStorage;
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
    pub secrets: SecretStorage,
    pub skills: SkillStorage,
    pub terminal_sessions: TerminalSessionStorage,
    pub execution_history: ExecutionHistoryStorage,
}

impl Storage {
    /// Create a new storage instance at the given path.
    pub fn new(path: &str) -> Result<Self> {
        let db = Arc::new(Database::create(path)?);

        let config = ConfigStorage::new(db.clone())?;
        let triggers = TriggerStorage::new(db.clone())?;
        let agents = AgentStorage::new(db.clone())?;
        let secrets = SecretStorage::new(db.clone())?;
        let skills = SkillStorage::new(db.clone())?;
        let terminal_sessions = TerminalSessionStorage::new(db.clone())?;
        let execution_history = ExecutionHistoryStorage::new(db.clone())?;

        Ok(Self {
            db,
            config,
            triggers,
            agents,
            secrets,
            skills,
            terminal_sessions,
            execution_history,
        })
    }

    /// Get a reference to the underlying database
    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}
