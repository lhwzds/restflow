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
            "tasks": report.tasks,
            "audit_events": report.audit_events,
            "telemetry_metric_samples": report.telemetry_metric_samples,
            "daemon_log_files": report.daemon_log_files
        }));
    }

    println!("Cleanup finished:");
    println!("  chat_sessions: {}", report.chat_sessions);
    println!("  tasks: {}", report.tasks);
    println!("  audit_events: {}", report.audit_events);
    println!(
        "  telemetry_metric_samples: {}",
        report.telemetry_metric_samples
    );
    println!("  daemon_log_files: {}", report.daemon_log_files);
    Ok(())
}
