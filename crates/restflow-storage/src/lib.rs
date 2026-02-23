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
pub mod chat_session;
pub mod checkpoint;
pub mod config;
pub mod daemon_state;
pub mod deliverable;
pub mod memory;
pub mod memory_index;
pub mod pairing;
pub mod paths;
pub mod range_utils;
pub mod secrets;
pub mod security_amendment;
pub mod kv_store;
pub mod skill;
pub mod terminal_session;
pub mod trigger;
pub mod vector;
pub mod work_item;

mod encryption;
mod simple_storage;
pub mod time_utils;

pub use agent::AgentStorage;
pub use audit::AuditStorage as AuditStorageBackend;
pub use auth_profiles::AuthProfileStorage;
pub use background_agent::BackgroundAgentStorage;
pub use chat_session::ChatSessionStorage;
pub use checkpoint::CheckpointStorage;
pub use config::{AgentDefaults, ConfigStorage, SystemConfig};
pub use daemon_state::DaemonStateStorage;
pub use deliverable::DeliverableStorage;
pub use memory::{MemoryStorage, PutChunkResult};
pub use memory_index::{IndexableChunk, MemoryIndex, SearchHit};
pub use pairing::PairingStorage;
pub use secrets::{Secret, SecretStorage, SecretStorageConfig};
pub use security_amendment::SecurityAmendmentStorage;
pub use kv_store::KvStoreStorage;
pub use simple_storage::SimpleStorage;
pub use skill::SkillStorage;
pub use terminal_session::TerminalSessionStorage;
pub use trigger::TriggerStorage;
pub use vector::{VectorConfig, VectorStats, VectorStorage};
pub use work_item::WorkItemStorage;
