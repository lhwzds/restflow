use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use serde_json::json;
use std::sync::Arc;

use crate::cli::ConfigCommands;
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::storage::SystemConfig;
use restflow_storage::{
    CliConfig, ConfigDocument, effective_config_sources, load_cli_config, load_global_cli_config,
    write_cli_config,
};

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
    let config = load_effective_config_document(executor).await?;
    let sources = effective_config_sources()?;

    if format.is_json() {
        let mut payload = serde_json::to_value(&config)?;
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "_effective_sources".to_string(),
                serde_json::to_value(&sources)?,
            );
        }
        return print_json(&payload);
    }

    let mut table = Table::new();
    table.set_header(vec!["Key", "Value"]);

    table.add_row(vec![
        Cell::new("system.worker_count"),
        Cell::new(config.system.worker_count),
    ]);
    table.add_row(vec![
        Cell::new("system.task_timeout_seconds"),
        Cell::new(config.system.task_timeout_seconds),
    ]);
    table.add_row(vec![
        Cell::new("system.stall_timeout_seconds"),
        Cell::new(config.system.stall_timeout_seconds),
    ]);
    table.add_row(vec![
        Cell::new("system.background_api_timeout_seconds"),
        Cell::new(format_optional_u64(
            config.system.background_api_timeout_seconds,
        )),
    ]);
    table.add_row(vec![
        Cell::new("system.chat_response_timeout_seconds"),
        Cell::new(format_optional_u64(
            config.system.chat_response_timeout_seconds,
        )),
    ]);
    table.add_row(vec![
        Cell::new("system.max_retries"),
        Cell::new(config.system.max_retries),
    ]);
    table.add_row(vec![
        Cell::new("system.chat_session_retention_days"),
        Cell::new(config.system.chat_session_retention_days),
    ]);
    table.add_row(vec![
        Cell::new("system.background_task_retention_days"),
        Cell::new(config.system.background_task_retention_days),
    ]);
    table.add_row(vec![
        Cell::new("system.checkpoint_retention_days"),
        Cell::new(config.system.checkpoint_retention_days),
    ]);
    table.add_row(vec![
        Cell::new("system.memory_chunk_retention_days"),
        Cell::new(config.system.memory_chunk_retention_days),
    ]);
    table.add_row(vec![
        Cell::new("system.log_file_retention_days"),
        Cell::new(config.system.log_file_retention_days),
    ]);
    table.add_row(vec![
        Cell::new("system.experimental_features"),
        Cell::new(format_string_list(&config.system.experimental_features)),
    ]);
    table.add_row(vec![
        Cell::new("agent.max_iterations"),
        Cell::new(config.agent.max_iterations),
    ]);
    table.add_row(vec![
        Cell::new("agent.max_depth"),
        Cell::new(config.agent.max_depth),
    ]);
    table.add_row(vec![
        Cell::new("agent.tool_timeout_secs"),
        Cell::new(config.agent.tool_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.llm_timeout_secs"),
        Cell::new(format_optional_u64(config.agent.llm_timeout_secs)),
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
        Cell::new("agent.browser_timeout_secs"),
        Cell::new(config.agent.browser_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.process_session_ttl_secs"),
        Cell::new(config.agent.process_session_ttl_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.approval_timeout_secs"),
        Cell::new(config.agent.approval_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.subagent_timeout_secs"),
        Cell::new(config.agent.subagent_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("agent.max_parallel_subagents"),
        Cell::new(config.agent.max_parallel_subagents),
    ]);
    table.add_row(vec![
        Cell::new("agent.max_tool_calls"),
        Cell::new(config.agent.max_tool_calls),
    ]);
    table.add_row(vec![
        Cell::new("agent.max_tool_concurrency"),
        Cell::new(config.agent.max_tool_concurrency),
    ]);
    table.add_row(vec![
        Cell::new("agent.max_tool_result_length"),
        Cell::new(config.agent.max_tool_result_length),
    ]);
    table.add_row(vec![
        Cell::new("agent.prune_tool_max_chars"),
        Cell::new(config.agent.prune_tool_max_chars),
    ]);
    table.add_row(vec![
        Cell::new("agent.compact_preserve_tokens"),
        Cell::new(config.agent.compact_preserve_tokens),
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
    table.add_row(vec![
        Cell::new("api.memory_search_limit"),
        Cell::new(config.api.memory_search_limit),
    ]);
    table.add_row(vec![
        Cell::new("api.session_list_limit"),
        Cell::new(config.api.session_list_limit),
    ]);
    table.add_row(vec![
        Cell::new("api.background_progress_event_limit"),
        Cell::new(config.api.background_progress_event_limit),
    ]);
    table.add_row(vec![
        Cell::new("api.background_message_list_limit"),
        Cell::new(config.api.background_message_list_limit),
    ]);
    table.add_row(vec![
        Cell::new("api.background_trace_list_limit"),
        Cell::new(config.api.background_trace_list_limit),
    ]);
    table.add_row(vec![
        Cell::new("api.background_trace_line_limit"),
        Cell::new(config.api.background_trace_line_limit),
    ]);
    table.add_row(vec![
        Cell::new("api.web_search_num_results"),
        Cell::new(config.api.web_search_num_results),
    ]);
    table.add_row(vec![
        Cell::new("api.diagnostics_timeout_ms"),
        Cell::new(config.api.diagnostics_timeout_ms),
    ]);
    table.add_row(vec![
        Cell::new("runtime.background_runner_poll_interval_ms"),
        Cell::new(config.runtime.background_runner_poll_interval_ms),
    ]);
    table.add_row(vec![
        Cell::new("runtime.background_runner_max_concurrent_tasks"),
        Cell::new(config.runtime.background_runner_max_concurrent_tasks),
    ]);
    table.add_row(vec![
        Cell::new("runtime.chat_max_session_history"),
        Cell::new(config.runtime.chat_max_session_history),
    ]);
    table.add_row(vec![
        Cell::new("channel.telegram_api_timeout_secs"),
        Cell::new(config.channel.telegram_api_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("channel.telegram_polling_timeout_secs"),
        Cell::new(config.channel.telegram_polling_timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("registry.github_cache_ttl_secs"),
        Cell::new(config.registry.github_cache_ttl_secs),
    ]);
    table.add_row(vec![
        Cell::new("registry.marketplace_cache_ttl_secs"),
        Cell::new(config.registry.marketplace_cache_ttl_secs),
    ]);
    table.add_row(vec![
        Cell::new("cli.version"),
        Cell::new(config.cli.version),
    ]);
    table.add_row(vec![
        Cell::new("cli.agent"),
        Cell::new(format_optional_string(config.cli.agent.as_deref())),
    ]);
    table.add_row(vec![
        Cell::new("cli.model"),
        Cell::new(format_optional_string(config.cli.model.as_deref())),
    ]);
    table.add_row(vec![
        Cell::new("cli.sandbox.enabled"),
        Cell::new(config.cli.sandbox.enabled),
    ]);
    table.add_row(vec![
        Cell::new("cli.sandbox.env.isolate"),
        Cell::new(config.cli.sandbox.env.isolate),
    ]);
    table.add_row(vec![
        Cell::new("cli.sandbox.env.allow"),
        Cell::new(format_string_list(&config.cli.sandbox.env.allow)),
    ]);
    table.add_row(vec![
        Cell::new("cli.sandbox.env.block"),
        Cell::new(format_string_list(&config.cli.sandbox.env.block)),
    ]);
    table.add_row(vec![
        Cell::new("cli.sandbox.limits.timeout_secs"),
        Cell::new(config.cli.sandbox.limits.timeout_secs),
    ]);
    table.add_row(vec![
        Cell::new("cli.sandbox.limits.max_output_bytes"),
        Cell::new(config.cli.sandbox.limits.max_output_bytes),
    ]);
    table.add_row(vec![
        Cell::new("sources.global"),
        Cell::new(format_source_info(&sources.global)),
    ]);
    table.add_row(vec![
        Cell::new("sources.workspace"),
        Cell::new(format_source_info(&sources.workspace)),
    ]);
    table.add_row(vec![
        Cell::new("sources.write_target"),
        Cell::new(format_source_info(&sources.write_target)),
    ]);
    crate::output::table::print_table(table)
}

async fn get_config_value(
    executor: Arc<dyn CommandExecutor>,
    key: &str,
    format: OutputFormat,
) -> Result<()> {
    let config = load_effective_config_document(executor).await?;

    let value = match key {
        "system" => json!(config.system),
        "system.worker_count" => json!(config.system.worker_count),
        "system.task_timeout_seconds" => json!(config.system.task_timeout_seconds),
        "system.stall_timeout_seconds" => json!(config.system.stall_timeout_seconds),
        "system.background_api_timeout_seconds" => {
            json!(config.system.background_api_timeout_seconds)
        }
        "system.chat_response_timeout_seconds" => {
            json!(config.system.chat_response_timeout_seconds)
        }
        "system.max_retries" => json!(config.system.max_retries),
        "system.chat_session_retention_days" => json!(config.system.chat_session_retention_days),
        "system.background_task_retention_days" => {
            json!(config.system.background_task_retention_days)
        }
        "system.checkpoint_retention_days" => json!(config.system.checkpoint_retention_days),
        "system.memory_chunk_retention_days" => json!(config.system.memory_chunk_retention_days),
        "system.log_file_retention_days" => json!(config.system.log_file_retention_days),
        "system.experimental_features" => json!(config.system.experimental_features),
        "agent" => json!(config.agent),
        "agent.tool_timeout_secs" => json!(config.agent.tool_timeout_secs),
        "agent.llm_timeout_secs" => json!(config.agent.llm_timeout_secs),
        "agent.bash_timeout_secs" => json!(config.agent.bash_timeout_secs),
        "agent.python_timeout_secs" => json!(config.agent.python_timeout_secs),
        "agent.browser_timeout_secs" => json!(config.agent.browser_timeout_secs),
        "agent.process_session_ttl_secs" => json!(config.agent.process_session_ttl_secs),
        "agent.approval_timeout_secs" => json!(config.agent.approval_timeout_secs),
        "agent.max_iterations" => json!(config.agent.max_iterations),
        "agent.max_depth" => json!(config.agent.max_depth),
        "agent.subagent_timeout_secs" => json!(config.agent.subagent_timeout_secs),
        "agent.max_parallel_subagents" => json!(config.agent.max_parallel_subagents),
        "agent.max_tool_calls" => json!(config.agent.max_tool_calls),
        "agent.max_tool_concurrency" => json!(config.agent.max_tool_concurrency),
        "agent.max_tool_result_length" => json!(config.agent.max_tool_result_length),
        "agent.prune_tool_max_chars" => json!(config.agent.prune_tool_max_chars),
        "agent.compact_preserve_tokens" => json!(config.agent.compact_preserve_tokens),
        "agent.max_wall_clock_secs" => json!(config.agent.max_wall_clock_secs),
        "agent.default_task_timeout_secs" => json!(config.agent.default_task_timeout_secs),
        "agent.default_max_duration_secs" => json!(config.agent.default_max_duration_secs),
        "agent.fallback_models" => json!(config.agent.fallback_models),
        "api" => json!(config.api),
        "api.memory_search_limit" => json!(config.api.memory_search_limit),
        "api.session_list_limit" => json!(config.api.session_list_limit),
        "api.background_progress_event_limit" => {
            json!(config.api.background_progress_event_limit)
        }
        "api.background_message_list_limit" => {
            json!(config.api.background_message_list_limit)
        }
        "api.background_trace_list_limit" => {
            json!(config.api.background_trace_list_limit)
        }
        "api.background_trace_line_limit" => {
            json!(config.api.background_trace_line_limit)
        }
        "api.web_search_num_results" => json!(config.api.web_search_num_results),
        "api.diagnostics_timeout_ms" => {
            json!(config.api.diagnostics_timeout_ms)
        }
        "runtime" => json!(config.runtime),
        "runtime.background_runner_poll_interval_ms" => {
            json!(config.runtime.background_runner_poll_interval_ms)
        }
        "runtime.background_runner_max_concurrent_tasks" => {
            json!(config.runtime.background_runner_max_concurrent_tasks)
        }
        "runtime.chat_max_session_history" => {
            json!(config.runtime.chat_max_session_history)
        }
        "channel" => json!(config.channel),
        "channel.telegram_api_timeout_secs" => {
            json!(config.channel.telegram_api_timeout_secs)
        }
        "channel.telegram_polling_timeout_secs" => {
            json!(config.channel.telegram_polling_timeout_secs)
        }
        "registry" => json!(config.registry),
        "registry.github_cache_ttl_secs" => {
            json!(config.registry.github_cache_ttl_secs)
        }
        "registry.marketplace_cache_ttl_secs" => {
            json!(config.registry.marketplace_cache_ttl_secs)
        }
        "cli" => json!(config.cli),
        "cli.version" => json!(config.cli.version),
        "cli.agent" => json!(config.cli.agent),
        "cli.model" => json!(config.cli.model),
        "cli.sandbox.enabled" => json!(config.cli.sandbox.enabled),
        "cli.sandbox.env.isolate" => json!(config.cli.sandbox.env.isolate),
        "cli.sandbox.env.allow" => json!(config.cli.sandbox.env.allow),
        "cli.sandbox.env.block" => json!(config.cli.sandbox.env.block),
        "cli.sandbox.limits.timeout_secs" => json!(config.cli.sandbox.limits.timeout_secs),
        "cli.sandbox.limits.max_output_bytes" => {
            json!(config.cli.sandbox.limits.max_output_bytes)
        }
        "_effective_sources" | "effective_sources" => json!(effective_config_sources()?),
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
    // Keep CLI-only preferences local so daemon-owned config stays behind the executor boundary.
    if key.starts_with("cli.") {
        let mut config = load_global_cli_config()?;
        match key {
            "cli.version" => {
                config.version = parse_value(value)?;
            }
            "cli.agent" => {
                config.agent = parse_optional_string(value);
            }
            "cli.model" => {
                config.model = parse_optional_string(value);
            }
            "cli.sandbox.enabled" => {
                config.sandbox.enabled = parse_value(value)?;
            }
            "cli.sandbox.env.isolate" => {
                config.sandbox.env.isolate = parse_value(value)?;
            }
            "cli.sandbox.env.allow" => {
                config.sandbox.env.allow = parse_string_list(value)?;
            }
            "cli.sandbox.env.block" => {
                config.sandbox.env.block = parse_string_list(value)?;
            }
            "cli.sandbox.limits.timeout_secs" => {
                config.sandbox.limits.timeout_secs = parse_value(value)?;
            }
            "cli.sandbox.limits.max_output_bytes" => {
                config.sandbox.limits.max_output_bytes = parse_value(value)?;
            }
            _ => bail!("Unsupported config key: {key}"),
        }
        write_cli_config(&config)?;
    } else {
        let mut config = executor.get_global_config().await?;

        match key {
            "system.worker_count" => {
                config.worker_count = parse_value(value)?;
            }
            "system.task_timeout_seconds" => {
                config.task_timeout_seconds = parse_value(value)?;
            }
            "system.stall_timeout_seconds" => {
                config.stall_timeout_seconds = parse_value(value)?;
            }
            "system.background_api_timeout_seconds" => {
                config.background_api_timeout_seconds = parse_optional_u64(value)?;
            }
            "system.chat_response_timeout_seconds" => {
                config.chat_response_timeout_seconds = parse_optional_u64(value)?;
            }
            "system.max_retries" => {
                config.max_retries = parse_value(value)?;
            }
            "system.chat_session_retention_days" => {
                config.chat_session_retention_days = parse_value(value)?;
            }
            "system.background_task_retention_days" => {
                config.background_task_retention_days = parse_value(value)?;
            }
            "system.checkpoint_retention_days" => {
                config.checkpoint_retention_days = parse_value(value)?;
            }
            "system.memory_chunk_retention_days" => {
                config.memory_chunk_retention_days = parse_value(value)?;
            }
            "system.log_file_retention_days" => {
                config.log_file_retention_days = parse_value(value)?;
            }
            "system.experimental_features" => {
                config.experimental_features = parse_string_list(value)?;
            }
            "agent.tool_timeout_secs" => {
                config.agent.tool_timeout_secs = parse_value(value)?;
            }
            "agent.llm_timeout_secs" => {
                config.agent.llm_timeout_secs = parse_optional_u64(value)?;
            }
            "agent.bash_timeout_secs" => {
                config.agent.bash_timeout_secs = parse_value(value)?;
            }
            "agent.python_timeout_secs" => {
                config.agent.python_timeout_secs = parse_value(value)?;
            }
            "agent.browser_timeout_secs" => {
                config.agent.browser_timeout_secs = parse_value(value)?;
            }
            "agent.process_session_ttl_secs" => {
                config.agent.process_session_ttl_secs = parse_value(value)?;
            }
            "agent.approval_timeout_secs" => {
                config.agent.approval_timeout_secs = parse_value(value)?;
            }
            "agent.max_iterations" => {
                config.agent.max_iterations = parse_value(value)?;
            }
            "agent.max_depth" => {
                config.agent.max_depth = parse_value(value)?;
            }
            "agent.subagent_timeout_secs" => {
                config.agent.subagent_timeout_secs = parse_value(value)?;
            }
            "agent.max_parallel_subagents" => {
                config.agent.max_parallel_subagents = parse_value(value)?;
            }
            "agent.max_tool_calls" => {
                config.agent.max_tool_calls = parse_value(value)?;
            }
            "agent.max_tool_concurrency" => {
                config.agent.max_tool_concurrency = parse_value(value)?;
            }
            "agent.max_tool_result_length" => {
                config.agent.max_tool_result_length = parse_value(value)?;
            }
            "agent.prune_tool_max_chars" => {
                config.agent.prune_tool_max_chars = parse_value(value)?;
            }
            "agent.compact_preserve_tokens" => {
                config.agent.compact_preserve_tokens = parse_value(value)?;
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
                config.agent.fallback_models = parse_optional_string_list(value)?;
            }
            "api.memory_search_limit" => {
                config.api_defaults.memory_search_limit = parse_value(value)?;
            }
            "api.session_list_limit" => {
                config.api_defaults.session_list_limit = parse_value(value)?;
            }
            "api.background_progress_event_limit" => {
                config.api_defaults.background_progress_event_limit = parse_value(value)?;
            }
            "api.background_message_list_limit" => {
                config.api_defaults.background_message_list_limit = parse_value(value)?;
            }
            "api.background_trace_list_limit" => {
                config.api_defaults.background_trace_list_limit = parse_value(value)?;
            }
            "api.background_trace_line_limit" => {
                config.api_defaults.background_trace_line_limit = parse_value(value)?;
            }
            "api.web_search_num_results" => {
                config.api_defaults.web_search_num_results = parse_value(value)?;
            }
            "api.diagnostics_timeout_ms" => {
                config.api_defaults.diagnostics_timeout_ms = parse_value(value)?;
            }
            "runtime.background_runner_poll_interval_ms" => {
                config.runtime_defaults.background_runner_poll_interval_ms = parse_value(value)?;
            }
            "runtime.background_runner_max_concurrent_tasks" => {
                config
                    .runtime_defaults
                    .background_runner_max_concurrent_tasks = parse_value(value)?;
            }
            "runtime.chat_max_session_history" => {
                config.runtime_defaults.chat_max_session_history = parse_value(value)?;
            }
            "channel.telegram_api_timeout_secs" => {
                config.channel_defaults.telegram_api_timeout_secs = parse_value(value)?;
            }
            "channel.telegram_polling_timeout_secs" => {
                config.channel_defaults.telegram_polling_timeout_secs = parse_value(value)?;
            }
            "registry.github_cache_ttl_secs" => {
                config.registry_defaults.github_cache_ttl_secs = parse_value(value)?;
            }
            "registry.marketplace_cache_ttl_secs" => {
                config.registry_defaults.marketplace_cache_ttl_secs = parse_value(value)?;
            }
            _ => bail!("Unsupported config key: {key}"),
        }

        executor.set_config(config).await?;
    }

    if format.is_json() {
        return print_json(&json!({ "updated": true, "key": key }));
    }

    println!("Updated {key}");
    Ok(())
}

async fn reset_config(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let config = SystemConfig::default();
    executor.set_config(config).await?;
    write_cli_config(&CliConfig::default())?;

    if format.is_json() {
        return print_json(&json!({ "reset": true }));
    }

    println!("Global configuration reset to defaults. Workspace overrides may still apply.");
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

fn parse_optional_string(value: &str) -> Option<String> {
    let normalized = value.trim();
    if normalized.eq_ignore_ascii_case("none")
        || normalized.eq_ignore_ascii_case("null")
        || normalized.eq_ignore_ascii_case("unset")
    {
        return None;
    }
    Some(normalized.to_string())
}

fn parse_string_list(value: &str) -> Result<Vec<String>> {
    serde_json::from_str(value).map_err(|e| anyhow::anyhow!("Invalid JSON array: {}", e))
}

fn parse_optional_string_list(value: &str) -> Result<Option<Vec<String>>> {
    let normalized = value.trim();
    if normalized.eq_ignore_ascii_case("none")
        || normalized.eq_ignore_ascii_case("null")
        || normalized.eq_ignore_ascii_case("unset")
    {
        return Ok(None);
    }
    parse_string_list(normalized).map(Some)
}

fn format_optional_string(value: Option<&str>) -> String {
    value.unwrap_or("none").to_string()
}

fn format_string_list(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

async fn load_effective_config_document(
    executor: Arc<dyn CommandExecutor>,
) -> Result<ConfigDocument> {
    let system = executor.get_config().await?;
    let cli = load_cli_config()?;
    Ok(ConfigDocument::from_system_config(system, cli))
}

fn format_source_info(source: &Option<restflow_storage::ConfigSourcePathInfo>) -> String {
    match source {
        Some(info) => {
            let exists = if info.exists { "exists" } else { "missing" };
            let origin = if info.from_env { "env" } else { "default" };
            format!("{} ({exists}, {origin})", info.path)
        }
        None => "none".to_string(),
    }
}

fn format_optional_u64(value: Option<u64>) -> String {
    value
        .map(|secs| secs.to_string())
        .unwrap_or_else(|| "none".to_string())
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use crate::executor::{CommandExecutor, direct::DirectExecutor};
    use restflow_storage::{load_cli_config, load_global_cli_config};
    use std::env;
    use std::path::Path;
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
        crate::test_support::env_lock()
    }

    struct TestContext {
        executor: Arc<dyn CommandExecutor>,
        _temp_dir: tempfile::TempDir,
        _restflow_dir_guard: EnvGuard,
        _global_config_guard: EnvGuard,
        _env_lock: std::sync::MutexGuard<'static, ()>,
    }

    async fn setup_executor() -> TestContext {
        let env_guard = env_lock();
        let temp_dir = tempdir().expect("tempdir");
        let restflow_dir_guard = EnvGuard::set_path("RESTFLOW_DIR", temp_dir.path());
        let config_path = temp_dir.path().join("config.toml");
        let global_config_guard = EnvGuard::set_path("RESTFLOW_GLOBAL_CONFIG", &config_path);
        let db_path = temp_dir.path().join("restflow.db");
        let direct_executor = DirectExecutor::connect(Some(path_to_string(&db_path)))
            .await
            .expect("connect direct executor");
        let executor: Arc<dyn CommandExecutor> = Arc::new(direct_executor);

        TestContext {
            executor,
            _temp_dir: temp_dir,
            _restflow_dir_guard: restflow_dir_guard,
            _global_config_guard: global_config_guard,
            _env_lock: env_guard,
        }
    }

    fn path_to_string(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

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

    #[tokio::test]
    async fn test_set_config_supports_log_file_retention_days() {
        let ctx = setup_executor().await;

        set_config_value(
            ctx.executor.clone(),
            "system.log_file_retention_days",
            "45",
            OutputFormat::Json,
        )
        .await
        .expect("set config should succeed");

        let config = ctx.executor.get_config().await.expect("get config");
        assert_eq!(config.log_file_retention_days, 45);
    }

    #[tokio::test]
    async fn test_set_config_supports_agent_max_depth() {
        let ctx = setup_executor().await;

        set_config_value(
            ctx.executor.clone(),
            "agent.max_depth",
            "4",
            OutputFormat::Json,
        )
        .await
        .expect("set config should support agent.max_depth");

        let config = ctx.executor.get_config().await.expect("get config");
        assert_eq!(config.agent.max_depth, 4);
    }

    #[tokio::test]
    async fn test_get_config_supports_log_file_retention_days() {
        let ctx = setup_executor().await;
        let mut config = ctx
            .executor
            .get_global_config()
            .await
            .expect("get global config");
        config.log_file_retention_days = 21;
        ctx.executor
            .set_config(config)
            .await
            .expect("persist config");

        get_config_value(
            ctx.executor.clone(),
            "system.log_file_retention_days",
            OutputFormat::Json,
        )
        .await
        .expect("get config should support system.log_file_retention_days");
    }

    #[tokio::test]
    async fn test_set_config_supports_cli_agent() {
        let ctx = setup_executor().await;

        set_config_value(
            ctx.executor.clone(),
            "cli.agent",
            "planner",
            OutputFormat::Json,
        )
        .await
        .expect("set config should support cli.agent");

        let cli = load_cli_config().expect("load cli config");
        assert_eq!(cli.agent.as_deref(), Some("planner"));
    }

    #[tokio::test]
    async fn test_set_config_cli_write_preserves_workspace_overrides() {
        let ctx = setup_executor().await;
        let workspace_path = ctx._temp_dir.path().join("workspace-config.toml");
        std::fs::write(&workspace_path, "[cli]\nagent = \"workspace-agent\"\n")
            .expect("write workspace config");
        let _workspace_guard = EnvGuard::set_path("RESTFLOW_WORKSPACE_CONFIG", &workspace_path);

        set_config_value(
            ctx.executor.clone(),
            "cli.model",
            "gpt-5",
            OutputFormat::Json,
        )
        .await
        .expect("set config should support cli.model");

        let global_cli = load_global_cli_config().expect("load global cli config");
        assert_eq!(global_cli.agent, None);
        assert_eq!(global_cli.model.as_deref(), Some("gpt-5"));

        let effective_cli = load_cli_config().expect("load effective cli config");
        assert_eq!(effective_cli.agent.as_deref(), Some("workspace-agent"));
        assert_eq!(effective_cli.model.as_deref(), Some("gpt-5"));
    }

    #[tokio::test]
    async fn test_set_config_supports_clearing_agent_fallback_models() {
        let ctx = setup_executor().await;

        set_config_value(
            ctx.executor.clone(),
            "agent.fallback_models",
            "[\"glm-5\", \"gpt-5\"]",
            OutputFormat::Json,
        )
        .await
        .expect("set fallback models should succeed");

        set_config_value(
            ctx.executor.clone(),
            "agent.fallback_models",
            "null",
            OutputFormat::Json,
        )
        .await
        .expect("clearing fallback models should succeed");

        let config = ctx
            .executor
            .get_global_config()
            .await
            .expect("get global config");
        assert_eq!(config.agent.fallback_models, None);
    }

    #[tokio::test]
    async fn test_get_config_supports_effective_sources_aliases() {
        let ctx = setup_executor().await;

        get_config_value(
            ctx.executor.clone(),
            "effective_sources",
            OutputFormat::Json,
        )
        .await
        .expect("get config should support effective_sources");

        get_config_value(
            ctx.executor.clone(),
            "_effective_sources",
            OutputFormat::Json,
        )
        .await
        .expect("get config should support _effective_sources");
    }
}
