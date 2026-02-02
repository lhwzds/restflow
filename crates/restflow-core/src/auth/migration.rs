//! Migration utilities for moving legacy plaintext credentials to SecretStorage.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

use super::types::{Credential, CredentialSource, AuthProvider, ProfileHealth};
use super::writer::CredentialWriter;
use super::types::AuthProfile;
use crate::storage::SecretStorage;

/// Legacy profile format (with plaintext credentials).
#[derive(Debug, Deserialize)]
struct LegacyAuthProfile {
    pub id: String,
    pub name: String,
    pub credential: Credential,
    pub source: CredentialSource,
    pub provider: AuthProvider,
    #[serde(default)]
    pub health: ProfileHealth,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_failed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub failure_count: u32,
    #[serde(default)]
    pub cooldown_until: Option<DateTime<Utc>>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Default)]
pub struct MigrationResult {
    pub migrated: usize,
    pub errors: Vec<String>,
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

    let legacy_profiles: Vec<LegacyAuthProfile> = match serde_json::from_str(&content) {
        Ok(profiles) => profiles,
        Err(_) => {
            return Ok(MigrationResult::default());
        }
    };

    if legacy_profiles.is_empty() {
        return Ok(MigrationResult::default());
    }

    let writer = CredentialWriter::new(Arc::new(secrets.clone()));
    let mut migrated_profiles = Vec::new();
    let mut errors = Vec::new();
    let mut migrated = 0;

    for profile in legacy_profiles {
        match migrate_single_profile(&profile, &writer) {
            Ok(secure) => {
                migrated += 1;
                migrated_profiles.push(secure);
            }
            Err(err) => {
                errors.push(format!("{}: {}", profile.id, err));
            }
        }
    }

    if migrated > 0 {
        let backup_path = legacy_path.with_extension("json.bak");
        tokio::fs::rename(legacy_path, &backup_path).await?;
        let content = serde_json::to_string_pretty(&migrated_profiles)?;
        tokio::fs::write(legacy_path, content).await?;
    }

    Ok(MigrationResult { migrated, errors })
}

fn migrate_single_profile(
    legacy: &LegacyAuthProfile,
    writer: &CredentialWriter,
) -> Result<AuthProfile> {
    let secure_credential = writer.store_credential(&legacy.id, &legacy.credential)?;

    Ok(AuthProfile {
        id: legacy.id.clone(),
        name: legacy.name.clone(),
        credential: secure_credential,
        source: legacy.source,
        provider: legacy.provider,
        health: legacy.health,
        enabled: legacy.enabled,
        priority: legacy.priority,
        created_at: legacy.created_at,
        last_used_at: legacy.last_used_at,
        last_failed_at: legacy.last_failed_at,
        failure_count: legacy.failure_count,
        cooldown_until: legacy.cooldown_until,
    })
}
