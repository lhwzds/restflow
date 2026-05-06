//! RestFlow Storage - Low-level storage abstraction layer
//!
//! This crate provides the reduced storage layer for RestFlow. Secrets use the
//! only remaining redb table; MVP runtime stores are process-local byte stores
//! until they are promoted to explicit file-backed formats.
//!
//! # Architecture
//!
//! Higher-level typed wrappers are provided by `restflow-core`. New durable
//! product data should prefer explicit files such as session JSONL or agent
//! Markdown, not additional redb tables.

pub mod agent;
pub mod auth_profiles;
pub mod channel_session_binding;
pub mod chat_session;
pub mod checkpoint;
pub mod config;
pub mod daemon_state;
pub mod execution_trace;
pub mod memory;
pub mod memory_index;
pub mod pairing;
pub mod paths;
pub mod range_utils;
pub mod secrets;
pub mod task;
pub mod terminal_session;
pub mod vector;

mod encryption;
mod simple_storage;
pub mod time_utils;

pub use agent::AgentStorage;
pub use auth_profiles::AuthProfileStorage;
pub use channel_session_binding::ChannelSessionBindingStorage;
pub use chat_session::ChatSessionStorage;
pub use checkpoint::CheckpointStorage;
pub use config::{
    AgentDefaults, AgentSettings, ApiDefaults, ApiSettings, ChannelDefaults, ChannelSettings,
    CliConfig, ConfigDocument, ConfigSourcePathInfo, ConfigStorage, ConfigValueSourceInfo,
    ConfigValueSourceKind, EffectiveConfigSources, RegistryDefaults, RegistrySettings,
    RuntimeDefaults, RuntimeSettings, SystemConfig, SystemSection, effective_config_sources,
    load_cli_config, load_global_cli_config, write_cli_config,
};
pub use daemon_state::DaemonStateStorage;
pub use execution_trace::ExecutionTraceStorage as ExecutionTraceStorageBackend;
pub use memory::{MemoryStorage, PutChunkResult};
pub use memory_index::{IndexableChunk, MemoryIndex, SearchHit};
pub use pairing::PairingStorage;
pub use secrets::{Secret, SecretStorage, SecretStorageConfig};
pub use simple_storage::SimpleStorage;
pub use task::TaskStorage;
pub use terminal_session::TerminalSessionStorage;
pub use vector::{VectorConfig, VectorStats, VectorStorage};
