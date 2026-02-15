//! Credential writer for secure credential storage.

use crate::storage::SecretStorage;
use anyhow::Result;
use std::sync::Arc;

use super::types::{Credential, SecureCredential, secret_key};

/// Writes credentials to SecretStorage.
pub struct CredentialWriter {
    secrets: Arc<SecretStorage>,
}

impl CredentialWriter {
    pub fn new(secrets: Arc<SecretStorage>) -> Self {
        Self { secrets }
    }

    /// Convert a legacy credential to a secure credential, storing secrets.
    pub fn store_credential(
        &self,
        profile_id: &str,
        credential: &Credential,
    ) -> Result<SecureCredential> {
        match credential {
            Credential::ApiKey { key, email } => {
                let secret_ref = secret_key(profile_id, "api_key");
                self.secrets.set_secret(&secret_ref, key, None)?;
                Ok(SecureCredential::ApiKey {
                    secret_ref,
                    email: email.clone(),
                })
            }
            Credential::Token {
                token,
                expires_at,
                email,
            } => {
                let secret_ref = secret_key(profile_id, "token");
                self.secrets.set_secret(&secret_ref, token, None)?;
                Ok(SecureCredential::Token {
                    secret_ref,
                    expires_at: *expires_at,
                    email: email.clone(),
                })
            }
            Credential::OAuth {
                access_token,
                refresh_token,
                expires_at,
                email,
            } => {
                let access_token_ref = secret_key(profile_id, "access_token");
                self.secrets
                    .set_secret(&access_token_ref, access_token, None)?;
                let refresh_token_ref = if let Some(refresh) = refresh_token {
                    let refresh_ref = secret_key(profile_id, "refresh_token");
                    self.secrets.set_secret(&refresh_ref, refresh, None)?;
                    Some(refresh_ref)
                } else {
                    None
                };
                Ok(SecureCredential::OAuth {
                    access_token_ref,
                    refresh_token_ref,
                    expires_at: *expires_at,
                    email: email.clone(),
                })
            }
        }
    }

    /// Delete all secrets for a credential.
    pub fn delete_credential(&self, credential: &SecureCredential) -> Result<()> {
        for secret_ref in credential.secret_refs() {
            self.secrets.delete_secret(secret_ref)?;
        }
        Ok(())
    }

    /// Update a specific secret value.
    pub fn update_secret(&self, secret_ref: &str, value: &str) -> Result<()> {
        self.secrets.set_secret(secret_ref, value, None)
    }
}
