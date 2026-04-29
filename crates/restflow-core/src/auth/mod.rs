//! Authentication Profile Management
//!
//! This module provides unified credential management for RestFlow with:
//! - Profile storage and rotation
//! - Health tracking and cooldown management
//! - Secure storage for manual profiles

pub mod manager;
pub(crate) mod provider_access;
pub mod refresh;
pub mod resolver;
pub mod types;
pub mod writer;

pub use manager::{AuthManagerConfig, AuthProfileManager, ManagerSummary, ProfileUpdate};
pub(crate) use provider_access::{
    build_runtime_api_keys, provider_available, resolve_model_from_credentials, secret_exists,
};
pub use refresh::{AnthropicRefresher, OAuthRefresher, RefreshedCredential};
pub use resolver::CredentialResolver;
pub use types::{
    AuthProfile, AuthProvider, Credential, CredentialSource, ProfileHealth, ProfileSelection,
    SecureCredential, secret_key,
};
pub use writer::CredentialWriter;
