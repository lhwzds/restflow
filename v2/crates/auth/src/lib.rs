//! # codocia
//!
//! Auth owns secret references and provider access profiles.
//!
//! ## Owns
//! - SecretRef
//! - Profile
//! - provider credential references
//!
//! ## Must Not
//! - expose secret values in docs
//! - define model catalog entries
//! - call UI code
//!
//! ## Inputs
//! - provider IDs
//! - secret keys
//!
//! ## Outputs
//! - auth profiles
//! - secret references
//!
//! ## Depends On
//! - model
//! - store
//!
//! ## Verify
//! - cargo check -p auth

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRef {
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub provider: String,
    pub secret: SecretRef,
}
