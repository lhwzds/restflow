use crate::AppCore;
use crate::storage::SystemConfig;
use anyhow::{Context, Result};
use std::sync::Arc;

// Get complete system configuration
pub async fn get_config(core: &Arc<AppCore>) -> Result<SystemConfig> {
    core.storage
        .config
        .get_effective_config()
        .context("Failed to get config")
}

// Update system configuration with validation
pub async fn update_config(core: &Arc<AppCore>, config: SystemConfig) -> Result<()> {
    // Validate configuration before updating
    config.validate().context("Invalid configuration")?;

    // Update configuration
    core.storage
        .config
        .update_config(config)
        .context("Failed to update config")
}
