use crate::daemon::ipc_client;
use crate::daemon::process::{DaemonConfig, ProcessManager};
use crate::paths;
use anyhow::Result;
#[cfg(not(unix))]
use std::process::Command;
use std::time::Duration;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, PartialEq)]
pub enum DaemonStatus {
    Running { pid: u32 },
    NotRunning,
    Stale { pid: u32 },
}

pub fn check_daemon_status() -> Result<DaemonStatus> {
    let pid_path = paths::daemon_pid_path()?;
    if !pid_path.exists() {
        return Ok(DaemonStatus::NotRunning);
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str.trim().parse()?;

    if is_process_alive(pid) {
        Ok(DaemonStatus::Running { pid })
    } else {
        let _ = std::fs::remove_file(&pid_path);
        Ok(DaemonStatus::Stale { pid })
    }
}

pub fn start_daemon() -> Result<u32> {
    start_daemon_with_config(DaemonConfig::default())
}

pub fn start_daemon_with_config(config: DaemonConfig) -> Result<u32> {
    let manager = ProcessManager::new()?;
    let pid = manager.start(config)?;
    info!(pid, "Daemon started in background");
    Ok(pid)
}

pub fn stop_daemon() -> Result<bool> {
    match check_daemon_status()? {
        DaemonStatus::Running { pid } => {
            #[cfg(unix)]
            {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;
                kill(Pid::from_raw(pid as i32), Signal::SIGTERM)?;
            }

            #[cfg(not(unix))]
            {
                Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output()?;
            }

            info!(pid, "Sent stop signal to daemon");
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub async fn ensure_daemon_running() -> Result<()> {
    ensure_daemon_running_with_config(DaemonConfig::default()).await
}

pub async fn ensure_daemon_running_with_config(config: DaemonConfig) -> Result<()> {
    let socket_path = paths::socket_path()?;
    if ipc_client::is_daemon_available(&socket_path).await {
        debug!("Daemon already running");
        return Ok(());
    }

    match check_daemon_status()? {
        DaemonStatus::Running { pid } => {
            debug!(pid, "Daemon process exists, waiting for socket");
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if ipc_client::is_daemon_available(&socket_path).await {
                    return Ok(());
                }
            }
            warn!("Daemon running but socket unavailable");
        }
        DaemonStatus::NotRunning | DaemonStatus::Stale { .. } => {
            info!("Starting daemon automatically");
            start_daemon_with_config(config)?;
            for _ in 0..600 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if ipc_client::is_daemon_available(&socket_path).await {
                    info!("Daemon started successfully");
                    return Ok(());
                }
            }
            anyhow::bail!("Daemon failed to start within timeout");
        }
    }

    Ok(())
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        kill(Pid::from_raw(pid as i32), None).is_ok()
    }

    #[cfg(not(unix))]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}
