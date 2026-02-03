use anyhow::Result;
use comfy_table::{Cell, Table};
use redb::Database;
use std::path::Path;
use std::sync::Arc;

use crate::cli::SecretCommands;
use crate::commands::utils::format_timestamp;
use crate::output::{json::print_json, OutputFormat};
use restflow_core::storage::MasterKeyMigrationStatus;
use restflow_core::AppCore;
use serde_json::json;

pub async fn migrate_master_key_direct(db_path: &str, format: OutputFormat) -> Result<()> {
    let db = open_database(db_path)?;
    let result = restflow_storage::secrets::migrate_master_key_from_db(&db)?;
    print_migration_result(&result, format)
}

fn open_database(db_path: &str) -> Result<Arc<Database>> {
    let path = Path::new(db_path);
    if path.exists() {
        Ok(Arc::new(Database::open(path)?))
    } else {
        Ok(Arc::new(Database::create(path)?))
    }
}

fn print_migration_result(
    result: &restflow_storage::secrets::MasterKeyMigrationResult,
    format: OutputFormat,
) -> Result<()> {
    if format.is_json() {
        return print_json(result);
    }

    match result.status {
        MasterKeyMigrationStatus::Migrated => {
            println!("Master key migrated to {}", result.path.display());
        }
        MasterKeyMigrationStatus::JsonAlreadyExists => {
            println!(
                "Master key JSON already exists at {}",
                result.path.display()
            );
        }
        MasterKeyMigrationStatus::NoDatabaseKey => {
            println!("No master key found in database.");
        }
    }

    Ok(())
}

pub async fn run(core: Arc<AppCore>, command: SecretCommands, format: OutputFormat) -> Result<()> {
    match command {
        SecretCommands::List => list_secrets(&core, format).await,
        SecretCommands::Set { key, value } => set_secret(&core, &key, &value, format).await,
        SecretCommands::Delete { key } => delete_secret(&core, &key, format).await,
        SecretCommands::Has { key } => has_secret(&core, &key, format).await,
        SecretCommands::MigrateMasterKey => migrate_master_key(&core, format).await,
    }
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
    let result = core.storage.migrate_master_key_from_db()?;
    print_migration_result(&result, format)
}
