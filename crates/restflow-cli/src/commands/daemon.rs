use crate::cli::DaemonCommands;
use crate::commands::claude_mcp::try_sync_claude_http_mcp;
use crate::commands::codex_mcp::try_sync_codex_http_mcp;
use crate::commands::daemon_state::{self, EffectiveDaemonStatus, RunningSource};
use crate::daemon::CliBackgroundAgentRunner;
use anyhow::{Context, Result};
use restflow_core::AppCore;
use restflow_core::daemon::{DaemonConfig, IpcServer, start_daemon_with_config, stop_daemon};
use restflow_core::paths;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
#[cfg(not(unix))]
use std::process::Command;
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};

#[cfg(unix)]
use nix::libc;

const MCP_BIND_ADDR_ENV: &str = "RESTFLOW_MCP_BIND_ADDR";
const CLEANUP_INTERVAL_HOURS: u64 = 24;
const DAEMON_STOP_TIMEOUT: Duration = Duration::from_secs(30);
const DAEMON_STOP_POLL_INTERVAL: Duration = Duration::from_millis(200);

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

    let was_running = stop_daemon_effective().await?;
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

    let pid = tokio::task::spawn_blocking(move || start_daemon_with_config(config)).await??;
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

/// Run daemon commands that do not require opening AppCore.
/// Returns true when the command is handled and the caller should return.
pub async fn run_without_core(command: &DaemonCommands) -> Result<bool> {
    match command {
        DaemonCommands::Start {
            foreground: false,
            mcp_port,
        } => {
            start_background(*mcp_port).await?;
            Ok(true)
        }
        DaemonCommands::Restart {
            foreground: false,
            mcp_port,
        } => {
            restart_background(*mcp_port).await?;
            Ok(true)
        }
        DaemonCommands::Stop => {
            stop().await?;
            Ok(true)
        }
        DaemonCommands::Status => {
            status().await?;
            Ok(true)
        }
        DaemonCommands::Start {
            foreground: true, ..
        }
        | DaemonCommands::Restart {
            foreground: true, ..
        } => Ok(false),
    }
}

async fn start_background(mcp_port: Option<u16>) -> Result<()> {
    let config = DaemonConfig {
        mcp: true,
        mcp_port,
    };

    let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
    if let EffectiveDaemonStatus::Running { pid, .. } = snapshot.daemon_status {
        print_already_running(pid);
        return Ok(());
    }

    let report = restflow_core::daemon::recovery::recover().await?;
    if !report.is_clean() {
        println!("{}", report);
    }
    let pid = tokio::task::spawn_blocking(move || start_daemon_with_config(config)).await??;
    println!("Daemon started (PID: {})", pid);
    sync_mcp_configs(mcp_port).await;
    Ok(())
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
        let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
        if let EffectiveDaemonStatus::Running { pid, .. } = snapshot.daemon_status {
            print_already_running(pid);
            Ok(())
        } else {
            // Clean stale artifacts (e.g. leftover socket) before spawning.
            let report = restflow_core::daemon::recovery::recover().await?;
            if !report.is_clean() {
                println!("{}", report);
            }
            let pid = tokio::task::spawn_blocking(move || start_daemon_with_config(config)).await??;
            println!("Daemon started (PID: {})", pid);
            Ok(())
        }
    }
}

fn print_already_running(pid: Option<u32>) {
    if let Some(pid) = pid {
        println!("Daemon already running (PID: {})", pid);
    } else {
        println!("Daemon already running (PID: unknown)");
    }
}

async fn restart(core: Arc<AppCore>, foreground: bool, mcp_port: Option<u16>) -> Result<()> {
    if foreground {
        let config = DaemonConfig {
            mcp: true,
            mcp_port,
        };
        let was_running = stop_daemon_effective().await?;
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

    let lock_path = paths::daemon_lock_path()?;
    let _lock_guard = DaemonLockGuard::acquire(lock_path)?;

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

    let mut runner = CliBackgroundAgentRunner::new(core.clone());
    if let Err(err) = runner.start().await {
        error!(error = %err, "Task runner failed to start; continuing without runner");
    }

    if let Err(err) = run_and_log_cleanup(core.clone()).await {
        warn!(error = %err, "Startup cleanup failed");
    }

    let cleanup_shutdown = shutdown_tx.subscribe();
    let cleanup_core = core.clone();
    let cleanup_handle = tokio::spawn(async move {
        run_cleanup_loop(cleanup_core, cleanup_shutdown).await;
    });

    // Ensure core services did not fail immediately before declaring daemon as running.
    sleep(Duration::from_millis(120)).await;
    if ipc_handle.is_finished() {
        anyhow::bail!("IPC server exited during startup");
    }
    if mcp_handle.is_finished() {
        anyhow::bail!("MCP server exited during startup");
    }

    let pid_path = paths::daemon_pid_path()?;
    std::fs::write(&pid_path, std::process::id().to_string())?;
    let _pid_guard = PidFileGuard::new(pid_path.clone());

    println!("Daemon running. Press Ctrl+C to stop.");

    let mut shutdown_rx = shutdown_tx.subscribe();
    let _ = shutdown_rx.recv().await;

    runner.stop().await?;
    let _ = ipc_handle.await;
    let _ = mcp_handle.await;
    let _ = cleanup_handle.await;

    println!("Daemon stopped");
    Ok(())
}

async fn run_cleanup_loop(core: Arc<AppCore>, mut shutdown: tokio::sync::broadcast::Receiver<()>) {
    let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_HOURS * 60 * 60));
    interval.tick().await;
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            _ = interval.tick() => {
                if let Err(err) = run_and_log_cleanup(core.clone()).await {
                    warn!(error = %err, "Scheduled cleanup failed");
                }
            }
        }
    }
}

