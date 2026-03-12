//! System configuration tool for AI agents.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt::Display;
use std::sync::Arc;

use restflow_storage::{
    CliConfig, ConfigDocument, ConfigStorage, SystemConfig, load_cli_config,
    load_global_cli_config, write_cli_config,
};

use crate::Result;
use crate::{Tool, ToolError, ToolOutput};

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

    fn storage_error(error: impl Display) -> ToolError {
        ToolError::Tool(format!(
            "Config storage error: {error}. The config file may be missing, invalid, or inaccessible. Retry the operation."
        ))
    }

    fn write_guard(&self) -> Result<()> {
        if self.allow_write {
            Ok(())
        } else {
            Err(crate::ToolError::Tool(
                "Write access to config is disabled. Available read-only operations: get, show, list. To modify config, the user must grant write permissions.".to_string(),
            ))
        }
    }

    fn get_effective_config(&self) -> Result<ConfigDocument> {
        let system = self
            .storage
            .get_effective_config()
            .map_err(Self::storage_error)?;
        let cli = load_cli_config().map_err(Self::storage_error)?;
        Ok(ConfigDocument::from_system_config(system, cli))
    }

    fn get_writable_config(&self) -> Result<ConfigDocument> {
        let system = self
            .storage
            .get_global_config()
            .map_err(Self::storage_error)?;
        let cli = load_global_cli_config().map_err(Self::storage_error)?;
        Ok(ConfigDocument::from_system_config(system, cli))
    }

    fn persist_config(&self, config: &ConfigDocument) -> Result<()> {
        self.storage
            .update_config(config.system_config())
            .map_err(Self::storage_error)?;
        write_cli_config(&config.cli).map_err(Self::storage_error)?;
        Ok(())
    }

    fn apply_update(&self, key: &str, value: &Value) -> Result<ConfigDocument> {
        let mut config = self.get_writable_config()?;

        match key {
            "system.worker_count" => {
                let count = value.as_u64().ok_or_else(|| {
                    ToolError::Tool("system.worker_count must be a number".to_string())
                })?;
                config.system.worker_count = count as usize;
            }
            "system.task_timeout_seconds" => {
                let timeout = value.as_u64().ok_or_else(|| {
                    ToolError::Tool("system.task_timeout_seconds must be a number".to_string())
                })?;
                config.system.task_timeout_seconds = timeout;
            }
            "system.stall_timeout_seconds" => {
                let timeout = value.as_u64().ok_or_else(|| {
                    ToolError::Tool("system.stall_timeout_seconds must be a number".to_string())
                })?;
                config.system.stall_timeout_seconds = timeout;
            }
            "system.background_api_timeout_seconds" => {
                config.system.background_api_timeout_seconds =
                    Self::parse_optional_timeout(value, "system.background_api_timeout_seconds")?;
            }
            "system.chat_response_timeout_seconds" => {
                config.system.chat_response_timeout_seconds =
                    Self::parse_optional_timeout(value, "system.chat_response_timeout_seconds")?;
            }
            "system.max_retries" => {
                let retries = value.as_u64().ok_or_else(|| {
                    ToolError::Tool("system.max_retries must be a number".to_string())
                })?;
                config.system.max_retries = retries as u32;
            }
            "system.chat_session_retention_days" => {
                let days = value.as_u64().ok_or_else(|| {
                    ToolError::Tool(
                        "system.chat_session_retention_days must be a number".to_string(),
                    )
                })?;
                config.system.chat_session_retention_days = days as u32;
            }
            "system.background_task_retention_days" => {
                let days = value.as_u64().ok_or_else(|| {
                    ToolError::Tool(
                        "system.background_task_retention_days must be a number".to_string(),
                    )
                })?;
                config.system.background_task_retention_days = days as u32;
            }
            "system.checkpoint_retention_days" => {
                let days = value.as_u64().ok_or_else(|| {
                    ToolError::Tool("system.checkpoint_retention_days must be a number".to_string())
                })?;
                config.system.checkpoint_retention_days = days as u32;
            }
            "system.memory_chunk_retention_days" => {
                let days = value.as_u64().ok_or_else(|| {
                    ToolError::Tool(
                        "system.memory_chunk_retention_days must be a number".to_string(),
                    )
                })?;
                config.system.memory_chunk_retention_days = days as u32;
            }
            "system.log_file_retention_days" => {
                let days = value.as_u64().ok_or_else(|| {
                    ToolError::Tool("system.log_file_retention_days must be a number".to_string())
                })?;
                config.system.log_file_retention_days = days as u32;
            }
            "system.experimental_features" => {
                let values = value.as_array().ok_or_else(|| {
                    ToolError::Tool(
                        "system.experimental_features must be an array of strings".to_string(),
                    )
                })?;
                let mut features = Vec::with_capacity(values.len());
                for entry in values {
                    let feature = entry.as_str().ok_or_else(|| {
                        ToolError::Tool(
                            "system.experimental_features must be an array of strings".to_string(),
                        )
                    })?;
                    features.push(feature.to_string());
                }
                config.system.experimental_features = features;
            }
            key if key.starts_with("agent.") => {
                let field = &key["agent.".len()..];
                match field {
                    "tool_timeout_secs" => {
                        config.agent.tool_timeout_secs = value.as_u64().ok_or_else(|| {
                            ToolError::Tool("agent.tool_timeout_secs must be a number".to_string())
                        })?;
                    }
                    "llm_timeout_secs" => {
                        config.agent.llm_timeout_secs =
                            Self::parse_optional_timeout(value, "agent.llm_timeout_secs")?;
                    }
                    "bash_timeout_secs" => {
                        config.agent.bash_timeout_secs = value.as_u64().ok_or_else(|| {
                            ToolError::Tool("agent.bash_timeout_secs must be a number".to_string())
                        })?;
                    }
                    "python_timeout_secs" => {
                        config.agent.python_timeout_secs = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "agent.python_timeout_secs must be a number".to_string(),
                            )
                        })?;
                    }
                    "browser_timeout_secs" => {
                        config.agent.browser_timeout_secs = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "agent.browser_timeout_secs must be a number".to_string(),
                            )
                        })?;
                    }
                    "process_session_ttl_secs" => {
                        config.agent.process_session_ttl_secs =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "agent.process_session_ttl_secs must be a number".to_string(),
                                )
                            })?;
                    }
                    "approval_timeout_secs" => {
                        config.agent.approval_timeout_secs = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "agent.approval_timeout_secs must be a number".to_string(),
                            )
                        })?;
                    }
                    "max_iterations" => {
                        config.agent.max_iterations = value.as_u64().ok_or_else(|| {
                            ToolError::Tool("agent.max_iterations must be a number".to_string())
                        })? as usize;
                    }
                    "subagent_timeout_secs" => {
                        config.agent.subagent_timeout_secs = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "agent.subagent_timeout_secs must be a number".to_string(),
                            )
                        })?;
                    }
                    "max_parallel_subagents" => {
                        config.agent.max_parallel_subagents = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "agent.max_parallel_subagents must be a number".to_string(),
                            )
                        })? as usize;
                    }
                    "max_tool_calls" => {
                        config.agent.max_tool_calls = value.as_u64().ok_or_else(|| {
                            ToolError::Tool("agent.max_tool_calls must be a number".to_string())
                        })? as usize;
                    }
                    "max_tool_concurrency" => {
                        config.agent.max_tool_concurrency = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "agent.max_tool_concurrency must be a number".to_string(),
                            )
                        })? as usize;
                    }
                    "max_tool_result_length" => {
                        config.agent.max_tool_result_length = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "agent.max_tool_result_length must be a number".to_string(),
                            )
                        })? as usize;
                    }
                    "prune_tool_max_chars" => {
                        config.agent.prune_tool_max_chars = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "agent.prune_tool_max_chars must be a number".to_string(),
                            )
                        })? as usize;
                    }
                    "compact_preserve_tokens" => {
                        config.agent.compact_preserve_tokens = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "agent.compact_preserve_tokens must be a number".to_string(),
                            )
                        })? as usize;
                    }
                    "max_wall_clock_secs" => {
                        config.agent.max_wall_clock_secs =
                            Self::parse_optional_timeout(value, "agent.max_wall_clock_secs")?;
                    }
                    "default_task_timeout_secs" => {
                        config.agent.default_task_timeout_secs =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "agent.default_task_timeout_secs must be a number".to_string(),
                                )
                            })?;
                    }
                    "default_max_duration_secs" => {
                        config.agent.default_max_duration_secs =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "agent.default_max_duration_secs must be a number".to_string(),
                                )
                            })?;
                    }
                    "fallback_models" => {
                        config.agent.fallback_models =
                            Self::parse_optional_string_list(value, "agent.fallback_models")?;
                    }
                    unknown => {
                        return Err(crate::ToolError::Tool(format!(
                            "Unknown agent config field: 'agent.{unknown}'. Valid agent fields: agent.tool_timeout_secs, agent.llm_timeout_secs, agent.bash_timeout_secs, agent.python_timeout_secs, agent.browser_timeout_secs, agent.process_session_ttl_secs, agent.approval_timeout_secs, agent.max_iterations, agent.subagent_timeout_secs, agent.max_parallel_subagents, agent.max_tool_calls, agent.max_tool_concurrency, agent.max_tool_result_length, agent.prune_tool_max_chars, agent.compact_preserve_tokens, agent.max_wall_clock_secs, agent.default_task_timeout_secs, agent.default_max_duration_secs, agent.fallback_models."
                        )));
                    }
                }
            }
            key if key.starts_with("api.") => {
                let field = &key["api.".len()..];
                match field {
                    "memory_search_limit" => {
                        config.api.memory_search_limit = value.as_u64().ok_or_else(|| {
                            ToolError::Tool("api.memory_search_limit must be a number".to_string())
                        })? as u32;
                    }
                    "session_list_limit" => {
                        config.api.session_list_limit = value.as_u64().ok_or_else(|| {
                            ToolError::Tool("api.session_list_limit must be a number".to_string())
                        })? as u32;
                    }
                    "background_progress_event_limit" => {
                        config.api.background_progress_event_limit =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "api.background_progress_event_limit must be a number"
                                        .to_string(),
                                )
                            })? as usize;
                    }
                    "background_message_list_limit" => {
                        config.api.background_message_list_limit =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "api.background_message_list_limit must be a number"
                                        .to_string(),
                                )
                            })? as usize;
                    }
                    "background_trace_list_limit" => {
                        config.api.background_trace_list_limit =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "api.background_trace_list_limit must be a number".to_string(),
                                )
                            })? as usize;
                    }
                    "background_trace_line_limit" => {
                        config.api.background_trace_line_limit =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "api.background_trace_line_limit must be a number".to_string(),
                                )
                            })? as usize;
                    }
                    "web_search_num_results" => {
                        config.api.web_search_num_results = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "api.web_search_num_results must be a number".to_string(),
                            )
                        })? as usize;
                    }
                    "diagnostics_timeout_ms" => {
                        config.api.diagnostics_timeout_ms = value.as_u64().ok_or_else(|| {
                            ToolError::Tool(
                                "api.diagnostics_timeout_ms must be a number".to_string(),
                            )
                        })?;
                    }
                    unknown => {
                        return Err(crate::ToolError::Tool(format!(
                            "Unknown api config field: 'api.{unknown}'. Valid api fields: api.memory_search_limit, api.session_list_limit, api.background_progress_event_limit, api.background_message_list_limit, api.background_trace_list_limit, api.background_trace_line_limit, api.web_search_num_results, api.diagnostics_timeout_ms."
                        )));
                    }
                }
            }
            key if key.starts_with("runtime.") => {
                let field = &key["runtime.".len()..];
                match field {
                    "background_runner_poll_interval_ms" => {
                        config.runtime.background_runner_poll_interval_ms =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "runtime.background_runner_poll_interval_ms must be a number"
                                        .to_string(),
                                )
                            })?;
                    }
                    "background_runner_max_concurrent_tasks" => {
                        config.runtime.background_runner_max_concurrent_tasks =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "runtime.background_runner_max_concurrent_tasks must be a number".to_string(),
                                )
                            })? as usize;
                    }
                    "chat_max_session_history" => {
                        config.runtime.chat_max_session_history =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "runtime.chat_max_session_history must be a number".to_string(),
                                )
                            })? as usize;
                    }
                    unknown => {
                        return Err(crate::ToolError::Tool(format!(
                            "Unknown runtime config field: 'runtime.{unknown}'. Valid runtime fields: runtime.background_runner_poll_interval_ms, runtime.background_runner_max_concurrent_tasks, runtime.chat_max_session_history."
                        )));
                    }
                }
            }
            key if key.starts_with("channel.") => {
                let field = &key["channel.".len()..];
                match field {
                    "telegram_api_timeout_secs" => {
                        config.channel.telegram_api_timeout_secs =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "channel.telegram_api_timeout_secs must be a number"
                                        .to_string(),
                                )
                            })?;
                    }
                    "telegram_polling_timeout_secs" => {
                        config.channel.telegram_polling_timeout_secs =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "channel.telegram_polling_timeout_secs must be a number"
                                        .to_string(),
                                )
                            })? as u32;
                    }
                    unknown => {
                        return Err(crate::ToolError::Tool(format!(
                            "Unknown channel config field: 'channel.{unknown}'. Valid channel fields: channel.telegram_api_timeout_secs, channel.telegram_polling_timeout_secs."
                        )));
                    }
                }
            }
            key if key.starts_with("registry.") => {
                let field = &key["registry.".len()..];
                match field {
                    "github_cache_ttl_secs" => {
                        config.registry.github_cache_ttl_secs =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "registry.github_cache_ttl_secs must be a number".to_string(),
                                )
                            })?;
                    }
                    "marketplace_cache_ttl_secs" => {
                        config.registry.marketplace_cache_ttl_secs =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "registry.marketplace_cache_ttl_secs must be a number"
                                        .to_string(),
                                )
                            })?;
                    }
                    unknown => {
                        return Err(crate::ToolError::Tool(format!(
                            "Unknown registry config field: 'registry.{unknown}'. Valid registry fields: registry.github_cache_ttl_secs, registry.marketplace_cache_ttl_secs."
                        )));
                    }
                }
            }
            key if key.starts_with("cli.") => {
                let field = &key["cli.".len()..];
                match field {
                    "version" => {
                        config.cli.version = value.as_u64().ok_or_else(|| {
                            ToolError::Tool("cli.version must be a number".to_string())
                        })? as u32;
                    }
                    "agent" => {
                        config.cli.agent = Self::parse_optional_string(value, "cli.agent")?;
                    }
                    "model" => {
                        config.cli.model = Self::parse_optional_string(value, "cli.model")?;
                    }
                    "sandbox.enabled" => {
                        config.cli.sandbox.enabled = value.as_bool().ok_or_else(|| {
                            ToolError::Tool("cli.sandbox.enabled must be a boolean".to_string())
                        })?;
                    }
                    "sandbox.env.isolate" => {
                        config.cli.sandbox.env.isolate = value.as_bool().ok_or_else(|| {
                            ToolError::Tool("cli.sandbox.env.isolate must be a boolean".to_string())
                        })?;
                    }
                    "sandbox.env.allow" => {
                        config.cli.sandbox.env.allow =
                            Self::parse_string_list(value, "cli.sandbox.env.allow")?;
                    }
                    "sandbox.env.block" => {
                        config.cli.sandbox.env.block =
                            Self::parse_string_list(value, "cli.sandbox.env.block")?;
                    }
                    "sandbox.limits.timeout_secs" => {
                        config.cli.sandbox.limits.timeout_secs =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "cli.sandbox.limits.timeout_secs must be a number".to_string(),
                                )
                            })?;
                    }
                    "sandbox.limits.max_output_bytes" => {
                        config.cli.sandbox.limits.max_output_bytes =
                            value.as_u64().ok_or_else(|| {
                                ToolError::Tool(
                                    "cli.sandbox.limits.max_output_bytes must be a number"
                                        .to_string(),
                                )
                            })? as usize;
                    }
                    unknown => {
                        return Err(crate::ToolError::Tool(format!(
                            "Unknown cli config field: 'cli.{unknown}'. Valid cli fields: cli.version, cli.agent, cli.model, cli.sandbox.enabled, cli.sandbox.env.isolate, cli.sandbox.env.allow, cli.sandbox.env.block, cli.sandbox.limits.timeout_secs, cli.sandbox.limits.max_output_bytes."
                        )));
                    }
                }
            }
            _ => {
                return Err(crate::ToolError::Tool(format!(
                    "Unknown config field: '{key}'. Valid fields: system.*, agent.*, api.*, runtime.*, channel.*, registry.*, cli.*."
                )));
            }
        }

        Ok(config)
    }

    fn parse_optional_timeout(value: &Value, key: &str) -> Result<Option<u64>> {
        if value.is_null() {
            return Ok(None);
        }
        value
            .as_u64()
            .map(Some)
            .ok_or_else(|| ToolError::Tool(format!("{key} must be a number or null")))
    }

    fn parse_optional_string_list(value: &Value, key: &str) -> Result<Option<Vec<String>>> {
        if value.is_null() {
            return Ok(None);
        }

        let entries = value
            .as_array()
            .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings or null")))?;

        let mut result = Vec::with_capacity(entries.len());
        for entry in entries {
            let text = entry.as_str().ok_or_else(|| {
                ToolError::Tool(format!("{key} must be an array of strings or null"))
            })?;
            result.push(text.to_string());
        }

        Ok(Some(result))
    }

    fn parse_optional_string(value: &Value, key: &str) -> Result<Option<String>> {
        if value.is_null() {
            return Ok(None);
        }
        value
            .as_str()
            .map(|text| Some(text.to_string()))
            .ok_or_else(|| ToolError::Tool(format!("{key} must be a string or null")))
    }

    fn parse_string_list(value: &Value, key: &str) -> Result<Vec<String>> {
        let values = value
            .as_array()
            .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings")))?;
        let mut result = Vec::with_capacity(values.len());
        for entry in values {
            let text = entry
                .as_str()
                .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings")))?;
            result.push(text.to_string());
        }
        Ok(result)
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
        config: Option<Box<ConfigDocument>>,
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
                let config = self.get_effective_config()?;
                ToolOutput::success(serde_json::to_value(config)?)
            }
            ConfigAction::List => ToolOutput::success(json!({
                "fields": [
                    "system.worker_count",
                    "system.task_timeout_seconds",
                    "system.stall_timeout_seconds",
                    "system.background_api_timeout_seconds",
                    "system.chat_response_timeout_seconds",
                    "system.max_retries",
                    "system.chat_session_retention_days",
                    "system.background_task_retention_days",
                    "system.checkpoint_retention_days",
                    "system.memory_chunk_retention_days",
                    "system.log_file_retention_days",
                    "system.experimental_features",
                    "agent.tool_timeout_secs",
                    "agent.llm_timeout_secs",
                    "agent.bash_timeout_secs",
                    "agent.python_timeout_secs",
                    "agent.browser_timeout_secs",
                    "agent.process_session_ttl_secs",
                    "agent.approval_timeout_secs",
                    "agent.max_iterations",
                    "agent.subagent_timeout_secs",
                    "agent.max_parallel_subagents",
                    "agent.max_tool_calls",
                    "agent.max_tool_concurrency",
                    "agent.max_tool_result_length",
                    "agent.prune_tool_max_chars",
                    "agent.compact_preserve_tokens",
                    "agent.max_wall_clock_secs",
                    "agent.default_task_timeout_secs",
                    "agent.default_max_duration_secs",
                    "agent.fallback_models",
                    "api.memory_search_limit",
                    "api.session_list_limit",
                    "api.background_progress_event_limit",
                    "api.background_message_list_limit",
                    "api.background_trace_list_limit",
                    "api.background_trace_line_limit",
                    "api.web_search_num_results",
                    "api.diagnostics_timeout_ms",
                    "runtime.background_runner_poll_interval_ms",
                    "runtime.background_runner_max_concurrent_tasks",
                    "runtime.chat_max_session_history",
                    "channel.telegram_api_timeout_secs",
                    "channel.telegram_polling_timeout_secs",
                    "registry.github_cache_ttl_secs",
                    "registry.marketplace_cache_ttl_secs",
                    "cli.version",
                    "cli.agent",
                    "cli.model",
                    "cli.sandbox.enabled",
                    "cli.sandbox.env.isolate",
                    "cli.sandbox.env.allow",
                    "cli.sandbox.env.block",
                    "cli.sandbox.limits.timeout_secs",
                    "cli.sandbox.limits.max_output_bytes"
                ]
            })),
            ConfigAction::Reset => {
                self.write_guard()?;
                let config = ConfigDocument::from_system_config(
                    SystemConfig::default(),
                    CliConfig::default(),
                );
                self.persist_config(&config)?;
                ToolOutput::success(serde_json::to_value(config)?)
            }
            ConfigAction::Set { config, key, value } => {
                self.write_guard()?;
                let updated = if let Some(config) = config {
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
                ToolOutput::success(serde_json::to_value(updated)?)
            }
        };

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    struct EnvGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set_path(key: &'static str, path: &Path) -> Self {
            let original = env::var_os(key);
            unsafe {
                env::set_var(key, path);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                unsafe {
                    env::set_var(self.key, value);
                }
            } else {
                unsafe {
                    env::remove_var(self.key);
                }
            }
        }
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    struct TestContext {
        storage: Arc<ConfigStorage>,
        _temp_dir: tempfile::TempDir,
        _global_guard: EnvGuard,
        _env_lock: std::sync::MutexGuard<'static, ()>,
    }

    fn setup_storage() -> TestContext {
        let env_guard = env_lock();
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let global_guard = EnvGuard::set_path("RESTFLOW_GLOBAL_CONFIG", &config_path);
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let storage = Arc::new(ConfigStorage::new(db).unwrap());
        TestContext {
            storage,
            _temp_dir: temp_dir,
            _global_guard: global_guard,
            _env_lock: env_guard,
        }
    }

    #[tokio::test]
    async fn test_get_config() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage);

        let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
        assert!(output.success);
        assert!(
            output
                .result
                .pointer("/system/worker_count")
                .and_then(|value| value.as_u64())
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_set_requires_write() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage);

        let result = tool
            .execute(json!({
                "operation": "set",
                "key": "system.worker_count",
                "value": 8
            }))
            .await;
        let err = result.expect_err("expected write-guard error");
        assert!(
            err.to_string()
                .contains("Available read-only operations: get, show, list")
        );
    }

    #[tokio::test]
    async fn test_set_rejects_unknown_field_with_valid_fields_hint() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

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
            "Valid fields: system.*, agent.*, api.*, runtime.*, channel.*, registry.*, cli.*."
        ));
    }

    #[tokio::test]
    async fn test_set_experimental_features() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

        let output = tool
            .execute(json!({
                "operation": "set",
                "key": "system.experimental_features",
                "value": ["plan_mode", "websocket_transport"]
            }))
            .await
            .unwrap();
        assert!(output.success);
        let values = output
            .result
            .pointer("/system/experimental_features")
            .and_then(|value| value.as_array())
            .expect("experimental_features should be an array");
        assert_eq!(values.len(), 2);
    }

    #[tokio::test]
    async fn test_set_optional_timeout_with_null() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

        let output = tool
            .execute(json!({
                "operation": "set",
                "key": "system.background_api_timeout_seconds",
                "value": null
            }))
            .await
            .unwrap();
        assert!(output.success);
        assert!(
            output
                .result
                .pointer("/system/background_api_timeout_seconds")
                .is_some_and(|v| v.is_null())
        );
    }

    #[tokio::test]
    async fn test_set_agent_max_wall_clock_with_null() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

        let output = tool
            .execute(json!({
                "operation": "set",
                "key": "agent.max_wall_clock_secs",
                "value": null
            }))
            .await
            .unwrap();
        assert!(output.success);
        let agent = output
            .result
            .get("agent")
            .expect("agent block should exist");
        assert!(
            agent
                .get("max_wall_clock_secs")
                .is_some_and(|v| v.is_null())
        );
    }

    #[tokio::test]
    async fn test_list_supported_fields() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage);

        let output = tool.execute(json!({ "operation": "list" })).await.unwrap();
        assert!(output.success);
        let fields = output
            .result
            .get("fields")
            .and_then(|v| v.as_array())
            .expect("fields should be an array");
        assert!(
            fields
                .iter()
                .any(|field| field.as_str() == Some("system.log_file_retention_days")),
            "list should expose system.log_file_retention_days"
        );
    }

    #[tokio::test]
    async fn test_listed_retention_fields_are_settable() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

        let updates = [
            ("system.chat_session_retention_days", json!(0)),
            ("system.background_task_retention_days", json!(14)),
            ("system.checkpoint_retention_days", json!(5)),
            ("system.memory_chunk_retention_days", json!(120)),
            ("system.log_file_retention_days", json!(30)),
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
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

        let updates = [
            ("agent.tool_timeout_secs", json!(180)),
            ("agent.llm_timeout_secs", json!(900)),
            ("agent.bash_timeout_secs", json!(600)),
            ("agent.python_timeout_secs", json!(60)),
            ("agent.browser_timeout_secs", json!(240)),
            ("agent.process_session_ttl_secs", json!(5400)),
            ("agent.approval_timeout_secs", json!(420)),
            ("agent.max_iterations", json!(50)),
            ("agent.subagent_timeout_secs", json!(900)),
            ("agent.max_parallel_subagents", json!(12)),
            ("agent.max_tool_calls", json!(300)),
            ("agent.max_tool_concurrency", json!(24)),
            ("agent.max_tool_result_length", json!(8192)),
            ("agent.prune_tool_max_chars", json!(4096)),
            ("agent.compact_preserve_tokens", json!(16000)),
            ("agent.max_wall_clock_secs", json!(3600)),
            ("agent.default_task_timeout_secs", json!(3600)),
            ("agent.default_max_duration_secs", json!(3600)),
            ("agent.fallback_models", json!(["glm-5", "gpt-5"])),
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
            agent.get("llm_timeout_secs").and_then(|v| v.as_u64()),
            Some(900)
        );
        assert_eq!(
            agent.get("bash_timeout_secs").and_then(|v| v.as_u64()),
            Some(600)
        );
        assert_eq!(
            agent.get("max_iterations").and_then(|v| v.as_u64()),
            Some(50)
        );
        assert_eq!(
            agent.get("browser_timeout_secs").and_then(|v| v.as_u64()),
            Some(240)
        );
        assert_eq!(
            agent
                .get("process_session_ttl_secs")
                .and_then(|v| v.as_u64()),
            Some(5400)
        );
        assert_eq!(
            agent.get("approval_timeout_secs").and_then(|v| v.as_u64()),
            Some(420)
        );
        assert_eq!(
            agent.get("max_parallel_subagents").and_then(|v| v.as_u64()),
            Some(12)
        );
        assert_eq!(
            agent.get("max_tool_concurrency").and_then(|v| v.as_u64()),
            Some(24)
        );
        assert_eq!(
            agent.get("max_tool_result_length").and_then(|v| v.as_u64()),
            Some(8192)
        );
        assert_eq!(
            agent.get("prune_tool_max_chars").and_then(|v| v.as_u64()),
            Some(4096)
        );
        assert_eq!(
            agent
                .get("compact_preserve_tokens")
                .and_then(|v| v.as_u64()),
            Some(16000)
        );
        assert_eq!(
            agent.get("fallback_models"),
            Some(&json!(["glm-5", "gpt-5"]))
        );
    }

    #[tokio::test]
    async fn test_set_agent_unknown_field() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

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
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage);

        let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
        assert!(output.success);
        let agent = output
            .result
            .get("agent")
            .expect("agent block should exist");
        assert!(agent.get("tool_timeout_secs").is_some());
        assert!(agent.get("llm_timeout_secs").is_some());
        assert!(agent.get("bash_timeout_secs").is_some());
        assert!(agent.get("browser_timeout_secs").is_some());
        assert!(agent.get("process_session_ttl_secs").is_some());
        assert!(agent.get("approval_timeout_secs").is_some());
        assert!(agent.get("max_iterations").is_some());
        assert!(agent.get("max_tool_concurrency").is_some());
        assert!(agent.get("max_tool_result_length").is_some());
        assert!(agent.get("prune_tool_max_chars").is_some());
        assert!(agent.get("compact_preserve_tokens").is_some());
        assert!(agent.get("fallback_models").is_some());
    }

    #[tokio::test]
    async fn test_set_runtime_channel_and_registry_defaults() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

        let updates = [
            ("runtime.background_runner_poll_interval_ms", json!(15000)),
            ("runtime.background_runner_max_concurrent_tasks", json!(8)),
            ("runtime.chat_max_session_history", json!(40)),
            ("channel.telegram_api_timeout_secs", json!(45)),
            ("channel.telegram_polling_timeout_secs", json!(55)),
            ("registry.github_cache_ttl_secs", json!(900)),
            ("registry.marketplace_cache_ttl_secs", json!(450)),
        ];

        for (key, value) in updates {
            let output = tool
                .execute(json!({
                    "operation": "set",
                    "key": key,
                    "value": value
                }))
                .await
                .unwrap_or_else(|err| panic!("set should support config field '{key}': {err}"));
            assert!(
                output.success,
                "set should succeed for config field '{key}'"
            );
        }

        let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
        assert_eq!(
            output
                .result
                .pointer("/runtime/background_runner_poll_interval_ms")
                .and_then(|value| value.as_u64()),
            Some(15000)
        );
        assert_eq!(
            output
                .result
                .pointer("/runtime/background_runner_max_concurrent_tasks")
                .and_then(|value| value.as_u64()),
            Some(8)
        );
        assert_eq!(
            output
                .result
                .pointer("/runtime/chat_max_session_history")
                .and_then(|value| value.as_u64()),
            Some(40)
        );
        assert_eq!(
            output
                .result
                .pointer("/channel/telegram_api_timeout_secs")
                .and_then(|value| value.as_u64()),
            Some(45)
        );
        assert_eq!(
            output
                .result
                .pointer("/channel/telegram_polling_timeout_secs")
                .and_then(|value| value.as_u64()),
            Some(55)
        );
        assert_eq!(
            output
                .result
                .pointer("/registry/github_cache_ttl_secs")
                .and_then(|value| value.as_u64()),
            Some(900)
        );
        assert_eq!(
            output
                .result
                .pointer("/registry/marketplace_cache_ttl_secs")
                .and_then(|value| value.as_u64()),
            Some(450)
        );
    }

    #[tokio::test]
    async fn test_set_agent_fallback_models_allows_null_clear() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

        tool.execute(json!({
            "operation": "set",
            "key": "agent.fallback_models",
            "value": ["glm-5", "gpt-5"]
        }))
        .await
        .expect("initial fallback_models set should succeed");

        let output = tool
            .execute(json!({
                "operation": "set",
                "key": "agent.fallback_models",
                "value": null
            }))
            .await
            .expect("clearing fallback_models should succeed");

        assert!(output.success);
        let agent = output
            .result
            .get("agent")
            .expect("agent block should exist");
        assert!(
            agent
                .get("fallback_models")
                .is_some_and(|value| value.is_null())
        );
    }

    #[tokio::test]
    async fn test_set_cli_field_preserves_workspace_override() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);
        let workspace_path = ctx._temp_dir.path().join("workspace-config.toml");
        fs::write(&workspace_path, "[cli]\nagent = \"workspace-agent\"\n")
            .expect("write workspace config");
        let _workspace_guard = EnvGuard::set_path("RESTFLOW_WORKSPACE_CONFIG", &workspace_path);

        let output = tool
            .execute(json!({
                "operation": "set",
                "key": "cli.model",
                "value": "gpt-5"
            }))
            .await
            .expect("setting cli.model should succeed");

        assert!(output.success);
        let global_cli = load_global_cli_config().expect("load global cli config");
        assert_eq!(global_cli.agent, None);
        assert_eq!(global_cli.model.as_deref(), Some("gpt-5"));

        let effective_cli = load_cli_config().expect("load effective cli config");
        assert_eq!(effective_cli.agent.as_deref(), Some("workspace-agent"));
        assert_eq!(effective_cli.model.as_deref(), Some("gpt-5"));
    }

    #[tokio::test]
    async fn test_set_api_defaults() {
        let ctx = setup_storage();
        let tool = ConfigTool::new(ctx.storage).with_write(true);

        let updates = [
            ("api.memory_search_limit", json!(25)),
            ("api.session_list_limit", json!(30)),
            ("api.background_progress_event_limit", json!(12)),
            ("api.background_message_list_limit", json!(60)),
            ("api.background_trace_list_limit", json!(80)),
            ("api.background_trace_line_limit", json!(300)),
            ("api.web_search_num_results", json!(7)),
            ("api.diagnostics_timeout_ms", json!(9000)),
        ];

        for (key, value) in updates {
            let output = tool
                .execute(json!({
                    "operation": "set",
                    "key": key,
                    "value": value
                }))
                .await
                .unwrap_or_else(|err| panic!("set should support api field '{key}': {err}"));
            assert!(output.success, "set should succeed for api field '{key}'");
        }

        let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
        let api = output.result.get("api").expect("api block should exist");
        assert_eq!(
            api.get("web_search_num_results").and_then(|v| v.as_u64()),
            Some(7)
        );
        assert_eq!(
            api.get("diagnostics_timeout_ms").and_then(|v| v.as_u64()),
            Some(9000)
        );
    }
}
