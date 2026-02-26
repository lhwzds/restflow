use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use serde_json::json;
use std::sync::Arc;

use crate::cli::ConfigCommands;
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::storage::SystemConfig;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: ConfigCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        ConfigCommands::Show => show_config(executor, format).await,
        ConfigCommands::Get { key } => get_config_value(executor, &key, format).await,
        ConfigCommands::Set { key, value } => {
            set_config_value(executor, &key, &value, format).await
        }
        ConfigCommands::Reset => reset_config(executor, format).await,
    }
}

async fn show_config(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let config = executor.get_config().await?;

    if format.is_json() {
        return print_json(&config);
    }

    let mut table = Table::new();
    table.set_header(vec!["Key", "Value"]);

    table.add_row(vec![
        Cell::new("worker_count"),
        Cell::new(config.worker_count),
    ]);
    table.add_row(vec![
        Cell::new("task_timeout_seconds"),
        Cell::new(config.task_timeout_seconds),
    ]);
    table.add_row(vec![
        Cell::new("stall_timeout_seconds"),
        Cell::new(config.stall_timeout_seconds),
    ]);
    table.add_row(vec![
        Cell::new("background_api_timeout_seconds"),
        Cell::new(format_optional_u64(config.background_api_timeout_seconds)),
    ]);
    table.add_row(vec![
        Cell::new("chat_response_timeout_seconds"),
        Cell::new(format_optional_u64(config.chat_response_timeout_seconds)),
    ]);
    table.add_row(vec![
        Cell::new("max_retries"),
        Cell::new(config.max_retries),
    ]);
    table.add_row(vec![
        Cell::new("chat_session_retention_days"),
        Cell::new(config.chat_session_retention_days),
    ]);
    table.add_row(vec![
        Cell::new("background_task_retention_days"),
        Cell::new(config.background_task_retention_days),
    ]);
    table.add_row(vec![
        Cell::new("checkpoint_retention_days"),
        Cell::new(config.checkpoint_retention_days),
    ]);
    table.add_row(vec![
        Cell::new("memory_chunk_retention_days"),
        Cell::new(config.memory_chunk_retention_days),
    ]);
    table.add_row(vec![
        Cell::new("agent.max_iterations"),
        Cell::new(config.agent.max_iterations),
    ]);
    table.add_row(vec![
        Cell::new("agent.tool_timeout_secs"),
        Cell::new(config.agent.tool_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.bash_timeout_secs"),
        Cell::new(config.agent.bash_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.python_timeout_secs"),
        Cell::new(config.agent.python_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.subagent_timeout_secs"),
        Cell::new(config.agent.subagent_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.max_tool_calls"),
        Cell::new(config.agent.max_tool_calls),
    ]);
    table.add_row(vec![
        Cell::new("agent.max_wall_clock_secs"),
        Cell::new(format_optional_u64(config.agent.max_wall_clock_secs)),
    ]);
    table.add_row(vec![
        Cell::new("agent.default_task_timeout_secs"),
        Cell::new(config.agent.default_task_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.default_max_duration_secs"),
        Cell::new(config.agent.default_max_duration_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.fallback_models"),
        Cell::new(
            config
                .agent
                .fallback_models
                .as_ref()
                .map(|m| m.join(", "))
                .unwrap_or_else(|| "none".to_string()),
        ),
    ]);
    crate::output::table::print_table(table)
}

async fn get_config_value(
    executor: Arc<dyn CommandExecutor>,
    key: &str,
    format: OutputFormat,
) -> Result<()> {
    let config = executor.get_config().await?;

    let value = match key {
        "worker_count" => json!(config.worker_count),
        "task_timeout_seconds" => json!(config.task_timeout_seconds),
        "stall_timeout_seconds" => json!(config.stall_timeout_seconds),
        "background_api_timeout_seconds" => json!(config.background_api_timeout_seconds),
        "chat_response_timeout_seconds" => json!(config.chat_response_timeout_seconds),
        "max_retries" => json!(config.max_retries),
        "chat_session_retention_days" => json!(config.chat_session_retention_days),
        "background_task_retention_days" => json!(config.background_task_retention_days),
        "checkpoint_retention_days" => json!(config.checkpoint_retention_days),
        "memory_chunk_retention_days" => json!(config.memory_chunk_retention_days),
        "agent" => json!(config.agent),
        "agent.tool_timeout_secs" => json!(config.agent.tool_timeout_secs),
        "agent.bash_timeout_secs" => json!(config.agent.bash_timeout_secs),
        "agent.python_timeout_secs" => json!(config.agent.python_timeout_secs),
        "agent.max_iterations" => json!(config.agent.max_iterations),
        "agent.subagent_timeout_secs" => json!(config.agent.subagent_timeout_secs),
        "agent.max_tool_calls" => json!(config.agent.max_tool_calls),
        "agent.max_wall_clock_secs" => json!(config.agent.max_wall_clock_secs),
        "agent.default_task_timeout_secs" => json!(config.agent.default_task_timeout_secs),
        "agent.default_max_duration_secs" => json!(config.agent.default_max_duration_secs),
        "agent.fallback_models" => json!(config.agent.fallback_models),
        _ => bail!("Unsupported config key: {key}"),
    };

    if format.is_json() {
        return print_json(&json!({ "key": key, "value": value }));
    }

    println!("{key} = {value}");
    Ok(())
}

async fn set_config_value(
    executor: Arc<dyn CommandExecutor>,
    key: &str,
    value: &str,
    format: OutputFormat,
) -> Result<()> {
    let mut config = executor.get_config().await?;

    match key {
        "worker_count" => {
            config.worker_count = parse_value(value)?;
        }
        "task_timeout_seconds" => {
            config.task_timeout_seconds = parse_value(value)?;
        }
        "stall_timeout_seconds" => {
            config.stall_timeout_seconds = parse_value(value)?;
        }
        "background_api_timeout_seconds" => {
            config.background_api_timeout_seconds = parse_optional_u64(value)?;
        }
        "chat_response_timeout_seconds" => {
            config.chat_response_timeout_seconds = parse_optional_u64(value)?;
        }
        "max_retries" => {
            config.max_retries = parse_value(value)?;
        }
        "chat_session_retention_days" => {
            config.chat_session_retention_days = parse_value(value)?;
        }
        "background_task_retention_days" => {
            config.background_task_retention_days = parse_value(value)?;
        }
        "checkpoint_retention_days" => {
            config.checkpoint_retention_days = parse_value(value)?;
        }
        "memory_chunk_retention_days" => {
            config.memory_chunk_retention_days = parse_value(value)?;
        }
        "agent.tool_timeout_secs" => {
            config.agent.tool_timeout_secs = parse_value(value)?;
        }
        "agent.bash_timeout_secs" => {
            config.agent.bash_timeout_secs = parse_value(value)?;
        }
        "agent.python_timeout_secs" => {
            config.agent.python_timeout_secs = parse_value(value)?;
        }
        "agent.max_iterations" => {
            config.agent.max_iterations = parse_value(value)?;
        }
        "agent.subagent_timeout_secs" => {
            config.agent.subagent_timeout_secs = parse_value(value)?;
        }
        "agent.max_tool_calls" => {
            config.agent.max_tool_calls = parse_value(value)?;
        }
        "agent.max_wall_clock_secs" => {
            config.agent.max_wall_clock_secs = parse_optional_u64(value)?;
        }
        "agent.default_task_timeout_secs" => {
            config.agent.default_task_timeout_secs = parse_value(value)?;
        }
        "agent.default_max_duration_secs" => {
            config.agent.default_max_duration_secs = parse_value(value)?;
        }
        "agent.fallback_models" => {
            let models: Vec<String> = serde_json::from_str(value)
                .map_err(|e| anyhow::anyhow!("Invalid JSON array: {}", e))?;
            config.agent.fallback_models = Some(models);
        }
        _ => bail!("Unsupported config key: {key}"),
    }

    executor.set_config(config).await?;

    if format.is_json() {
        return print_json(&json!({ "updated": true, "key": key }));
    }

    println!("Updated {key}");
    Ok(())
}

async fn reset_config(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let config = SystemConfig::default();
    executor.set_config(config).await?;

    if format.is_json() {
        return print_json(&json!({ "reset": true }));
    }

    println!("Configuration reset to defaults.");
    Ok(())
}

fn parse_value<T>(value: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|e| anyhow::anyhow!("Invalid value '{value}': {e}"))
}

fn parse_optional_u64(value: &str) -> Result<Option<u64>> {
    let normalized = value.trim();
    if normalized.eq_ignore_ascii_case("none")
        || normalized.eq_ignore_ascii_case("null")
        || normalized.eq_ignore_ascii_case("unset")
    {
        return Ok(None);
    }
    parse_value::<u64>(normalized).map(Some)
}

fn format_optional_u64(value: Option<u64>) -> String {
    value
        .map(|secs| secs.to_string())
        .unwrap_or_else(|| "none".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_optional_u64_none_aliases() {
        assert_eq!(parse_optional_u64("none").unwrap(), None);
        assert_eq!(parse_optional_u64("null").unwrap(), None);
        assert_eq!(parse_optional_u64("unset").unwrap(), None);
    }

    #[test]
    fn test_parse_optional_u64_number() {
        assert_eq!(parse_optional_u64("3600").unwrap(), Some(3600));
    }

    #[test]
    fn test_format_optional_u64() {
        assert_eq!(format_optional_u64(Some(42)), "42");
        assert_eq!(format_optional_u64(None), "none");
    }
}
