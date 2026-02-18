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
            "background_api_timeout_seconds" => {
                let timeout = value.as_u64().ok_or_else(|| {
                    AiError::Tool("background_api_timeout_seconds must be a number".to_string())
                })?;
                config.background_api_timeout_seconds = timeout;
            }
            "max_retries" => {
                let retries = value
                    .as_u64()
                    .ok_or_else(|| AiError::Tool("max_retries must be a number".to_string()))?;
                config.max_retries = retries as u32;
            }
            "chat_session_retention_days" => {
                let days = value.as_u64().ok_or_else(|| {
                    AiError::Tool("chat_session_retention_days must be a number".to_string())
                })?;
                config.chat_session_retention_days = days as u32;
            }
            "background_task_retention_days" => {
                let days = value.as_u64().ok_or_else(|| {
                    AiError::Tool("background_task_retention_days must be a number".to_string())
                })?;
                config.background_task_retention_days = days as u32;
            }
            "checkpoint_retention_days" => {
                let days = value.as_u64().ok_or_else(|| {
                    AiError::Tool("checkpoint_retention_days must be a number".to_string())
                })?;
                config.checkpoint_retention_days = days as u32;
            }
            "memory_chunk_retention_days" => {
                let days = value.as_u64().ok_or_else(|| {
                    AiError::Tool("memory_chunk_retention_days must be a number".to_string())
                })?;
                config.memory_chunk_retention_days = days as u32;
            }
            "experimental_features" => {
                let values = value.as_array().ok_or_else(|| {
                    AiError::Tool("experimental_features must be an array of strings".to_string())
                })?;
                let mut features = Vec::with_capacity(values.len());
                for entry in values {
                    let feature = entry.as_str().ok_or_else(|| {
                        AiError::Tool(
                            "experimental_features must be an array of strings".to_string(),
                        )
                    })?;
                    features.push(feature.to_string());
                }
                config.experimental_features = features;
            }
            key if key.starts_with("agent.") => {
                let field = &key["agent.".len()..];
                match field {
                    "tool_timeout_secs" => {
                        config.agent.tool_timeout_secs = value.as_u64().ok_or_else(|| {
                            AiError::Tool("agent.tool_timeout_secs must be a number".to_string())
                        })?;
                    }
                    "bash_timeout_secs" => {
                        config.agent.bash_timeout_secs = value.as_u64().ok_or_else(|| {
                            AiError::Tool("agent.bash_timeout_secs must be a number".to_string())
                        })?;
                    }
                    "python_timeout_secs" => {
                        config.agent.python_timeout_secs = value.as_u64().ok_or_else(|| {
                            AiError::Tool("agent.python_timeout_secs must be a number".to_string())
                        })?;
                    }
                    "max_iterations" => {
                        config.agent.max_iterations = value.as_u64().ok_or_else(|| {
                            AiError::Tool("agent.max_iterations must be a number".to_string())
                        })? as usize;
                    }
                    "subagent_timeout_secs" => {
                        config.agent.subagent_timeout_secs = value.as_u64().ok_or_else(|| {
                            AiError::Tool(
                                "agent.subagent_timeout_secs must be a number".to_string(),
                            )
                        })?;
                    }
                    "max_tool_calls" => {
                        config.agent.max_tool_calls = value.as_u64().ok_or_else(|| {
                            AiError::Tool("agent.max_tool_calls must be a number".to_string())
                        })? as usize;
                    }
                    "max_wall_clock_secs" => {
                        config.agent.max_wall_clock_secs = value.as_u64().ok_or_else(|| {
                            AiError::Tool("agent.max_wall_clock_secs must be a number".to_string())
                        })?;
                    }
                    "default_task_timeout_secs" => {
                        config.agent.default_task_timeout_secs =
                            value.as_u64().ok_or_else(|| {
                                AiError::Tool(
                                    "agent.default_task_timeout_secs must be a number".to_string(),
                                )
                            })?;
                    }
                    "default_max_duration_secs" => {
                        config.agent.default_max_duration_secs =
                            value.as_u64().ok_or_else(|| {
                                AiError::Tool(
                                    "agent.default_max_duration_secs must be a number".to_string(),
                                )
                            })?;
                    }
                    unknown => {
                        return Err(AiError::Tool(format!(
                            "Unknown agent config field: 'agent.{unknown}'. Valid agent fields: agent.tool_timeout_secs, agent.bash_timeout_secs, agent.python_timeout_secs, agent.max_iterations, agent.subagent_timeout_secs, agent.max_tool_calls, agent.max_wall_clock_secs, agent.default_task_timeout_secs, agent.default_max_duration_secs."
                        )));
                    }
                }
            }
            _ => {
                return Err(AiError::Tool(format!(
                    "Unknown config field: '{key}'. Valid fields: worker_count, task_timeout_seconds, stall_timeout_seconds, background_api_timeout_seconds, max_retries, chat_session_retention_days, background_task_retention_days, checkpoint_retention_days, memory_chunk_retention_days, experimental_features, agent.*."
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
    List,
    Reset,
    Set {
        #[serde(default)]
        config: Option<Box<SystemConfig>>,
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
                    "enum": ["get", "show", "list", "set", "reset"],
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
            ConfigAction::List => ToolOutput::success(json!({
                "fields": [
                    "worker_count",
                    "task_timeout_seconds",
                    "stall_timeout_seconds",
                    "background_api_timeout_seconds",
                    "max_retries",
                    "chat_session_retention_days",
                    "background_task_retention_days",
                    "checkpoint_retention_days",
                    "memory_chunk_retention_days",
                    "experimental_features",
                    "agent.tool_timeout_secs",
                    "agent.bash_timeout_secs",
                    "agent.python_timeout_secs",
                    "agent.max_iterations",
                    "agent.subagent_timeout_secs",
                    "agent.max_tool_calls",
                    "agent.max_wall_clock_secs",
                    "agent.default_task_timeout_secs",
                    "agent.default_max_duration_secs"
                ]
            })),
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
                    *config
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
        let err = result.expect_err("expected write-guard error");
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
        let err = result.expect_err("expected unknown field error");
        let message = err.to_string();

        assert!(message.contains("Unknown config field: 'invalid_field'"));
        assert!(message.contains(
            "Valid fields: worker_count, task_timeout_seconds, stall_timeout_seconds, background_api_timeout_seconds, max_retries, chat_session_retention_days, background_task_retention_days, checkpoint_retention_days, memory_chunk_retention_days, experimental_features, agent.*"
        ));
    }

    #[tokio::test]
    async fn test_set_experimental_features() {
        let storage = setup_storage();
        let tool = ConfigTool::new(storage).with_write(true);

        let output = tool
            .execute(json!({
                "operation": "set",
                "key": "experimental_features",
                "value": ["plan_mode", "websocket_transport"]
            }))
            .await
            .unwrap();
        assert!(output.success);
        let values = output
            .result
            .get("experimental_features")
            .and_then(|value| value.as_array())
            .expect("experimental_features should be an array");
        assert_eq!(values.len(), 2);
    }

    #[tokio::test]
    async fn test_list_supported_fields() {
        let storage = setup_storage();
        let tool = ConfigTool::new(storage);

        let output = tool.execute(json!({ "operation": "list" })).await.unwrap();
        assert!(output.success);
        assert!(
            output
                .result
                .get("fields")
                .and_then(|v| v.as_array())
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_listed_retention_fields_are_settable() {
        let storage = setup_storage();
        let tool = ConfigTool::new(storage).with_write(true);

        let updates = [
            ("chat_session_retention_days", json!(0)),
            ("background_task_retention_days", json!(14)),
            ("checkpoint_retention_days", json!(5)),
            ("memory_chunk_retention_days", json!(120)),
        ];

        for (key, value) in updates {
            let output = tool
                .execute(json!({
                    "operation": "set",
                    "key": key,
                    "value": value
                }))
                .await
                .unwrap_or_else(|err| panic!("set should support listed field '{key}': {err}"));
            assert!(
                output.success,
                "set should succeed for listed field '{key}'"
            );
        }
    }

    #[tokio::test]
    async fn test_set_agent_defaults() {
        let storage = setup_storage();
        let tool = ConfigTool::new(storage).with_write(true);

        let updates = [
            ("agent.tool_timeout_secs", json!(180)),
            ("agent.bash_timeout_secs", json!(600)),
            ("agent.python_timeout_secs", json!(60)),
            ("agent.max_iterations", json!(50)),
            ("agent.subagent_timeout_secs", json!(900)),
            ("agent.max_tool_calls", json!(300)),
            ("agent.max_wall_clock_secs", json!(3600)),
            ("agent.default_task_timeout_secs", json!(3600)),
            ("agent.default_max_duration_secs", json!(3600)),
        ];

        for (key, value) in updates {
            let output = tool
                .execute(json!({
                    "operation": "set",
                    "key": key,
                    "value": value
                }))
                .await
                .unwrap_or_else(|err| panic!("set should support agent field '{key}': {err}"));
            assert!(output.success, "set should succeed for agent field '{key}'");
        }

        // Verify the values persisted
        let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
        let agent = output
            .result
            .get("agent")
            .expect("agent block should exist");
        assert_eq!(
            agent.get("tool_timeout_secs").and_then(|v| v.as_u64()),
            Some(180)
        );
        assert_eq!(
            agent.get("bash_timeout_secs").and_then(|v| v.as_u64()),
            Some(600)
        );
        assert_eq!(
            agent.get("max_iterations").and_then(|v| v.as_u64()),
            Some(50)
        );
    }

    #[tokio::test]
    async fn test_set_agent_unknown_field() {
        let storage = setup_storage();
        let tool = ConfigTool::new(storage).with_write(true);

        let result = tool
            .execute(json!({
                "operation": "set",
                "key": "agent.nonexistent",
                "value": 42
            }))
            .await;
        let err = result.expect_err("expected unknown agent field error");
        assert!(err.to_string().contains("Unknown agent config field"));
    }

    #[tokio::test]
    async fn test_get_includes_agent_defaults() {
        let storage = setup_storage();
        let tool = ConfigTool::new(storage);

        let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
        assert!(output.success);
        let agent = output
            .result
            .get("agent")
            .expect("agent block should exist");
        assert!(agent.get("tool_timeout_secs").is_some());
        assert!(agent.get("bash_timeout_secs").is_some());
        assert!(agent.get("max_iterations").is_some());
    }
}
