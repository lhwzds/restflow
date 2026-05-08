use anyhow::Result;
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::SecretCommands;
use crate::commands::utils::format_timestamp;
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use serde_json::json;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: SecretCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        SecretCommands::List => list_secrets(executor, format).await,
        SecretCommands::Set { key, value } => set_secret(executor, &key, &value, format).await,
        SecretCommands::Delete { key } => delete_secret(executor, &key, format).await,
        SecretCommands::Has { key } => has_secret(executor, &key, format).await,
    }
}

async fn list_secrets(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let secrets = executor.list_secrets().await?;

    if format.is_json() {
        return print_json(&secrets);
    }

    let mut table = Table::new();
    table.set_header(vec!["Key", "Updated"]);

    for secret in secrets {
        table.add_row(vec![
            Cell::new(secret.key),
            Cell::new(format_timestamp(Some(secret.updated_at))),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn set_secret(
    executor: Arc<dyn CommandExecutor>,
    key: &str,
    value: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.set_secret(key, value, None).await?;

    if format.is_json() {
        return print_json(&json!({ "set": true, "key": key }));
    }

    println!("Secret set: {key}");
    Ok(())
}

async fn delete_secret(
    executor: Arc<dyn CommandExecutor>,
    key: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.delete_secret(key).await?;

    if format.is_json() {
        return print_json(&json!({ "deleted": true, "key": key }));
    }

    println!("Secret deleted: {key}");
    Ok(())
}

async fn has_secret(
    executor: Arc<dyn CommandExecutor>,
    key: &str,
    format: OutputFormat,
) -> Result<()> {
    let exists = executor.has_secret(key).await?;

    if format.is_json() {
        return print_json(&json!({ "key": key, "exists": exists }));
    }

    if exists {
        println!("Secret exists: {key}");
    } else {
        println!("Secret not found: {key}");
    }
    Ok(())
}
