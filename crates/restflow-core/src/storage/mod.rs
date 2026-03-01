//! Storage layer with typed wrappers around restflow-storage.
//!
//! This module provides type-safe access to the storage layer by wrapping
//! the byte-level APIs from restflow-storage with Rust types from our models.

pub mod agent;
pub mod audit;
pub mod background_agent;
pub mod channel_session_binding;
pub mod chat_session;
pub mod checkpoint;
pub mod deliverable;
pub mod hook;
pub mod kv_store;
pub mod memory;
pub mod skill;
pub mod terminal_session;
pub mod tool_trace;
pub mod trigger;
pub mod work_item;

use anyhow::Result;
use redb::Database;
use restflow_storage::MemoryIndex;
use std::path::Path;
use std::sync::Arc;

use crate::models::{ChannelSessionBinding, ChatSessionSource};

// Re-export types that are self-contained in restflow-storage
pub use restflow_storage::{
    ConfigStorage, DaemonStateStorage, PairingStorage, Secret, SecretStorage, SecretStorageConfig,
    SystemConfig,
};

pub use agent::AgentStorage;
pub use audit::AuditStorage;
pub use background_agent::BackgroundAgentStorage;
pub use channel_session_binding::ChannelSessionBindingStorage;
pub use chat_session::ChatSessionStorage;
pub use checkpoint::CheckpointStorage;
pub use deliverable::DeliverableStorage;
pub use hook::HookStorage;
pub use kv_store::KvStoreStorage;
pub use memory::MemoryStorage;
pub use skill::SkillStorage;
pub use terminal_session::TerminalSessionStorage;
pub use tool_trace::ToolTraceStorage;
pub use trigger::TriggerStorage;
pub use work_item::WorkItemStorage;

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
    pub kv_store: KvStoreStorage,
    pub terminal_sessions: TerminalSessionStorage,
    pub memory: MemoryStorage,
    pub chat_sessions: ChatSessionStorage,
    pub channel_session_bindings: ChannelSessionBindingStorage,
    pub tool_traces: ToolTraceStorage,
    pub deliverables: DeliverableStorage,
    pub hooks: HookStorage,
    pub work_items: WorkItemStorage,
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
        let kv_store_raw = restflow_storage::KvStoreStorage::new(db.clone())?;
        let kv_store = KvStoreStorage::new(kv_store_raw);
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
        backfill_channel_session_bindings_from_legacy_sources(
            &chat_sessions,
            &channel_session_bindings,
        )?;
        let tool_traces = ToolTraceStorage::new(db.clone())?;
        let deliverables = DeliverableStorage::new(db.clone())?;
        let hooks = HookStorage::new(db.clone())?;
        let work_items = WorkItemStorage::new(db.clone())?;
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
            kv_store,
            terminal_sessions,
            memory,
            chat_sessions,
            channel_session_bindings,
            tool_traces,
            deliverables,
            hooks,
            work_items,
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

fn backfill_channel_session_bindings_from_legacy_sources(
    chat_sessions: &ChatSessionStorage,
    channel_session_bindings: &ChannelSessionBindingStorage,
) -> Result<usize> {
    let sessions = chat_sessions.list()?;
    let mut created = 0usize;

    for session in sessions {
        let channel_key = match session.source_channel {
            Some(ChatSessionSource::Telegram) => Some("telegram"),
            Some(ChatSessionSource::Discord) => Some("discord"),
            Some(ChatSessionSource::Slack) => Some("slack"),
            Some(ChatSessionSource::Workspace) | Some(ChatSessionSource::ExternalLegacy) | None => {
                None
            }
        };
        let Some(channel_key) = channel_key else {
            continue;
        };

        let Some(conversation_id) = session
            .source_conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        if channel_session_bindings
            .get_by_route(channel_key, None, conversation_id)?
            .is_some()
        {
            continue;
        }

        let binding =
            ChannelSessionBinding::new(channel_key, None, conversation_id.to_string(), session.id);
        channel_session_bindings.upsert(&binding)?;
        created += 1;
    }

    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ChatSession;
    use tempfile::tempdir;

    #[test]
    fn backfill_legacy_channel_session_bindings_is_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("storage-backfill.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();

        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Telegram, "chat-backfill");
        storage.chat_sessions.create(&session).unwrap();

        let created = backfill_channel_session_bindings_from_legacy_sources(
            &storage.chat_sessions,
            &storage.channel_session_bindings,
        )
        .unwrap();
        assert_eq!(created, 1);

        let binding = storage
            .channel_session_bindings
            .get_by_route("telegram", None, "chat-backfill")
            .unwrap()
            .expect("binding should be created");
        assert_eq!(binding.session_id, session.id);

        let created_again = backfill_channel_session_bindings_from_legacy_sources(
            &storage.chat_sessions,
            &storage.channel_session_bindings,
        )
        .unwrap();
        assert_eq!(created_again, 0);
    }
}
