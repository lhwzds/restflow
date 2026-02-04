use crate::cli::StartArgs;
use anyhow::Result;
use restflow_core::daemon::{DaemonConfig, ensure_daemon_running_with_config, stop_daemon};
use restflow_core::paths;
use std::process::Command;
use std::time::Duration;
use tokio::signal;

pub async fn run(args: StartArgs) -> Result<()> {
    let http_port = args.port.or(Some(3000));
    let config = DaemonConfig {
        http: args.http,
        http_port,
    };

    ensure_daemon_running_with_config(config).await?;
    wait_for_daemon_ready().await?;

    if args.http && !args.no_browser {
        let url = format!("http://localhost:{}", http_port.unwrap_or(3000));
        let _ = open_browser(&url);
    }

    print_startup_banner(args.http, http_port);
    wait_for_shutdown_signal().await;
    let _ = stop_daemon();
    Ok(())
}

fn print_startup_banner(http_enabled: bool, port: Option<u16>) {
    println!("RestFlow is running!");
    if http_enabled && let Some(port) = port {
        println!("API: http://localhost:{}", port);
    }
    println!("Logs: ~/.restflow/logs/");
    println!();
    println!("Press Ctrl+C to stop...");
}

async fn wait_for_daemon_ready() -> Result<()> {
    let socket_path = paths::socket_path()?;
    for _ in 0..50 {
        if restflow_core::daemon::is_daemon_available(&socket_path).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!("Daemon did not become ready in time");
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to register SIGTERM handler");
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = signal::ctrl_c() => {},
        }
    }

    #[cfg(not(unix))]
    {
        let _ = signal::ctrl_c().await;
    }
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn()?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(url).spawn()?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd").args(["/C", "start", url]).spawn()?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    {
        let _ = url;
        Ok(())
    }
}
