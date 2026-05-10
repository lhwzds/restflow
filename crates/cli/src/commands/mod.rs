pub mod agent;
pub mod config;
pub mod daemon;
pub mod daemon_state;
pub mod secret;
pub mod session;
pub mod skill;
pub mod upgrade;
pub mod utils;

pub mod info {
    use anyhow::Result;

    pub fn run() -> Result<()> {
        println!("RestFlow CLI {}", env!("CARGO_PKG_VERSION"));
        Ok(())
    }
}

pub mod maintenance {
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
                "daemon_log_files": report.daemon_log_files
            }));
        }

        println!("Cleanup finished:");
        println!("  chat_sessions: {}", report.chat_sessions);
        println!("  tasks: {}", report.tasks);
        println!("  audit_events: {}", report.audit_events);
        println!("  daemon_log_files: {}", report.daemon_log_files);
        Ok(())
    }
}

pub mod restart {
    use crate::cli::RestartArgs;
    use crate::commands::daemon::restart_background;
    use anyhow::Result;

    pub async fn run(_args: RestartArgs) -> Result<()> {
        restart_background().await
    }
}

pub mod start {
    use crate::cli::StartArgs;
    use anyhow::Result;
    use runtime::daemon::ensure_daemon_running;

    pub async fn run(args: StartArgs) -> Result<()> {
        let _ = args;
        ensure_daemon_running().await?;
        println!("RestFlow daemon started");
        Ok(())
    }
}

pub mod status {
    use anyhow::Result;
    use serde_json::json;

    use crate::commands::daemon_state::{self, EffectiveDaemonStatus, RunningSource};
    use crate::output::{OutputFormat, json::print_json};

    pub async fn run(format: OutputFormat) -> Result<()> {
        let snapshot = daemon_state::collect_daemon_status_snapshot(true).await?;

        let (status, pid, stale_pid, running_source) = match snapshot.daemon_status {
            EffectiveDaemonStatus::Running { pid, source } => {
                ("running", pid, None, Some(source.as_str()))
            }
            EffectiveDaemonStatus::NotRunning => ("not_running", None, None, None),
            EffectiveDaemonStatus::Stale { pid } => ("stale", None, Some(pid), None),
        };

        if format.is_json() {
            return print_json(&json!({
                "daemon_status": status,
                "pid": pid,
                "stale_pid": stale_pid,
                "running_source": running_source,
                "auto_recovery": snapshot.auto_recovery,
                "socket_path": snapshot.socket_path,
                "pid_path": snapshot.pid_path,
                "db_path": snapshot.db_path,
            }));
        }

        println!("RestFlow Status");
        match snapshot.daemon_status {
            EffectiveDaemonStatus::Running {
                pid: Some(pid),
                source,
            } => {
                if source == RunningSource::PidFile {
                    println!("Daemon: running (PID: {pid})");
                } else {
                    println!("Daemon: running (PID: {pid}, source: {})", source.as_str());
                }
            }
            EffectiveDaemonStatus::Running { pid: None, source } => {
                println!(
                    "Daemon: running (PID: unknown, source: {})",
                    source.as_str()
                );
            }
            EffectiveDaemonStatus::NotRunning => println!("Daemon: not running"),
            EffectiveDaemonStatus::Stale { pid } => println!("Daemon: stale pid file (PID: {pid})"),
        }
        if let Some(report) = snapshot.auto_recovery {
            println!("Auto-recovery: {report}");
        }
        println!("Socket: {}", snapshot.socket_path.display());
        println!("PID file: {}", snapshot.pid_path.display());
        println!("DB path: {}", snapshot.db_path.display());

        Ok(())
    }
}

pub mod stop {
    use anyhow::Result;
    use runtime::daemon::stop_daemon;

    pub async fn run() -> Result<()> {
        if stop_daemon()? {
            println!("RestFlow daemon stopped");
        } else {
            println!("RestFlow daemon not running");
        }
        Ok(())
    }
}
