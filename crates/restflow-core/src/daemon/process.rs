use crate::paths;
use anyhow::Result;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub http: bool,
    pub http_port: Option<u16>,
    pub mcp: bool,
    pub mcp_port: Option<u16>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            http: false,
            http_port: None,
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

        // Clean up stale PID file (process no longer alive) before attempting
        // exclusive creation. get_running_pid already removes stale files, but
        // a race between two callers could leave a leftover file.
        let _ = std::fs::remove_file(&self.pid_file);

        // Atomically create the PID file; if another process raced us, this
        // will fail with AlreadyExists.
        let mut pid_file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.pid_file)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Another process won the race — check if it's still alive.
                if let Some(pid) = self.get_running_pid()? {
                    return Ok(pid);
                }
                // Stale file from lost race; retry once.
                let _ = std::fs::remove_file(&self.pid_file);
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&self.pid_file)?
            }
            Err(e) => return Err(e.into()),
        };

        let exe = std::env::current_exe()?;
        let mut cmd = Command::new(exe);
        cmd.args(["daemon", "start", "--foreground"]);
        if config.http {
            cmd.arg("--http");
            if let Some(port) = config.http_port {
                cmd.args(["--port", &port.to_string()]);
            }
        }
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

        let child = cmd.spawn()?;
        let pid = child.id();
        write!(pid_file, "{}", pid)?;
        Ok(pid)
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
        let pid: u32 = pid_str.trim().parse()?;
        if self.is_process_alive(pid) {
            Ok(Some(pid))
        } else {
            let _ = std::fs::remove_file(&self.pid_file);
            Ok(None)
        }
    }

    fn is_process_alive(&self, pid: u32) -> bool {
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
}
