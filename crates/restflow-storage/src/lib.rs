//! RestFlow Storage - Low-level storage abstraction layer
//!
//! This crate provides the persistence layer for RestFlow, using redb as the
//! embedded database. It exposes byte-level APIs to avoid circular dependencies
//! with the workflow crate's models.
//!
//! # Architecture
//!
//! The storage layer uses a simple key-value design with separate tables for
//! different entity types. Higher-level type wrappers are provided by the
//! restflow-workflow crate.
//!
//! # Tables
//!
//! - `workflows` - Workflow definitions
//! - `skills` - Skill templates
//! - `secrets` - Encrypted secrets
//! - `agents` - Agent configurations
//! - `active_triggers` - Active trigger state
//! - `execution_history:data/index` - Execution history
//! - `system_config` - System configuration

pub mod agent;
pub mod agent_task;
pub mod chat_session;
pub mod config;
pub mod execution_history;
pub mod memory;
pub mod secrets;
pub mod shared_space;
pub mod skill;
pub mod terminal_session;
pub mod trigger;
pub mod workflow;
pub mod vector;

mod encryption;

use anyhow::Result;
use redb::Database;
use std::sync::Arc;

pub use agent::AgentStorage;
pub use agent_task::AgentTaskStorage;
pub use chat_session::ChatSessionStorage;
pub use config::{ConfigStorage, SystemConfig};
pub use execution_history::{
    ExecutionHistoryPage, ExecutionHistoryStorage, ExecutionStatus, ExecutionSummary,
};
pub use memory::MemoryStorage;
pub use secrets::{Secret, SecretStorage};
pub use shared_space::SharedSpaceStorage;
pub use skill::SkillStorage;
pub use terminal_session::TerminalSessionStorage;
pub use trigger::TriggerStorage;
pub use workflow::WorkflowStorage;
pub use vector::{VectorConfig, VectorStorage};

/// Central storage manager that initializes all storage subsystems
pub struct Storage {
    db: Arc<Database>,
    pub workflows: WorkflowStorage,
    pub config: ConfigStorage,
    pub triggers: TriggerStorage,
    pub agents: AgentStorage,
    pub agent_tasks: AgentTaskStorage,
    pub secrets: SecretStorage,
    pub skills: SkillStorage,
    pub terminal_sessions: TerminalSessionStorage,
    pub execution_history: ExecutionHistoryStorage,
    pub memory: MemoryStorage,
    pub chat_sessions: ChatSessionStorage,
}

impl Storage {
    /// Create a new storage instance at the given path.
    ///
    /// This will create the database file if it doesn't exist and initialize
    /// all required tables.
    pub fn new(path: &str) -> Result<Self> {
        let db = Arc::new(Database::create(path)?);

        let workflows = WorkflowStorage::new(db.clone())?;
        let config = ConfigStorage::new(db.clone())?;
        let triggers = TriggerStorage::new(db.clone())?;
        let agents = AgentStorage::new(db.clone())?;
        let agent_tasks = AgentTaskStorage::new(db.clone())?;
        let secrets = SecretStorage::new(db.clone())?;
        let skills = SkillStorage::new(db.clone())?;
        let terminal_sessions = TerminalSessionStorage::new(db.clone())?;
        let execution_history = ExecutionHistoryStorage::new(db.clone())?;
        let memory = MemoryStorage::new(db.clone())?;
        let chat_sessions = ChatSessionStorage::new(db.clone())?;

        Ok(Self {
            db,
            workflows,
            config,
            triggers,
            agents,
            agent_tasks,
            secrets,
            skills,
            terminal_sessions,
            execution_history,
            memory,
            chat_sessions,
        })
    }

    /// Get a reference to the underlying database
    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}
