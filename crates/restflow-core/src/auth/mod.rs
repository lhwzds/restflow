//! Authentication Profile Management
//!
//! This module provides unified credential management for RestFlow with:
//! - Automatic credential discovery from various sources
//! - Profile storage and rotation
//! - Health tracking and cooldown management
//! - Secure storage for manual profiles

pub mod discoverer;
pub mod manager;
pub mod refresh;
pub mod resolver;
pub mod types;
pub mod writer;

pub use discoverer::{
    ClaudeCodeDiscoverer, CodexCliDiscoverer, CompositeDiscoverer, CredentialDiscoverer,
    DiscoveredProfile, DiscoveryResult, EnvVarDiscoverer,
};
#[cfg(feature = "keychain")]
pub use discoverer::KeychainDiscoverer;
pub use manager::{AuthManagerConfig, AuthProfileManager, ManagerSummary, ProfileUpdate};
pub use refresh::{AnthropicRefresher, OAuthRefresher, RefreshedCredential};
pub use resolver::CredentialResolver;
pub use types::{
    secret_key, AuthProfile, AuthProvider, Credential, CredentialSource, DiscoverySummary,
    ProfileHealth, ProfileSelection, SecureCredential,
};
pub use writer::CredentialWriter;
