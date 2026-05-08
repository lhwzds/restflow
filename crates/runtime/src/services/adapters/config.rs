use crate::ConfigStorage;
use std::sync::Arc;
use types::config_types::{CliConfig, ConfigDocument, SystemConfig};
use types::store::ConfigStore;

pub struct ConfigStoreAdapter {
    storage: Arc<ConfigStorage>,
}

impl ConfigStoreAdapter {
    pub fn new(storage: Arc<ConfigStorage>) -> Self {
        Self { storage }
    }
}

fn config_error(e: impl std::fmt::Display) -> types::ToolError {
    types::ToolError::Tool(format!(
        "Config storage error: {e}. The config file may be missing, invalid, or inaccessible."
    ))
}

impl ConfigStore for ConfigStoreAdapter {
    fn get_effective_config(&self) -> types::error::Result<ConfigDocument> {
        let system = self.storage.get_effective_config().map_err(config_error)?;
        let system = serde_json::from_value(serde_json::to_value(system).map_err(config_error)?)
            .map_err(config_error)?;
        Ok(ConfigDocument::from_system_config(
            system,
            CliConfig::default(),
        ))
    }

    fn get_writable_config(&self) -> types::error::Result<ConfigDocument> {
        let system = self.storage.get_global_config().map_err(config_error)?;
        let system = serde_json::from_value(serde_json::to_value(system).map_err(config_error)?)
            .map_err(config_error)?;
        Ok(ConfigDocument::from_system_config(
            system,
            CliConfig::default(),
        ))
    }

    fn persist_config(&self, config: &ConfigDocument) -> types::error::Result<()> {
        let system = serde_json::from_value(
            serde_json::to_value(config.system_config()).map_err(config_error)?,
        )
        .map_err(config_error)?;
        self.storage.update_config(system).map_err(config_error)?;
        Ok(())
    }

    fn reset_config(&self) -> types::error::Result<ConfigDocument> {
        let doc = ConfigDocument::from_system_config(SystemConfig::default(), CliConfig::default());
        let system = serde_json::from_value(
            serde_json::to_value(doc.system_config()).map_err(config_error)?,
        )
        .map_err(config_error)?;
        self.storage.update_config(system).map_err(config_error)?;
        Ok(doc)
    }
}
