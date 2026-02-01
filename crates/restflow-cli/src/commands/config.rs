use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use serde_json::json;
use std::sync::Arc;

use crate::cli::ConfigCommands;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::AppCore;
use restflow_core::services::config as config_service;
use restflow_core::storage::SystemConfig;

pub async fn run(core: Arc<AppCore>, command: ConfigCommands, format: OutputFormat) -> Result<()> {
    match command {
        ConfigCommands::Show => show_config(&core, format).await,
        ConfigCommands::Get { key } => get_config_value(&core, &key, format).await,
        ConfigCommands::Set { key, value } => set_config_value(&core, &key, &value, format).await,
        ConfigCommands::Reset => reset_config(&core, format).await,
    }
}

async fn show_config(core: &Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let config = config_service::get_config(core).await?;

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
        Cell::new("max_retries"),
        Cell::new(config.max_retries),
    ]);

    crate::output::table::print_table(table)
}

async fn get_config_value(core: &Arc<AppCore>, key: &str, format: OutputFormat) -> Result<()> {
    let config = config_service::get_config(core).await?;

    let value = match key {
        "worker_count" => json!(config.worker_count),
        "task_timeout_seconds" => json!(config.task_timeout_seconds),
        "stall_timeout_seconds" => json!(config.stall_timeout_seconds),
        "max_retries" => json!(config.max_retries),
        _ => bail!("Unsupported config key: {key}"),
    };

    if format.is_json() {
        return print_json(&json!({ "key": key, "value": value }));
    }

    println!("{key} = {value}");
    Ok(())
}

async fn set_config_value(
    core: &Arc<AppCore>,
    key: &str,
    value: &str,
    format: OutputFormat,
) -> Result<()> {
    let mut config = config_service::get_config(core).await?;

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
        "max_retries" => {
            config.max_retries = parse_value(value)?;
        }
        _ => bail!("Unsupported config key: {key}"),
    }

    config_service::update_config(core, config).await?;

    if format.is_json() {
        return print_json(&json!({ "updated": true, "key": key }));
    }

    println!("Updated {key}");
    Ok(())
}

async fn reset_config(core: &Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let config = SystemConfig::default();
    config_service::update_config(core, config).await?;

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
