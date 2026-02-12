use crate::cli::DaemonCommands;
use crate::commands::claude_mcp::try_sync_claude_http_mcp;
use crate::commands::codex_mcp::try_sync_codex_http_mcp;
use crate::daemon::CliBackgroundAgentRunner;
use anyhow::Result;
use restflow_core::AppCore;
use restflow_core::daemon::{
    DaemonConfig, DaemonStatus, IpcServer, check_daemon_status, start_daemon_with_config,
    stop_daemon,
};
use restflow_core::paths;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};

#[cfg(unix)]
use nix::libc;

const MCP_BIND_ADDR_ENV: &str = "RESTFLOW_MCP_BIND_ADDR";

pub async fn sync_mcp_configs(mcp_port: Option<u16>) {
    if let Err(err) = try_sync_claude_http_mcp(mcp_port.unwrap_or(8787)).await {
        eprintln!("Warning: failed to auto-configure Claude MCP: {err}");
    }
    if let Err(err) = try_sync_codex_http_mcp(mcp_port.unwrap_or(8787)).await {
        eprintln!("Warning: failed to auto-configure Codex MCP: {err}");
    }
}

pub async fn restart_background(mcp_port: Option<u16>) -> Result<()> {
    let config = DaemonConfig {
        mcp: true,
        mcp_port,
    };

    let was_running = stop_daemon()?;
    if was_running {
        println!("Sent stop signal to daemon");
        wait_for_daemon_exit().await?;
    }

    // Clean stale artifacts that may remain after an unclean shutdown.
    let report = restflow_core::daemon::recovery::recover().await?;
    if !report.is_clean() {
        println!("{}", report);
    }

    sync_mcp_configs(mcp_port).await;

    let pid = start_daemon_with_config(config)?;
    if was_running {
        println!("Daemon restarted (PID: {})", pid);
    } else {
        println!("Daemon started (PID: {})", pid);
    }
    Ok(())
}

pub async fn run(core: Arc<AppCore>, command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Start {
            foreground,
            mcp_port,
        } => start(core, foreground, mcp_port).await,
        DaemonCommands::Restart {
            foreground,
            mcp_port,
        } => restart(core, foreground, mcp_port).await,
        DaemonCommands::Stop => stop().await,
        DaemonCommands::Status => status().await,
    }
}

async fn start(core: Arc<AppCore>, foreground: bool, mcp_port: Option<u16>) -> Result<()> {
    let config = DaemonConfig {
        mcp: true,
        mcp_port,
    };

    sync_mcp_configs(mcp_port).await;

    if foreground {
        // In foreground mode, clean stale artifacts before binding.
        let report = restflow_core::daemon::recovery::recover().await?;
        if !report.is_clean() {
            println!("{}", report);
        }
        run_daemon(core, config).await
    } else {
        match check_daemon_status()? {
            DaemonStatus::Running { pid } => {
                println!("Daemon already running (PID: {})", pid);
                Ok(())
            }
            _ => {
                // Clean stale artifacts (e.g. leftover socket) before spawning.
                let report = restflow_core::daemon::recovery::recover().await?;
                if !report.is_clean() {
                    println!("{}", report);
                }
                let pid = start_daemon_with_config(config)?;
                println!("Daemon started (PID: {})", pid);
                Ok(())
            }
        }
    }
}

async fn restart(core: Arc<AppCore>, foreground: bool, mcp_port: Option<u16>) -> Result<()> {
    if foreground {
        let config = DaemonConfig {
            mcp: true,
            mcp_port,
        };
        let was_running = stop_daemon()?;
        if was_running {
            println!("Sent stop signal to daemon");
            wait_for_daemon_exit().await?;
        }
        // Clean stale artifacts that may remain after an unclean shutdown.
        let report = restflow_core::daemon::recovery::recover().await?;
        if !report.is_clean() {
            println!("{}", report);
        }
        sync_mcp_configs(mcp_port).await;
        run_daemon(core, config).await
    } else {
        restart_background(mcp_port).await
    }
}

