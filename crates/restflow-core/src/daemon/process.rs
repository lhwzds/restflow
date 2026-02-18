//! Unified process handling abstraction for daemon-managed child processes.
//! Provides a centralized way to spawn, track, and terminate processes with
//! proper cleanup on failure.

use crate::paths;
#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;
use std::ffi::OsString;
use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::{debug, warn};

#[cfg(unix)]
fn pid_to_unix_pid(pid: u32) -> Result<nix::unistd::Pid> {
    let pid_i32 = i32::try_from(pid).with_context(|| format!("PID {} exceeds i32 range", pid))?;
    Ok(nix::unistd::Pid::from_raw(pid_i32))
}

/// Represents a spawned child process with unified lifecycle management.
pub struct ProcessHandle {
    pid: u32,
    /// If set, this process was spawned as part of a transactional startup.
    /// On failure, we can kill this process to roll back.
    transactional: bool,
}

impl ProcessHandle {
    /// Create a new ProcessHandle for the given PID.
    pub fn new(pid: u32, transactional: bool) -> Self {
        Self { pid, transactional }
    }

    /// Get the PID of this process.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Check if this process is still running.
    pub fn is_alive(&self) -> bool {
        is_process_alive_internal(self.pid)
    }

    /// Check if this is a transactional process.
    pub fn is_transactional(&self) -> bool {
        self.transactional
    }

    /// Terminate this process gracefully (SIGTERM on Unix).
    pub fn terminate(&self) -> Result<()> {
        terminate_process(self.pid)
    }

    /// Forcefully kill this process (SIGKILL on Unix).
    pub fn kill(&self) -> Result<()> {
        kill_process(self.pid)
    }

    /// Kill the entire process tree (process + all children).
    #[cfg(unix)]
    pub fn kill_tree(&self) -> Result<()> {
        kill_process_tree(self.pid)
    }

    #[cfg(not(unix))]
    pub fn kill_tree(&self) -> Result<()> {
        // On non-Unix, just kill the main process
        self.kill()
    }
}

/// Registry for tracking all spawned daemon child processes.
/// Enables centralized cleanup on shutdown or partial startup failure.
pub struct ProcessRegistry {
    processes: tokio::sync::RwLock<Vec<ProcessHandle>>,
}

impl Default for ProcessRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self {
            processes: tokio::sync::RwLock::new(Vec::new()),
        }
    }

    /// Register a newly spawned process.
    pub async fn register(&self, handle: ProcessHandle) {
        let mut processes = self.processes.write().await;
        processes.push(handle);
    }

    /// Register a spawned process synchronously.
    #[allow(dead_code)]
    pub fn register_sync(&self, handle: ProcessHandle) {
        let mut processes = self.processes.blocking_write();
        processes.push(handle);
    }

    /// Unregister a process (e.g., after it exits normally).
    pub async fn unregister(&self, pid: u32) {
        let mut processes = self.processes.write().await;
        processes.retain(|p| p.pid != pid);
    }

    /// Get count of tracked processes.
    pub async fn len(&self) -> usize {
        let processes = self.processes.read().await;
        processes.len()
    }

    /// Check if registry is empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Terminate all tracked processes. Used for cleanup on shutdown.
    pub async fn terminate_all(&self) -> Result<()> {
        let processes = self.processes.read().await;
        for handle in processes.iter() {
            let _ = handle.terminate();
        }
        Ok(())
    }

    /// Kill all tracked processes forcefully. Used for emergency shutdown.
    pub async fn kill_all(&self) -> Result<()> {
        let processes = self.processes.read().await;
        for handle in processes.iter() {
            let _ = handle.kill();
        }
        Ok(())
    }

    /// Wait for all processes to exit within the given timeout.
    pub async fn wait_all_exit(&self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        loop {
            let alive = {
                let processes = self.processes.read().await;
                processes.iter().filter(|p| p.is_alive()).count()
            };

            if alive == 0 {
                return Ok(());
            }

            if start.elapsed() > timeout {
                // Force kill any remaining processes
                self.kill_all().await?;
                anyhow::bail!("Timeout waiting for processes to exit");
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Clear all tracked processes (e.g., after graceful shutdown).
    pub async fn clear(&self) {
        let mut processes = self.processes.write().await;
        processes.clear();
    }
}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub mcp: bool,
    pub mcp_port: Option<u16>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            mcp: true,
            mcp_port: Some(8787),
        }
    }
}

