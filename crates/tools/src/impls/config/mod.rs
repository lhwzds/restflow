//! System configuration tool for AI agents.

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use types::config_types::{CliConfig, ConfigDocument};
use types::store::ConfigStore;

use crate::Result;
use crate::{Tool, ToolError, ToolOutput};

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
        let has_cli_overrides =
            cli.version != default_cli.version || cli.agent.is_some() || cli.model.is_some();
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
        apply_update(key, value, &mut config)?;
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
        parameters_schema()
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
                "fields": SUPPORTED_FIELDS,
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

const SUPPORTED_FIELDS: &[&str] = &[
    "system.worker_count",
    "system.stall_timeout_seconds",
    "system.chat_response_timeout_seconds",
    "system.max_retries",
    "system.chat_session_retention_days",
    "system.log_file_retention_days",
    "system.experimental_features",
    "agent.tool_timeout_secs",
    "agent.llm_timeout_secs",
    "agent.bash_timeout_secs",
    "agent.process_session_ttl_secs",
    "agent.approval_timeout_secs",
    "agent.max_iterations",
    "agent.max_depth",
    "agent.subagent_timeout_secs",
    "agent.max_parallel_subagents",
    "agent.max_tool_calls",
    "agent.max_tool_concurrency",
    "agent.max_tool_result_length",
    "agent.prune_tool_max_chars",
    "agent.compact_preserve_tokens",
    "agent.max_wall_clock_secs",
    "agent.fallback_models",
    "api.session_list_limit",
    "api.web_search_num_results",
    "runtime.chat_max_session_history",
    "registry.github_cache_ttl_secs",
    "registry.marketplace_cache_ttl_secs",
];

const VALID_TOP_LEVEL_FIELDS: &str = "system.*, agent.*, api.*, runtime.*, registry.*";
const VALID_AGENT_FIELDS: &str = "agent.tool_timeout_secs, agent.llm_timeout_secs, agent.bash_timeout_secs, agent.process_session_ttl_secs, agent.approval_timeout_secs, agent.max_iterations, agent.max_depth, agent.subagent_timeout_secs, agent.max_parallel_subagents, agent.max_tool_calls, agent.max_tool_concurrency, agent.max_tool_result_length, agent.prune_tool_max_chars, agent.compact_preserve_tokens, agent.max_wall_clock_secs, agent.fallback_models";
const VALID_API_FIELDS: &str = "api.session_list_limit, api.web_search_num_results";
const VALID_RUNTIME_FIELDS: &str = "runtime.chat_max_session_history";
const VALID_REGISTRY_FIELDS: &str =
    "registry.github_cache_ttl_secs, registry.marketplace_cache_ttl_secs";

