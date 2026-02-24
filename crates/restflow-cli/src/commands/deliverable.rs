use anyhow::Result;
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::{DeliverableCommands, OutputFormat};
use crate::commands::utils::format_timestamp;
use crate::executor::CommandExecutor;
use crate::output::json::print_json;
use crate::output::table::print_table;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: DeliverableCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        DeliverableCommands::List { task } => list_deliverables(executor, &task, format).await,
    }
}

async fn list_deliverables(
    executor: Arc<dyn CommandExecutor>,
    task_id: &str,
    format: OutputFormat,
) -> Result<()> {
    let deliverables = executor.list_deliverables(task_id).await?;

    if format.is_json() {
        return print_json(&deliverables);
    }

    if deliverables.is_empty() {
        println!("No deliverables found for task: {}", task_id);
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Type", "Title", "Created"]);

    for d in deliverables {
        let short_id = &d.id[..8.min(d.id.len())];
        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(format!("{:?}", d.deliverable_type).to_lowercase()),
            Cell::new(&d.title),
            Cell::new(format_timestamp(Some(d.created_at))),
        ]);
    }

    print_table(table)
}
