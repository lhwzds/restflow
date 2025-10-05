use crate::{AppCore, models::Secret};
use anyhow::{Result, Context};
use std::sync::Arc;

/// List all secrets (without values for security)
pub async fn list_secrets(core: &Arc<AppCore>) -> Result<Vec<Secret>> {
    core.storage.secrets.list_secrets()
        .context("Failed to list secrets")
}

/// Get a secret value by key
pub async fn get_secret(core: &Arc<AppCore>, key: &str) -> Result<Option<String>> {
    core.storage.secrets.get_secret(key)
        .with_context(|| format!("Failed to get secret {}", key))
}

/// Set or update a secret with optional description
pub async fn set_secret(core: &Arc<AppCore>, key: &str, value: &str, description: Option<String>) -> Result<()> {
    core.storage.secrets.set_secret(key, value, description)
        .with_context(|| format!("Failed to set secret {}", key))
}

/// Delete a secret
pub async fn delete_secret(core: &Arc<AppCore>, key: &str) -> Result<()> {
    core.storage.secrets.delete_secret(key)
        .with_context(|| format!("Failed to delete secret {}", key))
}

/// Check if a secret exists
pub async fn has_secret(core: &Arc<AppCore>, key: &str) -> Result<bool> {
    core.storage.secrets.has_secret(key)
        .with_context(|| format!("Failed to check secret {}", key))
}