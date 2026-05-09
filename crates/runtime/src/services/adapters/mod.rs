//! Storage-backed adapter implementations for tool traits.
//!
//! Each adapter bridges a runtime storage type to a tool trait
//! defined in types, making storage functionality available
//! to tool implementations in tools.

pub mod agent;
pub mod config;
pub mod marketplace;
pub mod ops;
pub mod secret;
pub mod security_query;
pub mod session;
pub mod skill_provider;

pub use agent::AgentStoreAdapter;
pub use config::ConfigStoreAdapter;
pub use marketplace::MarketplaceStoreAdapter;
pub use ops::OpsProviderAdapter;
pub use secret::SecretStoreAdapter;
pub use security_query::SecurityQueryProviderAdapter;
pub use session::SessionStorageAdapter;
pub use skill_provider::SkrunSkillProvider;
