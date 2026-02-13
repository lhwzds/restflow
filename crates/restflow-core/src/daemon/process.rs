use crate::paths;
use anyhow::Result;
use std::ffi::OsString;
use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::{debug, warn};

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

    pub fn start(&self, config: DaemonConfig) -> Result<u32> {
        if let Some(pid) = self.get_running_pid()? {
            return Ok(pid);
        }

        let exe = std::env::current_exe()?;
        let mut cmd = Command::new(exe);
        cmd.args(["daemon", "start", "--foreground"]);
        // MCP server is enabled by default; only pass an explicit port override.
        if config.mcp
            && let Some(port) = config.mcp_port
        {
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
                return Ok(pid);
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
        Ok(bootstrap_pid)
    }

    pub fn stop(&self) -> Result<bool> {
        if let Some(pid) = self.get_running_pid()? {
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
}
