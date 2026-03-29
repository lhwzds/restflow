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
        MaintenanceCommands::MigrateSessionSources { dry_run } => {
            run_migrate_session_sources(executor, format, dry_run).await
        }
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
    println!("  memory_sessions: {}", report.memory_sessions);
    println!("  vector_orphans: {}", report.vector_orphans);
    println!("  daemon_log_files: {}", report.daemon_log_files);
    Ok(())
}

async fn run_migrate_session_sources(
    executor: Arc<dyn CommandExecutor>,
    format: OutputFormat,
    dry_run: bool,
) -> Result<()> {
    let stats = executor.migrate_session_sources(dry_run).await?;

    if format.is_json() {
        return print_json(&json!({
            "dry_run": stats.dry_run,
            "scanned": stats.scanned,
            "migrated": stats.migrated,
            "skipped": stats.skipped,
            "failed": stats.failed
        }));
    }

    if dry_run {
        println!("Session source migration dry run:");
    } else {
        println!("Session source migration completed:");
    }
    println!("  scanned: {}", stats.scanned);
    println!("  migrated: {}", stats.migrated);
    println!("  skipped: {}", stats.skipped);
    println!("  failed: {}", stats.failed);
    Ok(())
}
