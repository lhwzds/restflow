//! Storage-backed adapter implementations for tool traits.
//!
//! Each adapter bridges a restflow-core storage type to a tool trait
//! defined in restflow-traits, making storage functionality available
//! to tool implementations in restflow-tools.

pub mod agent;
pub mod auth_profile;
pub mod config;
pub mod marketplace;
pub mod memory;
pub mod ops;
pub mod secret;
pub mod security_query;
pub mod session;
pub mod skill_provider;
pub mod task_store;
pub mod terminal;
pub mod unified_search;

pub use agent::AgentStoreAdapter;
pub use auth_profile::AuthProfileStorageAdapter;
pub use config::ConfigStoreAdapter;
pub use marketplace::MarketplaceStoreAdapter;
pub use memory::DbMemoryStoreAdapter;
pub use ops::OpsProviderAdapter;
pub use secret::SecretStoreAdapter;
pub use security_query::SecurityQueryProviderAdapter;
pub use session::SessionStorageAdapter;
pub use skill_provider::SkrunSkillProvider;
pub use task_store::TaskStoreAdapter;
pub use terminal::TerminalStoreAdapter;
pub use unified_search::UnifiedMemorySearchAdapter;
