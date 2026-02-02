//! Migrate plaintext credentials to SecretStorage.

use crate::auth::types::{secret_key, Credential, SecureCredential};
use crate::storage::SecretStorage;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

/// Legacy profile format (with plaintext credentials).
#[derive(Debug, Deserialize)]
struct LegacyAuthProfile {
    id: String,
    credential: Credential,
}

/// Migrate legacy profiles to secure storage.
pub async fn migrate_profiles(
    legacy_path: &Path,
    secrets: &SecretStorage,
) -> Result<MigrationResult> {
    if !legacy_path.exists() {
        return Ok(MigrationResult::default());
    }

    let content = tokio::fs::read_to_string(legacy_path).await?;
    let legacy_profiles: Vec<LegacyAuthProfile> = serde_json::from_str(&content)?;

    let mut migrated = 0;
    let mut errors = Vec::new();

    for profile in legacy_profiles {
        match migrate_single_profile(&profile, secrets) {
            Ok(_) => migrated += 1,
            Err(error) => errors.push(format!("{}: {}", profile.id, error)),
        }
    }

    if migrated > 0 {
        let backup_path = legacy_path.with_extension("json.bak");
        tokio::fs::rename(legacy_path, &backup_path).await?;
        tracing::info!(
            migrated,
            backup = %backup_path.display(),
            "Migrated legacy profiles to secure storage"
        );
    }

    Ok(MigrationResult { migrated, errors })
}

fn migrate_single_profile(profile: &LegacyAuthProfile, secrets: &SecretStorage) -> Result<SecureCredential> {
    match &profile.credential {
        Credential::ApiKey { key, email } => {
            let secret_ref = secret_key(&profile.id, "api_key");
            secrets.set_secret(&secret_ref, key, None)?;
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
            let secret_ref = secret_key(&profile.id, "token");
            secrets.set_secret(&secret_ref, token, None)?;
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
            let access_ref = secret_key(&profile.id, "access_token");
            secrets.set_secret(&access_ref, access_token, None)?;
            let refresh_ref = if let Some(rt) = refresh_token {
                let ref_key = secret_key(&profile.id, "refresh_token");
                secrets.set_secret(&ref_key, rt, None)?;
                Some(ref_key)
            } else {
                None
            };
            Ok(SecureCredential::OAuth {
                access_token_ref: access_ref,
                refresh_token_ref: refresh_ref,
                expires_at: *expires_at,
                email: email.clone(),
            })
        }
    }
}

#[derive(Debug, Default)]
pub struct MigrationResult {
    pub migrated: usize,
    pub errors: Vec<String>,
}
