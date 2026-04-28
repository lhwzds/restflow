use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

use crate::cli::MaintenanceCommands;
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: MaintenanceCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        MaintenanceCommands::Cleanup => run_cleanup(executor, format).await,
    }
}

async fn run_cleanup(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let report = executor.run_cleanup().await?;

    if format.is_json() {
        return print_json(&json!({
            "chat_sessions": report.chat_sessions,
            "background_tasks": report.background_tasks,
            "checkpoints": report.checkpoints,
            "memory_chunks": report.memory_chunks,
            "audit_events": report.audit_events,
            "telemetry_metric_samples": report.telemetry_metric_samples,
            "memory_sessions": report.memory_sessions,
            "vector_orphans": report.vector_orphans,
            "daemon_log_files": report.daemon_log_files
        }));
    }

    println!("Cleanup finished:");
    println!("  chat_sessions: {}", report.chat_sessions);
    println!("  background_tasks: {}", report.background_tasks);
    println!("  checkpoints: {}", report.checkpoints);
    println!("  memory_chunks: {}", report.memory_chunks);
    println!("  audit_events: {}", report.audit_events);
    println!(
        "  telemetry_metric_samples: {}",
        report.telemetry_metric_samples
    );
    println!("  memory_sessions: {}", report.memory_sessions);
    println!("  vector_orphans: {}", report.vector_orphans);
    println!("  daemon_log_files: {}", report.daemon_log_files);
    Ok(())
}
