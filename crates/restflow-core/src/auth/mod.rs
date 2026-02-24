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

#[cfg(feature = "keychain")]
pub use discoverer::KeychainDiscoverer;
pub use discoverer::{
    ClaudeCodeDiscoverer, CodexCliDiscoverer, CompositeDiscoverer, CredentialDiscoverer,
    DiscoveredProfile, DiscoveryResult, EnvVarDiscoverer,
};
pub use manager::{AuthManagerConfig, AuthProfileManager, ManagerSummary, ProfileUpdate};
pub use refresh::{AnthropicRefresher, OAuthRefresher, RefreshedCredential};
pub use resolver::CredentialResolver;
pub use types::{
    AuthProfile, AuthProvider, Credential, CredentialSource, DiscoverySummary, ProfileHealth,
    ProfileSelection, SecureCredential, secret_key,
};
pub use writer::CredentialWriter;