async fn run_and_log_cleanup(core: Arc<AppCore>) -> Result<()> {
    let report = restflow_core::services::cleanup::run_cleanup(&core).await?;
    info!(
        chat_sessions = report.chat_sessions,
        background_tasks = report.background_tasks,
        checkpoints = report.checkpoints,
        memory_chunks = report.memory_chunks,
        memory_sessions = report.memory_sessions,
        vector_orphans = report.vector_orphans,
        daemon_logs = report.daemon_log_files,
        event_logs = report.event_log_files,
        "Storage cleanup completed"
    );
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
    if stop_daemon_effective().await? {
        println!("Sent stop signal to daemon");
        wait_for_daemon_exit_or_kill().await?;
        println!("Daemon stopped");
    } else {
        println!("Daemon not running");
    }
    Ok(())
}

async fn stop_daemon_effective() -> Result<bool> {
    if stop_daemon()? {
        return Ok(true);
    }

    let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
    if let EffectiveDaemonStatus::Running { pid: Some(pid), .. } = snapshot.daemon_status {
        send_terminate_signal(pid)?;
        return Ok(true);
    }

    Ok(false)
}

fn send_terminate_signal(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let pid_i32 =
            i32::try_from(pid).with_context(|| format!("Daemon PID {} exceeds i32 range", pid))?;
        kill(Pid::from_raw(pid_i32), Signal::SIGTERM)?;
    }

    #[cfg(not(unix))]
    {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()?;
    }

    Ok(())
}

