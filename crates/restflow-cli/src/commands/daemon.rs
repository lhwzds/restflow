use crate::cli::DaemonCommands;
use crate::daemon::{cleanup_stale_pid, pid_file, read_pid, CliTaskRunner};
use anyhow::Result;
use restflow_core::AppCore;
use std::process::{Command, Stdio};
use std::sync::Arc;

pub async fn run(core: Arc<AppCore>, command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Start { foreground } => start(core, foreground).await,
        DaemonCommands::Stop => stop().await,
        DaemonCommands::Status => status().await,
    }
}

async fn start(core: Arc<AppCore>, foreground: bool) -> Result<()> {
    cleanup_stale_pid()?;
    if let Some(pid) = read_pid() {
        if is_process_running(pid) {
            println!("Daemon already running (PID: {})", pid);
            return Ok(());
        }
    }

    if foreground {
        run_daemon(core).await
    } else {
        println!("Starting daemon in background...");

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;

            let exe = std::env::current_exe()?;
            let mut command = Command::new(exe);
            // SAFETY: setsid() creates a new session and is safe to call from pre_exec
            unsafe {
                command
                    .arg("daemon")
                    .arg("start")
                    .arg("--foreground")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .pre_exec(|| {
                        nix::unistd::setsid().map(|_| ()).map_err(|err| {
                            std::io::Error::new(std::io::ErrorKind::Other, err)
                        })
                    });
            }

            let child = command.spawn()?;
            println!("Daemon started (PID: {})", child.id());
            Ok(())
        }

        #[cfg(not(unix))]
        {
            let _ = core;
            println!("Background mode not supported on this platform");
            println!("Use --foreground instead");
            Ok(())
        }
    }
}

async fn run_daemon(core: Arc<AppCore>) -> Result<()> {
    let pid_path = pid_file();
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    #[cfg(unix)]
    {
        tokio::spawn(async move {
            let mut sigterm = tokio::signal::unix::signal(
                tokio::signal::unix::SignalKind::terminate(),
            )
            .unwrap();

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
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = shutdown_tx.send(());
        });
    }

    let mut runner = CliTaskRunner::new(core);
    runner.start().await?;

    println!("Daemon running. Press Ctrl+C to stop.");

    let _ = shutdown_rx.await;

    runner.stop().await?;
    let _ = std::fs::remove_file(&pid_path);

    println!("Daemon stopped");
    Ok(())
}

async fn stop() -> Result<()> {
    cleanup_stale_pid()?;
    if let Some(pid) = read_pid() {
        if is_process_running(pid) {
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;

                kill(Pid::from_raw(pid), Signal::SIGTERM)?;
                println!("Sent stop signal to daemon (PID: {})", pid);
            }

            #[cfg(not(unix))]
            {
                println!("Stop not supported on this platform");
            }
        } else {
            println!("Daemon not running");
        }
    } else {
        println!("Daemon not running");
    }

    Ok(())
}

async fn status() -> Result<()> {
    cleanup_stale_pid()?;
    if let Some(pid) = read_pid() {
        if is_process_running(pid) {
            println!("Daemon running (PID: {})", pid);
        } else {
            println!("Daemon not running (stale PID file)");
            let _ = std::fs::remove_file(pid_file());
        }
    } else {
        println!("Daemon not running");
    }
    Ok(())
}

fn is_process_running(pid: i32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;

        return kill(Pid::from_raw(pid), None).is_ok();
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
