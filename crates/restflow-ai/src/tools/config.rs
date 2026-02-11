//! System configuration tool for AI agents.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt::Display;
use std::sync::Arc;

use restflow_storage::{ConfigStorage, SystemConfig};

use super::traits::{Tool, ToolOutput};
use crate::error::{AiError, Result};

#[derive(Clone)]
pub struct ConfigTool {
    storage: Arc<ConfigStorage>,
    allow_write: bool,
}

impl ConfigTool {
    pub fn new(storage: Arc<ConfigStorage>) -> Self {
        Self {
            storage,
            allow_write: false,
        }
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.allow_write = allow_write;
        self
    }

    fn storage_error(error: impl Display) -> AiError {
        AiError::Tool(format!(
            "Config storage error: {error}. The database may be locked or corrupted. Retry the operation."
        ))
    }

    fn write_guard(&self) -> Result<()> {
        if self.allow_write {
            Ok(())
        } else {
            Err(AiError::Tool(
                "Write access to config is disabled. Available read-only operations: get, list. To modify config, the user must grant write permissions.".to_string(),
            ))
        }
    }

    fn get_config(&self) -> Result<SystemConfig> {
        self.storage
            .get_config()
            .map_err(Self::storage_error)?
            .ok_or_else(|| {
                AiError::Tool(
                    "Config not initialized. Use 'reset' operation to create default configuration."
                        .to_string(),
                )
            })
    }

    fn apply_update(&self, key: &str, value: &Value) -> Result<SystemConfig> {
        let mut config = self.get_config()?;

        match key {
            "worker_count" => {
                let count = value
                    .as_u64()
                    .ok_or_else(|| AiError::Tool("worker_count must be a number".to_string()))?;
                config.worker_count = count as usize;
            }
            "task_timeout_seconds" => {
                let timeout = value.as_u64().ok_or_else(|| {
                    AiError::Tool("task_timeout_seconds must be a number".to_string())
                })?;
                config.task_timeout_seconds = timeout;
            }
            "stall_timeout_seconds" => {
                let timeout = value.as_u64().ok_or_else(|| {
                    AiError::Tool("stall_timeout_seconds must be a number".to_string())
                })?;
                config.stall_timeout_seconds = timeout;
            }
            "max_retries" => {
                let retries = value
                    .as_u64()
                    .ok_or_else(|| AiError::Tool("max_retries must be a number".to_string()))?;
                config.max_retries = retries as u32;
            }
            _ => {
                return Err(AiError::Tool(format!(
                    "Unknown config field: '{key}'. Valid fields: worker_count, task_timeout_seconds, stall_timeout_seconds, max_retries."
                )));
            }
        }

        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum ConfigAction {
    Get,
    Show,
    Reset,
    Set {
        #[serde(default)]
        config: Option<SystemConfig>,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        value: Option<Value>,
    },
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
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["get", "show", "set", "reset"],
                    "description": "Config operation to perform"
                },
                "config": {
                    "type": "object",
                    "description": "Full config object (for set)"
                },
                "key": {
                    "type": "string",
                    "description": "Config field to update (for set)"
                },
                "value": {
                    "description": "Value for the config field (for set)"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: ConfigAction = serde_json::from_value(input)?;

        let output = match action {
            ConfigAction::Get | ConfigAction::Show => {
                let config = self.get_config()?;
                ToolOutput::success(serde_json::to_value(config)?)
            }
            ConfigAction::Reset => {
                self.write_guard()?;
                let config = SystemConfig::default();
                self.storage
                    .update_config(config.clone())
                    .map_err(Self::storage_error)?;
                ToolOutput::success(serde_json::to_value(config)?)
            }
            ConfigAction::Set { config, key, value } => {
                self.write_guard()?;
                let updated = if let Some(config) = config {
                    config
                } else if let (Some(key), Some(value)) = (key, value) {
                    self.apply_update(&key, &value)?
                } else {
                    return Ok(ToolOutput::error(
                        "set requires either config or key/value".to_string(),
                    ));
                };

                self.storage
                    .update_config(updated.clone())
                    .map_err(Self::storage_error)?;
                ToolOutput::success(serde_json::to_value(updated)?)
            }
        };

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_storage() -> Arc<ConfigStorage> {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        Arc::new(ConfigStorage::new(db).unwrap())
    }

    #[tokio::test]
    async fn test_get_config() {
        let storage = setup_storage();
        let tool = ConfigTool::new(storage);

        let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
        assert!(output.success);
        assert!(output.result.get("worker_count").is_some());
    }

    #[tokio::test]
    async fn test_set_requires_write() {
        let storage = setup_storage();
        let tool = ConfigTool::new(storage);

        let result = tool
            .execute(json!({
                "operation": "set",
                "key": "worker_count",
                "value": 8
            }))
            .await;
        let err = result.err().expect("expected write-guard error");
        assert!(
            err.to_string()
                .contains("Available read-only operations: get, list")
        );
    }

    #[tokio::test]
    async fn test_set_rejects_unknown_field_with_valid_fields_hint() {
        let storage = setup_storage();
        let tool = ConfigTool::new(storage).with_write(true);

        let result = tool
            .execute(json!({
                "operation": "set",
                "key": "invalid_field",
                "value": 8
            }))
            .await;
        let err = result.err().expect("expected unknown field error");
        let message = err.to_string();

        assert!(message.contains("Unknown config field: 'invalid_field'"));
        assert!(message.contains(
            "Valid fields: worker_count, task_timeout_seconds, stall_timeout_seconds, max_retries"
        ));
    }
}