async fn status() -> Result<()> {
    let snapshot = daemon_state::collect_daemon_status_snapshot(true).await?;

    match snapshot.daemon_status {
        EffectiveDaemonStatus::Running { pid, source } => {
            match (pid, source) {
                (Some(pid), RunningSource::PidFile) => {
                    println!("Daemon running (PID: {})", pid);
                }
                (Some(pid), RunningSource::LockFile) => {
                    println!("Daemon running (PID: {}, detected via lock file)", pid);
                }
                (Some(pid), RunningSource::SocketProbe) => {
                    println!("Daemon running (PID: {}, detected via socket)", pid);
                }
                (None, RunningSource::SocketProbe) => {
                    println!("Daemon running (PID: unknown, detected via socket)");
                }
                (None, RunningSource::PidFile | RunningSource::LockFile) => {
                    println!("Daemon running (PID: unknown)");
                }
            };
        }
        EffectiveDaemonStatus::NotRunning => {
            println!("Daemon not running");
            if let Some(report) = snapshot.auto_recovery {
                println!("  {}", report);
            }
            if snapshot.stale_state == restflow_core::daemon::recovery::StaleState::StaleSocket {
                println!("  Note: stale socket detected (run `daemon start` to auto-clean)");
            }
        }
        EffectiveDaemonStatus::Stale { pid } => {
            println!("Daemon not running (stale PID: {})", pid);
            if matches!(
                snapshot.stale_state,
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
    let deadline = tokio::time::Instant::now() + DAEMON_STOP_TIMEOUT;
    loop {
        let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
        if !snapshot.is_running() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            let detail = match snapshot.daemon_status {
                EffectiveDaemonStatus::Running {
                    pid: Some(pid),
                    source,
                } => format!("still running (pid={pid}, source={})", source.as_str()),
                EffectiveDaemonStatus::Running { pid: None, source } => {
                    format!("still running (pid=unknown, source={})", source.as_str())
                }
                EffectiveDaemonStatus::NotRunning => "status switched to not_running".to_string(),
                EffectiveDaemonStatus::Stale { pid } => format!("stale pid={pid}"),
            };
            anyhow::bail!(
                "Daemon did not stop within {}s: {}",
                DAEMON_STOP_TIMEOUT.as_secs(),
                detail
            );
        }
        sleep(DAEMON_STOP_POLL_INTERVAL).await;
    }
}

/// Wait for daemon to exit gracefully, then SIGKILL if it doesn't stop in time.
///
/// Phase 1: Poll for graceful exit up to `DAEMON_STOP_TIMEOUT` (30s).
/// Phase 2: If still alive, send SIGKILL and wait up to 5s more.
async fn wait_for_daemon_exit_or_kill() -> Result<()> {
    const KILL_GRACE_PERIOD: Duration = Duration::from_secs(5);

    let deadline = tokio::time::Instant::now() + DAEMON_STOP_TIMEOUT;
    loop {
        let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
        if !snapshot.is_running() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            // Extract PID for SIGKILL
            let pid = match snapshot.daemon_status {
                EffectiveDaemonStatus::Running { pid: Some(pid), .. } => pid,
                _ => {
                    anyhow::bail!(
                        "Daemon did not stop within {}s and PID is unknown; cannot force-kill",
                        DAEMON_STOP_TIMEOUT.as_secs()
                    );
                }
            };

            warn!(
                pid,
                timeout_secs = DAEMON_STOP_TIMEOUT.as_secs(),
                "Daemon did not stop gracefully, sending SIGKILL"
            );
            send_kill_signal(pid)?;

            // Wait briefly for the kill to take effect
            let kill_deadline = tokio::time::Instant::now() + KILL_GRACE_PERIOD;
            loop {
                let snap = daemon_state::collect_daemon_status_snapshot(false).await?;
                if !snap.is_running() {
                    return Ok(());
                }
                if tokio::time::Instant::now() >= kill_deadline {
                    anyhow::bail!(
                        "Daemon (PID {}) still alive after SIGKILL; manual intervention required",
                        pid
                    );
                }
                sleep(DAEMON_STOP_POLL_INTERVAL).await;
            }
        }
        sleep(DAEMON_STOP_POLL_INTERVAL).await;
    }
}

fn send_kill_signal(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let pid_i32 =
            i32::try_from(pid).with_context(|| format!("Daemon PID {} exceeds i32 range", pid))?;
        kill(Pid::from_raw(pid_i32), Signal::SIGKILL)?;
    }

    #[cfg(not(unix))]
    {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()?;
    }

    Ok(())
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

struct PidFileGuard {
    path: PathBuf,
}

impl PidFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

struct DaemonLockGuard {
    path: PathBuf,
    #[cfg(unix)]
    _file: std::fs::File, // Keep file handle open for flock
}

impl DaemonLockGuard {
    fn acquire(path: PathBuf) -> Result<Self> {
        let current_pid = std::process::id();

        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            use std::os::unix::io::AsRawFd;

            // Create or open the lock file
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .context("Failed to create daemon lock file")?;

            // Try to acquire exclusive lock (non-blocking)
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };

            if rc != 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EWOULDBLOCK)
                    || err.raw_os_error() == Some(libc::EAGAIN)
                {
                    anyhow::bail!("Daemon already running (lock file held by another process)");
                }
                anyhow::bail!("Failed to acquire daemon lock: {}", err);
            }

            // Write PID to lock file
            write!(&file, "{}", current_pid)?;

            Ok(Self { path, _file: file })
        }

        #[cfg(not(unix))]
        {
            // Fallback for non-Unix platforms (still has TOCTOU but with reduced window)
            let mut attempts = 0;
            loop {
                attempts += 1;

                match std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                {
                    Ok(mut lock_file) => {
                        use std::io::Write;
                        write!(lock_file, "{}", current_pid)?;
                        return Ok(Self { path });
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                        if let Some(lock_pid) = read_lock_pid(&path)
                            && is_process_alive(lock_pid)
                        {
                            anyhow::bail!("Daemon already running (lock held by PID {})", lock_pid);
                        }
                        let _ = std::fs::remove_file(&path);
                        if attempts >= 2 {
                            anyhow::bail!(
                                "Failed to acquire daemon lock after removing stale lock file"
                            );
                        }
                    }
                    Err(err) => return Err(err.into()),
                }
            }
        }
    }
}

impl Drop for DaemonLockGuard {
    fn drop(&mut self) {
        // Note: On Unix, the lock is automatically released when the file handle is dropped.
        // We still remove the file for cleanup.
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(not(unix))]
fn read_lock_pid(path: &std::path::Path) -> Option<u32> {
    let content = std::fs::read_to_string(path).ok()?;
    content.trim().parse::<u32>().ok()
}

#[cfg(not(unix))]
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        let Ok(pid_i32) = i32::try_from(pid) else {
            return false;
        };
        kill(Pid::from_raw(pid_i32), None).is_ok()
    }

    #[cfg(not(unix))]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
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
