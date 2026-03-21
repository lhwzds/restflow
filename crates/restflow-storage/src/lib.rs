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
pub mod audit;
pub mod auth_profiles;
pub mod background_agent;
pub mod channel_session_binding;
pub mod chat_session;
pub mod checkpoint;
pub mod config;
pub mod daemon_state;
pub mod deliverable;
pub mod execution_trace;
pub mod kv_store;
pub mod memory;
pub mod memory_index;
pub mod pairing;
pub mod paths;
pub mod provider_health_snapshot;
pub mod range_utils;
pub mod secrets;
pub mod security_amendment;
pub mod skill;
pub mod structured_execution_log;
pub mod telemetry_metric_sample;
pub mod terminal_session;
pub mod trigger;
pub mod vector;
pub mod work_item;

mod encryption;
mod simple_storage;
pub mod time_utils;

pub use agent::AgentStorage;
pub use auth_profiles::AuthProfileStorage;
pub use background_agent::BackgroundAgentStorage;
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
pub use deliverable::DeliverableStorage;
pub use execution_trace::ExecutionTraceStorage as AuditStorageBackend;
pub use execution_trace::ExecutionTraceStorage as ExecutionTraceStorageBackend;
pub use kv_store::KvStoreStorage;
pub use memory::{MemoryStorage, PutChunkResult};
pub use memory_index::{IndexableChunk, MemoryIndex, SearchHit};
pub use pairing::PairingStorage;
pub use provider_health_snapshot::ProviderHealthSnapshotStorage;
pub use secrets::{Secret, SecretStorage, SecretStorageConfig};
pub use security_amendment::SecurityAmendmentStorage;
pub use simple_storage::SimpleStorage;
pub use skill::SkillStorage;
pub use structured_execution_log::StructuredExecutionLogStorage;
pub use telemetry_metric_sample::TelemetryMetricSampleStorage;
pub use terminal_session::TerminalSessionStorage;
pub use trigger::TriggerStorage;
pub use vector::{VectorConfig, VectorStats, VectorStorage};
pub use work_item::WorkItemStorage;
