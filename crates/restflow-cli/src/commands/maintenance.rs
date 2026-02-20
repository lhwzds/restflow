use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

use crate::cli::MaintenanceCommands;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::AppCore;

pub async fn run(
    core: Arc<AppCore>,
    command: MaintenanceCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        MaintenanceCommands::Cleanup => run_cleanup(core, format).await,
    }
}

async fn run_cleanup(core: Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let report = restflow_core::services::cleanup::run_cleanup(&core).await?;

    if format.is_json() {
        return print_json(&json!({
            "chat_sessions": report.chat_sessions,
            "background_tasks": report.background_tasks,
            "checkpoints": report.checkpoints,
            "memory_chunks": report.memory_chunks,
            "memory_sessions": report.memory_sessions,
            "vector_orphans": report.vector_orphans,
            "daemon_log_files": report.daemon_log_files,
            "event_log_files": report.event_log_files
        }));
    }

    println!("Cleanup finished:");
    println!("  chat_sessions: {}", report.chat_sessions);
    println!("  background_tasks: {}", report.background_tasks);
    println!("  checkpoints: {}", report.checkpoints);
    println!("  memory_chunks: {}", report.memory_chunks);
    println!("  memory_sessions: {}", report.memory_sessions);
    println!("  vector_orphans: {}", report.vector_orphans);
    println!("  daemon_log_files: {}", report.daemon_log_files);
    println!("  event_log_files: {}", report.event_log_files);
    Ok(())
}
