//! Storage layer with typed wrappers around restflow-storage.
//!
//! This module provides type-safe access to the storage layer by wrapping
//! the byte-level APIs from restflow-storage with Rust types from our models.

pub mod agent;
pub mod background_agent;
pub mod chat_session;
pub mod checkpoint;
pub mod deliverable;
pub mod hook;
pub mod memory;
pub mod shared_space;
pub mod skill;
pub mod audit;
pub mod terminal_session;
pub mod trigger;
pub mod workspace_note;

use anyhow::Result;
use redb::Database;
use restflow_storage::MemoryIndex;
use std::path::Path;
use std::sync::Arc;

// Re-export types that are self-contained in restflow-storage
pub use restflow_storage::{
    ConfigStorage, DaemonStateStorage, PairingStorage, Secret, SecretStorage, SecretStorageConfig,
    SystemConfig,
};

pub use agent::AgentStorage;
pub use background_agent::BackgroundAgentStorage;
pub use chat_session::ChatSessionStorage;
pub use checkpoint::CheckpointStorage;
pub use deliverable::DeliverableStorage;
pub use hook::HookStorage;
pub use memory::MemoryStorage;
pub use shared_space::SharedSpaceStorage;
pub use skill::SkillStorage;
pub use terminal_session::TerminalSessionStorage;
pub use trigger::TriggerStorage;
pub use workspace_note::WorkspaceNoteStorage;
pub use audit::AuditStorage;

/// Central storage manager that initializes all storage subsystems.
///
/// Provides typed access to all storage components through wrapper types
/// that convert between Rust models and byte-level storage.
pub struct Storage {
    db: Arc<Database>,
    pub config: ConfigStorage,
    pub triggers: TriggerStorage,
    pub agents: AgentStorage,
    pub background_agents: BackgroundAgentStorage,
    pub secrets: SecretStorage,
    pub daemon_state: DaemonStateStorage,
    pub skills: SkillStorage,
    pub shared_space: SharedSpaceStorage,
    pub terminal_sessions: TerminalSessionStorage,
    pub memory: MemoryStorage,
    pub chat_sessions: ChatSessionStorage,
    pub deliverables: DeliverableStorage,
    pub hooks: HookStorage,
    pub workspace_notes: WorkspaceNoteStorage,
    pub checkpoints: CheckpointStorage,
    pub pairing: PairingStorage,
    pub audit: AuditStorage,
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
        let background_agents = BackgroundAgentStorage::new(db.clone())?;
        let secrets = SecretStorage::with_config(db.clone(), secret_config)?;
        let daemon_state = DaemonStateStorage::new(db.clone())?;
        let skills = SkillStorage::new(db.clone())?;
        let shared_space_raw = restflow_storage::SharedSpaceStorage::new(db.clone())?;
        let shared_space = SharedSpaceStorage::new(shared_space_raw);
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
        let deliverables = DeliverableStorage::new(db.clone())?;
        let hooks = HookStorage::new(db.clone())?;
        let workspace_notes = WorkspaceNoteStorage::new(db.clone())?;
        let checkpoints = CheckpointStorage::new(db.clone())?;
        let pairing = PairingStorage::new(db.clone())?;
        let audit = AuditStorage::new(db.clone())?;

        Ok(Self {
            db,
            config,
            triggers,
            agents,
            background_agents,
            secrets,
            daemon_state,
            skills,
            shared_space,
            terminal_sessions,
            memory,
            chat_sessions,
            deliverables,
            hooks,
            workspace_notes,
            checkpoints,
            pairing,
            audit,
        })
    }

    /// Get a reference to the underlying database
    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}