pub struct ProcessManager {
    pid_file: PathBuf,
    log_dir: PathBuf,
}

impl ProcessManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            pid_file: paths::daemon_pid_path()?,
            log_dir: paths::logs_dir()?,
        })
    }

    /// Start the daemon, returning a ProcessHandle for tracking.
    /// This enables transactional startup: if something fails after spawning,
    /// the caller can use the handle to kill the process.
    pub fn start(&self, config: DaemonConfig) -> Result<ProcessHandle> {
        if let Some(pid) = self.get_running_pid()? {
            return Ok(ProcessHandle::new(pid, false));
        }

        let exe = std::env::current_exe()?;
        let mut cmd = Command::new(exe);
        cmd.args(["daemon", "start", "--foreground"]);
        // MCP server is enabled by default; only pass an explicit port override.
        if config.mcp && let Some(port) = config.mcp_port {
            cmd.args(["--mcp-port", &port.to_string()]);
        }

        std::fs::create_dir_all(&self.log_dir)?;
        let log_file = self.log_dir.join("daemon.log");
        let log = File::create(&log_file)?;
        cmd.stdout(log.try_clone()?);
        cmd.stderr(log);
        cmd.stdin(Stdio::null());
        self.configure_child_path(&mut cmd);

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                cmd.pre_exec(|| {
                    nix::unistd::setsid()
                        .map(|_| ())
                        .map_err(std::io::Error::other)
                });
            }
        }

        let mut child = cmd.spawn()?;
        let bootstrap_pid = child.id();

        // Detect immediate spawn failures before waiting for daemon.pid.
        std::thread::sleep(Duration::from_millis(150));
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("Daemon process exited early with status {}", status);
        }

        // daemon.pid is written by the daemon process itself after startup
        // succeeds. Wait for it and return the authoritative PID.
        for _ in 0..60 {
            if let Some(pid) = self.get_running_pid()? {
                // Return transactional handle - if startup fails later, we can kill it
                return Ok(ProcessHandle::new(pid, true));
            }

            if let Some(status) = child.try_wait()? {
                anyhow::bail!(
                    "Daemon process exited during startup with status {}",
                    status
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        warn!(
            bootstrap_pid,
            "Daemon started but PID file not available yet; returning bootstrap pid"
        );
        // Return as transactional since we may need to clean it up
        Ok(ProcessHandle::new(bootstrap_pid, true))
    }

    /// Legacy method for backward compatibility - returns PID directly.
    pub fn start_legacy(&self, config: DaemonConfig) -> Result<u32> {
        let handle = self.start(config)?;
        Ok(handle.pid())
    }

    pub fn stop(&self) -> Result<bool> {
        if let Some(pid) = self.get_running_pid()? {
            #[cfg(unix)]
            {
                use nix::sys::signal::{Signal, kill};
                let signal_pid = pid_to_unix_pid(pid)?;
                kill(signal_pid, Signal::SIGTERM)?;
            }

            #[cfg(not(unix))]
            {
                Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output()?;
            }

            for _ in 0..50 {
                if !self.is_process_alive(pid) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            let _ = std::fs::remove_file(&self.pid_file);
            return Ok(true);
        }

        Ok(false)
    }

    pub fn get_running_pid(&self) -> Result<Option<u32>> {
        if !self.pid_file.exists() {
            return Ok(None);
        }

        let pid_str = std::fs::read_to_string(&self.pid_file)?;
        let pid: u32 = match pid_str.trim().parse() {
            Ok(pid) => pid,
            Err(err) => {
                warn!(
                    path = %self.pid_file.display(),
                    error = %err,
                    "Invalid daemon PID file contents; removing stale file"
                );
                let _ = std::fs::remove_file(&self.pid_file);
                return Ok(None);
            }
        };
        if self.is_process_alive(pid) {
            Ok(Some(pid))
        } else {
            let _ = std::fs::remove_file(&self.pid_file);
            Ok(None)
        }
    }

    fn configure_child_path(&self, cmd: &mut Command) {
        if let Some(path) = build_daemon_child_path() {
            cmd.env("PATH", &path);
            debug!(path = %path.to_string_lossy(), "Configured daemon child PATH");
        }
    }

    fn is_process_alive(&self, pid: u32) -> bool {
        is_process_alive_internal(pid)
    }
}

/// Internal function to check if a process is alive.
fn is_process_alive_internal(pid: u32) -> bool {
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
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

/// Terminate a process gracefully (SIGTERM).
fn terminate_process(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        let signal_pid = pid_to_unix_pid(pid)?;
        kill(signal_pid, Signal::SIGTERM)?;
    }

    #[cfg(not(unix))]
    {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T"])
            .output()?;
    }
    Ok(())
}

/// Forcefully kill a process (SIGKILL).
fn kill_process(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        let signal_pid = pid_to_unix_pid(pid)?;
        kill(signal_pid, Signal::SIGKILL)?;
    }

    #[cfg(not(unix))]
    {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()?;
    }
    Ok(())
}

/// Kill a process and all its children (process tree).
#[cfg(unix)]
fn kill_process_tree(pid: u32) -> Result<()> {
    use std::process::Command;
    // Use pkill to kill the entire process tree
    let _output = Command::new("pkill")
        .args(["-TERM", "-P", &pid.to_string()])
        .output()?;

    // Also kill the parent
    let _ = kill_process(pid);
    Ok(())
}

fn build_daemon_child_path() -> Option<OsString> {
    let mut entries: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();

    append_default_exec_dirs(&mut entries);

    let unique_entries = unique_paths(entries);
    if unique_entries.is_empty() {
        return None;
    }

    std::env::join_paths(unique_entries).ok()
}

fn append_default_exec_dirs(entries: &mut Vec<PathBuf>) {
    let defaults = [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ];
    entries.extend(defaults.into_iter().map(PathBuf::from));

    if let Some(home) = dirs::home_dir() {
        entries.push(home.join(".local").join("bin"));
        entries.push(home.join(".npm-global").join("bin"));
    }
}

fn unique_paths(entries: Vec<PathBuf>) -> Vec<PathBuf> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for path in entries {
        if path.as_os_str().is_empty() {
            continue;
        }
        if seen.insert(path.clone()) {
            unique.push(path);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_paths_removes_duplicates() {
        let input = vec![
            PathBuf::from("/usr/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/opt/homebrew/bin"),
        ];
        let unique = unique_paths(input);
        assert_eq!(unique.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn pid_to_unix_pid_rejects_out_of_range() {
        assert!(pid_to_unix_pid(i32::MAX as u32).is_ok());
        assert!(pid_to_unix_pid(i32::MAX as u32 + 1).is_err());
    }

    #[test]
    fn process_handle_new_creates_valid_handle() {
        let handle = ProcessHandle::new(1234, true);
        assert_eq!(handle.pid(), 1234);
        assert!(handle.is_transactional());
    }

    #[tokio::test]
    async fn process_registry_registers_and_unregisters() {
        let registry = ProcessRegistry::new();
        
        let handle = ProcessHandle::new(5678, false);
        registry.register(handle).await;
        
        assert_eq!(registry.len().await, 1);
        
        registry.unregister(5678).await;
        assert!(registry.is_empty().await);
    }

    #[tokio::test]
    async fn process_registry_clear() {
        let registry = ProcessRegistry::new();
        
        registry.register(ProcessHandle::new(1000, true)).await;
        registry.register(ProcessHandle::new(1001, true)).await;
        
        assert_eq!(registry.len().await, 2);
        
        registry.clear().await;
        assert!(registry.is_empty().await);
    }
}
