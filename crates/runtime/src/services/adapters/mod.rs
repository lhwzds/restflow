//! Storage-backed adapter implementations for tool traits.
//!
//! Each adapter bridges a runtime storage type to a tool trait
//! defined in types, making storage functionality available
//! to tool implementations in tools.

pub mod agent;
pub mod config;
pub mod ops;
pub mod secret;
pub mod session;
pub mod skill_provider;

pub use self::agent::AgentStoreAdapter;
pub use config::ConfigStoreAdapter;
pub use ops::OpsProviderAdapter;
pub use secret::SecretStoreAdapter;
pub use session::SessionStorageAdapter;
pub use skill_provider::SkrunSkillProvider;
