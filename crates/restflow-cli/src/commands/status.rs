use anyhow::Result;
use restflow_core::daemon::{DaemonStatus, check_daemon_status};
use restflow_core::paths;
use serde_json::json;

use crate::output::{OutputFormat, json::print_json};

pub async fn run(format: OutputFormat) -> Result<()> {
    let mut daemon_status = check_daemon_status()?;
    let mut auto_recovery = None;
    if matches!(daemon_status, DaemonStatus::Stale { .. }) {
        let report = restflow_core::daemon::recovery::recover().await?;
        if !report.is_clean() {
            auto_recovery = Some(report.to_string());
        }
        daemon_status = check_daemon_status()?;
    }
    let socket_path = paths::socket_path()?;
    let pid_path = paths::daemon_pid_path()?;
    let db_path = paths::database_path()?;

    let (status, pid, stale_pid) = match daemon_status {
        DaemonStatus::Running { pid } => ("running", Some(pid), None),
        DaemonStatus::NotRunning => ("not_running", None, None),
        DaemonStatus::Stale { pid } => ("stale", None, Some(pid)),
    };

    if format.is_json() {
        return print_json(&json!({
            "daemon_status": status,
            "pid": pid,
            "stale_pid": stale_pid,
            "auto_recovery": auto_recovery,
            "socket_path": socket_path,
            "pid_path": pid_path,
            "db_path": db_path,
        }));
    }

    println!("浮流 RestFlow Status");
    match daemon_status {
        DaemonStatus::Running { pid } => println!("Daemon: running (PID: {pid})"),
        DaemonStatus::NotRunning => println!("Daemon: not running"),
        DaemonStatus::Stale { pid } => println!("Daemon: stale pid file (PID: {pid})"),
    }
    if let Some(report) = auto_recovery {
        println!("Auto-recovery: {report}");
    }
    println!("Socket: {}", socket_path.display());
    println!("PID file: {}", pid_path.display());
    println!("DB path: {}", db_path.display());

    Ok(())
}
