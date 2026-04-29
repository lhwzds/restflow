//! # codocia
//!
//! Model owns provider and model identity for the V2 kernel.
//!
//! ## Owns
//! - provider identity
//! - model identity
//! - canonical model construction
//!
//! ## Must Not
//! - read secrets
//! - call model providers
//! - depend on daemon state
//!
//! ## Inputs
//! - provider IDs
//! - model IDs
//!
//! ## Outputs
//! - Model
//! - Provider
//!
//! ## Used By
//! - agent
//! - auth
//!
//! ## Verify
//! - cargo check -p model

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Model {
    pub provider: Provider,
    pub id: String,
}

impl Model {
    pub fn new(provider: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            provider: Provider {
                id: provider.into(),
            },
            id: id.into(),
        }
    }
}
