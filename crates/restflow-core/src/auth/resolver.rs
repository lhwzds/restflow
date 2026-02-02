//! Credential resolver for secure credentials.

use crate::storage::SecretStorage;
use anyhow::{anyhow, Result};
use std::sync::Arc;

use super::types::SecureCredential;

/// Resolves secure credential references to actual secret values.
pub struct CredentialResolver {
    secrets: Arc<SecretStorage>,
}

impl CredentialResolver {
    pub fn new(secrets: Arc<SecretStorage>) -> Self {
        Self { secrets }
    }

    /// Resolve the primary authentication value (API key / token / access token).
    pub fn resolve_auth_value(&self, credential: &SecureCredential) -> Result<String> {
        let secret_ref = credential.primary_secret_ref();
        self.secrets
            .get_secret(secret_ref)?
            .ok_or_else(|| anyhow!("Secret not found: {}", secret_ref))
    }

    /// Resolve the refresh token for OAuth credentials, if present.
    pub fn resolve_refresh_token(&self, credential: &SecureCredential) -> Result<Option<String>> {
        match credential {
            SecureCredential::OAuth {
                refresh_token_ref: Some(refresh_ref),
                ..
            } => Ok(self.secrets.get_secret(refresh_ref)?),
            _ => Ok(None),
        }
    }

    /// Check if all required secrets exist for this credential.
    pub fn validate(&self, credential: &SecureCredential) -> Result<()> {
        for secret_ref in credential.secret_refs() {
            if self.secrets.get_secret(secret_ref)?.is_none() {
                return Err(anyhow!("Missing secret: {}", secret_ref));
            }
        }
        Ok(())
    }
}