fn parameters_schema() -> Value {
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

fn apply_update(key: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    match key {
        "system.worker_count" => {
            config.system.worker_count = parse_u64(value, key)? as usize;
        }
        "system.stall_timeout_seconds" => {
            config.system.stall_timeout_seconds = parse_u64(value, key)?;
        }
        "system.chat_response_timeout_seconds" => {
            config.system.chat_response_timeout_seconds = parse_optional_timeout(value, key)?;
        }
        "system.max_retries" => {
            config.system.max_retries = parse_u32(value, key)?;
        }
        "system.chat_session_retention_days" => {
            config.system.chat_session_retention_days = parse_u32(value, key)?;
        }
        "system.log_file_retention_days" => {
            config.system.log_file_retention_days = parse_u32(value, key)?;
        }
        "system.experimental_features" => {
            config.system.experimental_features = parse_string_list(value, key)?;
        }
        _ if key.starts_with("system.") => {
            return Err(unknown_domain_field(
                "system",
                key.trim_start_matches("system."),
                "system.worker_count, system.stall_timeout_seconds, system.chat_response_timeout_seconds, system.max_retries, system.chat_session_retention_days, system.log_file_retention_days, system.experimental_features",
            ));
        }

        "agent.tool_timeout_secs" => {
            config.agent.tool_timeout_secs = parse_u64(value, key)?;
        }
        "agent.llm_timeout_secs" => {
            config.agent.llm_timeout_secs = parse_optional_timeout(value, key)?;
        }
        "agent.bash_timeout_secs" => {
            config.agent.bash_timeout_secs = parse_u64(value, key)?;
        }
        "agent.process_session_ttl_secs" => {
            config.agent.process_session_ttl_secs = parse_u64(value, key)?;
        }
        "agent.approval_timeout_secs" => {
            config.agent.approval_timeout_secs = parse_u64(value, key)?;
        }
        "agent.max_iterations" => {
            config.agent.max_iterations = parse_usize(value, key)?;
        }
        "agent.max_depth" => {
            config.agent.max_depth = parse_usize(value, key)?;
        }
        "agent.subagent_timeout_secs" => {
            config.agent.subagent_timeout_secs = parse_u64(value, key)?;
        }
        "agent.max_parallel_subagents" => {
            config.agent.max_parallel_subagents = parse_usize(value, key)?;
        }
        "agent.max_tool_calls" => {
            config.agent.max_tool_calls = parse_usize(value, key)?;
        }
        "agent.max_tool_concurrency" => {
            config.agent.max_tool_concurrency = parse_usize(value, key)?;
        }
        "agent.max_tool_result_length" => {
            config.agent.max_tool_result_length = parse_usize(value, key)?;
        }
        "agent.prune_tool_max_chars" => {
            config.agent.prune_tool_max_chars = parse_usize(value, key)?;
        }
        "agent.compact_preserve_tokens" => {
            config.agent.compact_preserve_tokens = parse_usize(value, key)?;
        }
        "agent.max_wall_clock_secs" => {
            config.agent.max_wall_clock_secs = parse_optional_timeout(value, key)?;
        }
        "agent.fallback_models" => {
            config.agent.fallback_models = parse_optional_string_list(value, key)?;
        }
        _ if key.starts_with("agent.") => {
            return Err(unknown_domain_field(
                "agent",
                key.trim_start_matches("agent."),
                VALID_AGENT_FIELDS,
            ));
        }

        "api.session_list_limit" => {
            config.api.session_list_limit = parse_u32(value, key)?;
        }
        "api.web_search_num_results" => {
            config.api.web_search_num_results = parse_usize(value, key)?;
        }
        _ if key.starts_with("api.") => {
            return Err(unknown_domain_field(
                "api",
                key.trim_start_matches("api."),
                VALID_API_FIELDS,
            ));
        }

        "runtime.chat_max_session_history" => {
            config.runtime.chat_max_session_history = parse_usize(value, key)?;
        }
        _ if key.starts_with("runtime.") => {
            return Err(unknown_domain_field(
                "runtime",
                key.trim_start_matches("runtime."),
                VALID_RUNTIME_FIELDS,
            ));
        }

        "registry.github_cache_ttl_secs" => {
            config.registry.github_cache_ttl_secs = parse_u64(value, key)?;
        }
        "registry.marketplace_cache_ttl_secs" => {
            config.registry.marketplace_cache_ttl_secs = parse_u64(value, key)?;
        }
        _ if key.starts_with("registry.") => {
            return Err(unknown_domain_field(
                "registry",
                key.trim_start_matches("registry."),
                VALID_REGISTRY_FIELDS,
            ));
        }

        _ => return Err(unknown_top_level_field(key)),
    }
    Ok(())
}

fn parse_u64(value: &Value, key: &str) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| ToolError::Tool(format!("{key} must be a number")))
}

fn parse_u32(value: &Value, key: &str) -> Result<u32> {
    Ok(parse_u64(value, key)? as u32)
}

fn parse_usize(value: &Value, key: &str) -> Result<usize> {
    Ok(parse_u64(value, key)? as usize)
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
        let text = entry
            .as_str()
            .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings or null")))?;
        result.push(text.to_string());
    }

    Ok(Some(result))
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

fn unknown_top_level_field(key: &str) -> ToolError {
    ToolError::Tool(format!(
        "Unknown config field: '{key}'. Valid fields: {VALID_TOP_LEVEL_FIELDS}."
    ))
}

fn unknown_domain_field(domain: &str, field: &str, valid_fields: &str) -> ToolError {
    ToolError::Tool(format!(
        "Unknown {domain} config field: '{domain}.{field}'. Valid {domain} fields: {valid_fields}."
    ))
}
