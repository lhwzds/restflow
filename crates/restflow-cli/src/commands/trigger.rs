use anyhow::Result;
use std::sync::Arc;

use crate::cli::TriggerCommands;
use crate::executor::CommandExecutor;

pub async fn run(
    _executor: Arc<dyn CommandExecutor>,
    command: TriggerCommands,
    _format: crate::cli::OutputFormat,
) -> Result<()> {
    match command {
        TriggerCommands::List | TriggerCommands::Create { .. } | TriggerCommands::Delete { .. } => {
            anyhow::bail!(
                "Trigger management is not yet implemented. Use MCP tools for trigger operations."
            )
        }
    }
}
