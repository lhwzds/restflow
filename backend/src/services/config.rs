use crate::{AppCore, storage::config::SystemConfig};
use std::sync::Arc;

// Get complete system configuration
pub async fn get_config(core: &Arc<AppCore>) -> Result<SystemConfig, String> {
    match core.storage.config.get_config() {
        Ok(Some(config)) => Ok(config),
        Ok(None) => Ok(SystemConfig::default()),
        Err(e) => Err(format!("Failed to get config: {}", e)),
    }
}

// Update system configuration with validation
pub async fn update_config(
    core: &Arc<AppCore>,
    config: SystemConfig
) -> Result<(), String> {
    // Validate configuration before updating
    config.validate()
        .map_err(|e| format!("Invalid configuration: {}", e))?;
    
    // Update configuration
    core.storage.config.update_config(config)
        .map_err(|e| format!("Failed to update config: {}", e))
}