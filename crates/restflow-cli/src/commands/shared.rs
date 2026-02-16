use anyhow::Result;
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::{OutputFormat, SharedCommands};
use crate::executor::CommandExecutor;
use crate::output::json::print_json;
use crate::output::table::print_table;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: SharedCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        SharedCommands::List { namespace } => list_shared(executor, namespace.as_deref(), format).await,
        SharedCommands::Get { key } => get_shared(executor, &key, format).await,
        SharedCommands::Set { key, value, visibility } => {
            set_shared(executor, &key, &value, &visibility, format).await
        }
        SharedCommands::Delete { key } => delete_shared(executor, &key, format).await,
    }
}

async fn list_shared(
    executor: Arc<dyn CommandExecutor>,
    namespace: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let entries = executor.list_shared_space(namespace).await?;

    if format.is_json() {
        return print_json(&entries);
    }

    if entries.is_empty() {
        println!("No shared space entries found.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["Key", "Visibility", "Updated"]);

    for entry in entries {
        table.add_row(vec![
            Cell::new(entry.key),
            Cell::new(format!("{:?}", entry.visibility).to_lowercase()),
            Cell::new(format_timestamp(entry.updated_at)),
        ]);
    }

    print_table(table)
}

async fn get_shared(
    executor: Arc<dyn CommandExecutor>,
    key: &str,
    format: OutputFormat,
) -> Result<()> {
    let entry = executor
        .get_shared_space(key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Entry not found: {}", key))?;

    if format.is_json() {
        return print_json(&entry);
    }

    println!("Key:         {}", entry.key);
    println!("Visibility:  {:?}", entry.visibility);
    println!("Created:     {}", format_timestamp(entry.created_at));
    println!("Updated:     {}", format_timestamp(entry.updated_at));
    if let Some(owner) = &entry.owner {
        println!("Owner:       {}", owner);
    }
    if !entry.tags.is_empty() {
        println!("Tags:        {}", entry.tags.join(", "));
    }
    println!("\nValue:");
    println!("{}", entry.value);

    Ok(())
}

async fn set_shared(
    executor: Arc<dyn CommandExecutor>,
    key: &str,
    value: &str,
    visibility: &str,
    format: OutputFormat,
) -> Result<()> {
    let entry = executor.set_shared_space(key, value, visibility).await?;

    if format.is_json() {
        return print_json(&entry);
    }

    println!("Entry set: {} ({})", key, visibility);
    Ok(())
}

async fn delete_shared(
    executor: Arc<dyn CommandExecutor>,
    key: &str,
    format: OutputFormat,
) -> Result<()> {
    let deleted = executor.delete_shared_space(key).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({"deleted": deleted}));
    }

    if deleted {
        println!("Entry deleted: {}", key);
    } else {
        println!("Entry not found: {}", key);
    }

    Ok(())
}

fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ts.to_string())
}
