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
