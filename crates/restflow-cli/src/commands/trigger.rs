use anyhow::Result;
use std::sync::Arc;

use crate::cli::{OutputFormat, TriggerCommands};
use crate::executor::CommandExecutor;
use crate::output::json::print_json;

pub async fn run(
    _executor: Arc<dyn CommandExecutor>,
    command: TriggerCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        TriggerCommands::List => list_triggers(format).await,
        TriggerCommands::Create { name, trigger_type } => {
            create_trigger(&name, &trigger_type, format).await
        }
        TriggerCommands::Delete { id } => delete_trigger(&id, format).await,
    }
}

async fn list_triggers(format: OutputFormat) -> Result<()> {
    if format.is_json() {
        return print_json(&serde_json::json!([]));
    }

    println!("Trigger management is not yet implemented.");
    println!("Use MCP tools for trigger operations.");
    Ok(())
}

async fn create_trigger(name: &str, trigger_type: &str, format: OutputFormat) -> Result<()> {
    if format.is_json() {
        return print_json(&serde_json::json!({
            "name": name,
            "type": trigger_type
        }));
    }

    println!(
        "Trigger '{}' of type '{}' creation is not yet implemented.",
        name, trigger_type
    );
    println!("Use MCP tools for trigger operations.");
    Ok(())
}

async fn delete_trigger(id: &str, format: OutputFormat) -> Result<()> {
    if format.is_json() {
        return print_json(&serde_json::json!({"deleted": false}));
    }

    println!("Trigger {} deletion is not yet implemented.", id);
    println!("Use MCP tools for trigger operations.");
    Ok(())
}
