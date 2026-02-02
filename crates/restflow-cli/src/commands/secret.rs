use anyhow::Result;
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::SecretCommands;
use crate::commands::utils::format_timestamp;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::{storage::migrate_master_key_from_db_path, AppCore};
use serde_json::json;

pub async fn run(core: Arc<AppCore>, command: SecretCommands, format: OutputFormat) -> Result<()> {
    match command {
        SecretCommands::List => list_secrets(&core, format).await,
        SecretCommands::Set { key, value } => set_secret(&core, &key, &value, format).await,
        SecretCommands::Delete { key } => delete_secret(&core, &key, format).await,
        SecretCommands::Has { key } => has_secret(&core, &key, format).await,
        SecretCommands::MigrateMasterKey => migrate_master_key(&core, format).await,
    }
}

pub async fn migrate_master_key_with_path(db_path: &str, format: OutputFormat) -> Result<()> {
    let path = migrate_master_key_from_db_path(db_path)?;

    if format.is_json() {
        return print_json(&json!({ "migrated": true, "path": path }));
    }

    println!("Master key migrated to {}", path.to_string_lossy());
    Ok(())
}

async fn list_secrets(core: &Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let secrets = core.storage.secrets.list_secrets()?;

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
    core: &Arc<AppCore>,
    key: &str,
    value: &str,
    format: OutputFormat,
) -> Result<()> {
    core.storage.secrets.set_secret(key, value, None)?;

    if format.is_json() {
        return print_json(&json!({ "set": true, "key": key }));
    }

    println!("Secret set: {key}");
    Ok(())
}

async fn delete_secret(core: &Arc<AppCore>, key: &str, format: OutputFormat) -> Result<()> {
    core.storage.secrets.delete_secret(key)?;

    if format.is_json() {
        return print_json(&json!({ "deleted": true, "key": key }));
    }

    println!("Secret deleted: {key}");
    Ok(())
}

async fn has_secret(core: &Arc<AppCore>, key: &str, format: OutputFormat) -> Result<()> {
    let exists = core.storage.secrets.get_secret(key)?.is_some();

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

async fn migrate_master_key(core: &Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let path = core.storage.secrets.migrate_master_key_from_db()?;

    if format.is_json() {
        return print_json(&json!({ "migrated": true, "path": path }));
    }

    println!("Master key migrated to {}", path.to_string_lossy());
    Ok(())
}
