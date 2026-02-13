//! RestFlow Storage - Low-level storage abstraction layer
//!
//! This crate provides the persistence layer for RestFlow, using redb as the
//! embedded database. It exposes byte-level APIs for different entity types.
//!
//! # Architecture
//!
//! The storage layer uses a simple key-value design with separate tables for
//! different entity types. Higher-level type wrappers are provided by
//! restflow-core.

pub mod agent;
pub mod auth_profiles;
pub mod background_agent;
pub mod chat_session;
pub mod checkpoint;
pub mod config;
pub mod daemon_state;
pub mod keychain;
pub mod memory;
pub mod memory_index;
pub mod pairing;
mod paths;
pub mod range_utils;
pub mod secrets;
pub mod shared_space;
pub mod skill;
pub mod terminal_session;
pub mod trigger;
pub mod vector;
pub mod workspace_note;

mod encryption;
mod simple_storage;
pub mod time_utils;

use anyhow::Result;
use redb::Database;
use std::sync::Arc;

pub use agent::AgentStorage;
pub use auth_profiles::AuthProfileStorage;
pub use background_agent::BackgroundAgentStorage;
pub use chat_session::ChatSessionStorage;
pub use checkpoint::CheckpointStorage;
pub use config::{ConfigStorage, SystemConfig};
pub use daemon_state::DaemonStateStorage;
pub use memory::{MemoryStorage, PutChunkResult};
pub use memory_index::{IndexableChunk, MemoryIndex, SearchHit};
pub use pairing::PairingStorage;
pub use secrets::{Secret, SecretStorage, SecretStorageConfig};
pub use shared_space::SharedSpaceStorage;
pub use simple_storage::SimpleStorage;
pub use skill::SkillStorage;
pub use terminal_session::TerminalSessionStorage;
pub use trigger::TriggerStorage;
pub use vector::{VectorConfig, VectorStorage};
pub use workspace_note::WorkspaceNoteStorage;
/// Central storage manager that initializes all storage subsystems
pub struct Storage {
    db: Arc<Database>,
    pub config: ConfigStorage,
    pub triggers: TriggerStorage,
    pub agents: AgentStorage,
    pub background_agents: BackgroundAgentStorage,
    pub secrets: SecretStorage,
    pub daemon_state: DaemonStateStorage,
    pub skills: SkillStorage,
    pub terminal_sessions: TerminalSessionStorage,
    pub memory: MemoryStorage,
    pub chat_sessions: ChatSessionStorage,
    pub workspace_notes: WorkspaceNoteStorage,
    pub checkpoints: CheckpointStorage,
    pub pairing: PairingStorage,
}

impl Storage {
    /// Create a new storage instance at the given path.
    ///
    /// This will create the database file if it doesn't exist and initialize
    /// all required tables.
    pub fn new(path: &str) -> Result<Self> {
        let db = Arc::new(Database::create(path)?);

        let config = ConfigStorage::new(db.clone())?;
        let triggers = TriggerStorage::new(db.clone())?;
        let agents = AgentStorage::new(db.clone())?;
        let background_agents = BackgroundAgentStorage::new(db.clone())?;
        let secrets = SecretStorage::new(db.clone())?;
        let daemon_state = DaemonStateStorage::new(db.clone())?;
        let skills = SkillStorage::new(db.clone())?;
        let terminal_sessions = TerminalSessionStorage::new(db.clone())?;
        let memory = MemoryStorage::new(db.clone())?;
        let chat_sessions = ChatSessionStorage::new(db.clone())?;
        let workspace_notes = WorkspaceNoteStorage::new(db.clone())?;
        let checkpoints = CheckpointStorage::new(db.clone())?;
        let pairing = PairingStorage::new(db.clone())?;

        Ok(Self {
            db,
            config,
            triggers,
            agents,
            background_agents,
            secrets,
            daemon_state,
            skills,
            terminal_sessions,
            memory,
            chat_sessions,
            workspace_notes,
            checkpoints,
            pairing,
        })
    }

    /// Get a reference to the underlying database
    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}
