//! System configuration tool for AI agents.

mod action;
mod fields;
mod parse;
mod schema;
#[cfg(test)]
mod tests;
mod update;

use async_trait::async_trait;
use restflow_traits::config_types::{CliConfig, ConfigDocument};
use restflow_traits::store::ConfigStore;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolError, ToolOutput};

use self::action::ConfigAction;

#[derive(Clone)]
pub struct ConfigTool {
    store: Arc<dyn ConfigStore>,
    allow_write: bool,
}

impl ConfigTool {
    pub fn new(store: Arc<dyn ConfigStore>) -> Self {
        Self {
            store,
            allow_write: false,
        }
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.allow_write = allow_write;
        self
    }

    fn storage_error(error: impl std::fmt::Display) -> ToolError {
        ToolError::Tool(format!(
            "Config storage error: {error}. The config file may be missing, invalid, or inaccessible. Retry the operation."
        ))
    }

    fn write_guard(&self) -> Result<()> {
        if self.allow_write {
            Ok(())
        } else {
            Err(ToolError::Tool(
                "Write access to config is disabled. Available read-only operations: get, show, list. To modify config, the user must grant write permissions.".to_string(),
            ))
        }
    }

    fn get_effective_config(&self) -> Result<ConfigDocument> {
        self.store
            .get_effective_config()
            .map_err(Self::storage_error)
    }

    fn get_writable_config(&self) -> Result<ConfigDocument> {
        self.store
            .get_writable_config()
            .map_err(Self::storage_error)
    }

    fn persist_config(&self, config: &ConfigDocument) -> Result<()> {
        self.store
            .persist_config(config)
            .map_err(Self::storage_error)
    }

    fn daemon_view(config: &ConfigDocument) -> Result<Value> {
        let mut encoded = serde_json::to_value(config)?;
        if let Some(object) = encoded.as_object_mut() {
            object.remove("cli");
        }
        Ok(encoded)
    }

    fn reject_cli_local_config(config: &ConfigDocument) -> Result<()> {
        let default_cli = CliConfig::default();
        let cli = &config.cli;
        let has_cli_overrides = cli.version != default_cli.version
            || cli.agent.is_some()
            || cli.model.is_some()
            || cli.sandbox.enabled
            || cli.sandbox.env.isolate
            || !cli.sandbox.env.allow.is_empty()
            || !cli.sandbox.env.block.is_empty()
            || cli.sandbox.limits.timeout_secs != default_cli.sandbox.limits.timeout_secs
            || cli.sandbox.limits.max_output_bytes
                != default_cli.sandbox.limits.max_output_bytes;
        if has_cli_overrides {
            return Err(ToolError::Tool(
                "CLI-local config fields are not available through manage_config. Use the CLI-local config command path for cli.* settings.".to_string(),
            ));
        }
        Ok(())
    }

    fn reject_cli_section_in_payload(input: &Value) -> Result<()> {
        let has_cli_section = input
            .get("operation")
            .and_then(Value::as_str)
            .is_some_and(|operation| operation == "set")
            && input
                .get("config")
                .and_then(Value::as_object)
                .is_some_and(|config| config.contains_key("cli"));
        if has_cli_section {
            return Err(ToolError::Tool(
                "CLI-local config fields are not available through manage_config. Use the CLI-local config command path for cli.* settings.".to_string(),
            ));
        }
        Ok(())
    }

    fn apply_update(&self, key: &str, value: &Value) -> Result<ConfigDocument> {
        let mut config = self.get_writable_config()?;
        update::apply_update(key, value, &mut config)?;
        Ok(config)
    }
}

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &str {
        "manage_config"
    }

    fn description(&self) -> &str {
        "Read and update runtime configuration values such as workers, retries, and timeouts."
    }

    fn parameters_schema(&self) -> Value {
        schema::parameters_schema()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        Self::reject_cli_section_in_payload(&input)?;
        let action: ConfigAction = serde_json::from_value(input)?;

        let output = match action {
            ConfigAction::Get | ConfigAction::Show => {
                let config = self.get_effective_config()?;
                ToolOutput::success(Self::daemon_view(&config)?)
            }
            ConfigAction::List => ToolOutput::success(json!({
                "fields": fields::SUPPORTED_FIELDS,
            })),
            ConfigAction::Reset => {
                self.write_guard()?;
                let config = self.store.reset_config().map_err(Self::storage_error)?;
                ToolOutput::success(Self::daemon_view(&config)?)
            }
            ConfigAction::Set { config, key, value } => {
                self.write_guard()?;
                let updated = if let Some(config) = config {
                    Self::reject_cli_local_config(&config)?;
                    *config
                } else if let Some(key) = key {
                    let resolved_value = value.unwrap_or(Value::Null);
                    self.apply_update(&key, &resolved_value)?
                } else {
                    return Ok(ToolOutput::error(
                        "set requires either config or key/value".to_string(),
                    ));
                };

                self.persist_config(&updated)?;
                ToolOutput::success(Self::daemon_view(&updated)?)
            }
        };

        Ok(output)
    }
}
