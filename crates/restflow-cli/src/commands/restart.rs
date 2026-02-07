use crate::cli::RestartArgs;
use crate::commands::claude_mcp::try_sync_restflow_stdio_mcp;
use anyhow::{Result, bail};
use restflow_core::daemon::{DaemonStatus, check_daemon_status, start_daemon, stop_daemon};
use tokio::time::{Duration, sleep};

pub async fn run(args: RestartArgs) -> Result<()> {
    let _ = args;

    let was_running = stop_daemon()?;
    if was_running {
        wait_for_daemon_exit().await?;
    }

    let pid = start_daemon()?;
    if was_running {
        println!("RestFlow daemon restarted (PID: {pid})");
    } else {
        println!("RestFlow daemon started (PID: {pid})");
    }
    if let Err(err) = try_sync_restflow_stdio_mcp().await {
        eprintln!("Warning: failed to auto-configure Claude MCP: {err}");
    }

    Ok(())
}

async fn wait_for_daemon_exit() -> Result<()> {
    for _ in 0..50 {
        match check_daemon_status()? {
            DaemonStatus::Running { .. } => sleep(Duration::from_millis(100)).await,
            DaemonStatus::NotRunning | DaemonStatus::Stale { .. } => return Ok(()),
        }
    }

    bail!("Daemon did not stop within timeout")
}
