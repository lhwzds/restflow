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
//! - `pending/processing/completed` - Task queue tables
//! - `execution_history:data/index` - Execution history
//! - `system_config` - System configuration

pub mod agent;
pub mod config;
pub mod execution_history;
pub mod secrets;
pub mod skill;
pub mod task_queue;
pub mod terminal_session;
pub mod trigger;
pub mod workflow;

use anyhow::Result;
use redb::Database;
use std::sync::Arc;

pub use agent::AgentStorage;
pub use config::{ConfigStorage, SystemConfig};
pub use execution_history::{
    ExecutionHistoryPage, ExecutionHistoryStorage, ExecutionStatus, ExecutionSummary,
};
pub use secrets::{Secret, SecretStorage};
pub use skill::SkillStorage;
pub use task_queue::TaskQueue;
pub use terminal_session::TerminalSessionStorage;
pub use trigger::TriggerStorage;
pub use workflow::WorkflowStorage;

/// Central storage manager that initializes all storage subsystems
pub struct Storage {
    db: Arc<Database>,
    pub workflows: WorkflowStorage,
    pub queue: TaskQueue,
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
    ///
    /// This will create the database file if it doesn't exist and initialize
    /// all required tables.
    pub fn new(path: &str) -> Result<Self> {
        let db = Arc::new(Database::create(path)?);

        let workflows = WorkflowStorage::new(db.clone())?;
        let queue = TaskQueue::new(db.clone())?;
        let config = ConfigStorage::new(db.clone())?;
        let triggers = TriggerStorage::new(db.clone())?;
        let agents = AgentStorage::new(db.clone())?;
        let secrets = SecretStorage::new(db.clone())?;
        let skills = SkillStorage::new(db.clone())?;
        let terminal_sessions = TerminalSessionStorage::new(db.clone())?;
        let execution_history = ExecutionHistoryStorage::new(db.clone())?;

        Ok(Self {
            db,
            workflows,
            queue,
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
