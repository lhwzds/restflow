//! Storage-backed adapter implementations for tool traits.
//!
//! Each adapter bridges a restflow-core storage type to a tool trait
//! defined in restflow-traits, making storage functionality available
//! to tool implementations in restflow-tools.

pub mod agent;
pub mod auth_profile;
pub mod background_agent;
pub mod deliverable;
pub mod kv_store;
pub mod marketplace;
pub mod memory;
pub mod ops;
pub mod security_query;
pub mod session;
pub mod skill_provider;
pub mod terminal;
pub mod trigger;
pub mod unified_search;
pub mod work_item;

pub use agent::AgentStoreAdapter;
pub use auth_profile::AuthProfileStorageAdapter;
pub use background_agent::BackgroundAgentStoreAdapter;
pub use deliverable::DeliverableStoreAdapter;
pub use kv_store::KvStoreAdapter;
pub use marketplace::MarketplaceStoreAdapter;
pub use memory::{DbMemoryStoreAdapter, MemoryManagerAdapter};
pub use ops::OpsProviderAdapter;
pub use security_query::SecurityQueryProviderAdapter;
pub use session::SessionStorageAdapter;
pub use skill_provider::SkillStorageProvider;
pub use terminal::TerminalStoreAdapter;
pub use trigger::TriggerStoreAdapter;
pub use unified_search::UnifiedMemorySearchAdapter;
pub use work_item::DbWorkItemAdapter;