async fn run_daemon(core: Arc<AppCore>, config: DaemonConfig) -> Result<()> {
    #[cfg(unix)]
    configure_nofile_limit();

    let pid_path = paths::daemon_pid_path()?;
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

    #[cfg(unix)]
    {
        let shutdown_tx = shutdown_tx.clone();
        tokio::spawn(async move {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

            tokio::select! {
                _ = sigterm.recv() => {
                    let _ = shutdown_tx.send(());
                }
                _ = tokio::signal::ctrl_c() => {
                    let _ = shutdown_tx.send(());
                }
            }
        });
    }

    #[cfg(not(unix))]
    {
        let shutdown_tx = shutdown_tx.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = shutdown_tx.send(());
        });
    }

    let socket_path = paths::socket_path()?;
    let ipc_server = IpcServer::new(core.clone(), socket_path);
    let ipc_shutdown = shutdown_tx.subscribe();
    let ipc_handle = tokio::spawn(async move {
        if let Err(err) = ipc_server.run(ipc_shutdown).await {
            error!(error = %err, "IPC server stopped unexpectedly");
        }
    });

    // MCP server is always enabled
    let mcp_bind_addr = resolve_mcp_bind_addr();
    let addr = std::net::SocketAddr::new(mcp_bind_addr, config.mcp_port.unwrap_or(8787));
    let mcp_shutdown = shutdown_tx.subscribe();
    let mcp_core = core.clone();
    let mcp_handle = tokio::spawn(async move {
        if let Err(err) =
            restflow_core::daemon::run_mcp_http_server(mcp_core, addr, mcp_shutdown).await
        {
            error!(error = %err, "MCP server stopped unexpectedly");
        }
    });

    let mut runner = CliBackgroundAgentRunner::new(core);
    if let Err(err) = runner.start().await {
        error!(error = %err, "Task runner failed to start; continuing without runner");
    }

    println!("Daemon running. Press Ctrl+C to stop.");

    let mut shutdown_rx = shutdown_tx.subscribe();
    let _ = shutdown_rx.recv().await;

    runner.stop().await?;
    let _ = std::fs::remove_file(&pid_path);
    let _ = ipc_handle.await;
    let _ = mcp_handle.await;

    println!("Daemon stopped");
    Ok(())
}

#[cfg(unix)]
fn configure_nofile_limit() {
    const TARGET_NOFILE: libc::rlim_t = 8192;

    let mut limits = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };

    // SAFETY: `limits` points to initialized writable memory and `RLIMIT_NOFILE`
    // is a valid resource kind on Unix.
    let got_limits = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) };
    if got_limits != 0 {
        warn!(
            errno = std::io::Error::last_os_error().to_string(),
            "Failed to read RLIMIT_NOFILE"
        );
        return;
    }

    let hard_cap = if limits.rlim_max == libc::RLIM_INFINITY {
        TARGET_NOFILE
    } else {
        limits.rlim_max.min(TARGET_NOFILE)
    };

    if limits.rlim_cur >= hard_cap {
        return;
    }

    let desired = libc::rlimit {
        rlim_cur: hard_cap,
        rlim_max: limits.rlim_max,
    };

    // SAFETY: `desired` contains valid values derived from current rlimit.
    let set_limits = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &desired) };
    if set_limits == 0 {
        info!(
            previous_soft = limits.rlim_cur,
            new_soft = hard_cap,
            hard = limits.rlim_max,
            "Raised RLIMIT_NOFILE soft limit for daemon process"
        );
    } else {
        warn!(
            errno = std::io::Error::last_os_error().to_string(),
            requested_soft = hard_cap,
            hard = limits.rlim_max,
            "Failed to raise RLIMIT_NOFILE soft limit"
        );
    }
}

async fn stop() -> Result<()> {
    if stop_daemon()? {
        println!("Sent stop signal to daemon");
    } else {
        println!("Daemon not running");
    }
    Ok(())
}

async fn status() -> Result<()> {
    let pid_path = paths::daemon_pid_path()?;
    let socket_path = paths::socket_path()?;
    let stale_state = restflow_core::daemon::recovery::inspect(&pid_path, &socket_path).await?;

    match check_daemon_status()? {
        DaemonStatus::Running { pid } => {
            println!("Daemon running (PID: {})", pid);
        }
        DaemonStatus::NotRunning => {
            println!("Daemon not running");
            if stale_state == restflow_core::daemon::recovery::StaleState::StaleSocket {
                println!("  Note: stale socket detected (run `daemon start` to auto-clean)");
            }
        }
        DaemonStatus::Stale { pid } => {
            println!("Daemon not running (stale PID: {})", pid);
            if matches!(
                stale_state,
                restflow_core::daemon::recovery::StaleState::Both
                    | restflow_core::daemon::recovery::StaleState::StaleSocket
            ) {
                println!("  Note: stale socket also detected");
            }
            println!("  Hint: run `daemon start` or `daemon restart` to auto-clean");
        }
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

    anyhow::bail!("Daemon did not stop within timeout")
}

fn resolve_mcp_bind_addr() -> IpAddr {
    match std::env::var(MCP_BIND_ADDR_ENV) {
        Ok(value) => parse_mcp_bind_addr(Some(&value)).unwrap_or_else(|| {
            warn!(
                env = MCP_BIND_ADDR_ENV,
                value = %value,
                "Invalid MCP bind address, falling back to 127.0.0.1"
            );
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        }),
        Err(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}

fn parse_mcp_bind_addr(value: Option<&str>) -> Option<IpAddr> {
    value.and_then(|v| v.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::parse_mcp_bind_addr;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn parse_mcp_bind_addr_accepts_ipv4() {
        let ip = parse_mcp_bind_addr(Some("0.0.0.0"));
        assert_eq!(ip, Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
    }

    #[test]
    fn parse_mcp_bind_addr_accepts_ipv6() {
        let ip = parse_mcp_bind_addr(Some("::1"));
        assert_eq!(ip, Some(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn parse_mcp_bind_addr_rejects_invalid_value() {
        let ip = parse_mcp_bind_addr(Some("not-an-ip"));
        assert_eq!(ip, None);
    }
}
