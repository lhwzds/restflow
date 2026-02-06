use crate::cli::DaemonCommands;
use crate::daemon::CliTaskRunner;
use anyhow::Result;
use restflow_core::AppCore;
use restflow_core::daemon::{
    DaemonConfig, DaemonStatus, HttpConfig, HttpServer, IpcServer, check_daemon_status,
    start_daemon_with_config, stop_daemon,
};
use restflow_core::paths;
use std::sync::Arc;
use tracing::error;

pub async fn run(core: Arc<AppCore>, command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Start {
            foreground,
            http,
            port,
            mcp,
            mcp_port,
        } => start(core, foreground, http, port, mcp, mcp_port).await,
        DaemonCommands::Stop => stop().await,
        DaemonCommands::Status => status().await,
    }
}

async fn start(
    core: Arc<AppCore>,
    foreground: bool,
    http: bool,
    port: Option<u16>,
    mcp: bool,
    mcp_port: Option<u16>,
) -> Result<()> {
    let config = DaemonConfig {
        http,
        http_port: port,
        mcp,
        mcp_port,
    };

    if foreground {
        run_daemon(core, config).await
    } else {
        match check_daemon_status()? {
            DaemonStatus::Running { pid } => {
                println!("Daemon already running (PID: {})", pid);
                Ok(())
            }
            _ => {
                let pid = start_daemon_with_config(config)?;
                println!("Daemon started (PID: {})", pid);
                Ok(())
            }
        }
    }
}

async fn run_daemon(core: Arc<AppCore>, config: DaemonConfig) -> Result<()> {
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

    let http_handle = if config.http {
        let http_config = HttpConfig {
            port: config.http_port.unwrap_or(3000),
            ..HttpConfig::default()
        };
        let http_server = HttpServer::new(http_config, core.clone());
        let http_shutdown = shutdown_tx.subscribe();
        Some(tokio::spawn(async move {
            if let Err(err) = http_server.run(http_shutdown).await {
                error!(error = %err, "HTTP server stopped unexpectedly");
            }
        }))
    } else {
        None
    };

    let mcp_handle = if config.mcp {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], config.mcp_port.unwrap_or(8787)));
        let mcp_shutdown = shutdown_tx.subscribe();
        let mcp_core = core.clone();
        Some(tokio::spawn(async move {
            if let Err(err) =
                restflow_core::daemon::run_mcp_http_server(mcp_core, addr, mcp_shutdown).await
            {
                error!(error = %err, "MCP server stopped unexpectedly");
            }
        }))
    } else {
        None
    };

    let mut runner = CliTaskRunner::new(core);
    if let Err(err) = runner.start().await {
        error!(error = %err, "Task runner failed to start; continuing without runner");
    }

    println!("Daemon running. Press Ctrl+C to stop.");

    let mut shutdown_rx = shutdown_tx.subscribe();
    let _ = shutdown_rx.recv().await;

    runner.stop().await?;
    let _ = std::fs::remove_file(&pid_path);
    let _ = ipc_handle.await;
    if let Some(handle) = http_handle {
        let _ = handle.await;
    }
    if let Some(handle) = mcp_handle {
        let _ = handle.await;
    }

    println!("Daemon stopped");
    Ok(())
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
    match check_daemon_status()? {
        DaemonStatus::Running { pid } => {
            println!("Daemon running (PID: {})", pid);
        }
        DaemonStatus::NotRunning => {
            println!("Daemon not running");
        }
        DaemonStatus::Stale { pid } => {
            println!("Daemon not running (stale PID: {})", pid);
        }
    }
    Ok(())
}
