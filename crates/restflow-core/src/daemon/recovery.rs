use crate::paths;
use anyhow::Result;
use std::fmt;
use std::path::Path;
use tracing::{debug, info, warn};

/// Describes the state of daemon artifacts (PID file and socket).
#[derive(Debug, Clone, PartialEq)]
pub enum StaleState {
    /// Everything is healthy — an active daemon owns the artifacts.
    Healthy,
    /// PID file references a dead process.
    StalePid,
    /// Socket file exists but no daemon is listening.
    StaleSocket,
    /// Both PID file and socket are stale.
    Both,
    /// No artifacts present at all.
    Clean,
}

/// Evidence of what the recovery routine cleaned up.
#[derive(Debug, Clone, Default)]
pub struct RecoveryReport {
    pub removed_pid_file: bool,
    pub removed_socket: bool,
    pub stale_pid: Option<u32>,
}

impl RecoveryReport {
    /// Returns `true` when no cleanup was necessary.
    pub fn is_clean(&self) -> bool {
        !self.removed_pid_file && !self.removed_socket
    }
}

impl fmt::Display for RecoveryReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_clean() {
            return write!(f, "No stale artifacts found");
        }
        let mut parts = Vec::new();
        if self.removed_pid_file {
            if let Some(pid) = self.stale_pid {
                parts.push(format!("removed stale PID file (was PID {})", pid));
            } else {
                parts.push("removed stale PID file".to_string());
            }
        }
        if self.removed_socket {
            parts.push("removed stale socket".to_string());
        }
        write!(f, "Auto-cleaned: {}", parts.join(", "))
    }
}

/// Inspect daemon artifacts and determine their staleness.
pub async fn inspect(pid_path: &Path, socket_path: &Path) -> Result<StaleState> {
    let pid_exists = pid_path.exists();
    let socket_exists = socket_path.exists();

    if !pid_exists && !socket_exists {
        return Ok(StaleState::Clean);
    }

    // Check if the PID file references a live process.
    let pid_alive = if pid_exists {
        match read_pid(pid_path) {
            Some(pid) => is_process_alive(pid),
            None => false, // Unparseable PID file is effectively stale.
        }
    } else {
        false
    };

    // Check if the socket is alive by attempting a connection.
    let socket_alive = if socket_exists {
        crate::daemon::ipc_client::is_daemon_available(socket_path).await
    } else {
        false
    };

    match (pid_alive, socket_alive) {
        (true, _) => Ok(StaleState::Healthy),
        (false, true) => {
            // Socket responds but PID file is stale/missing — unusual but
            // treat socket as authoritative; only the PID file is stale.
            if pid_exists {
                Ok(StaleState::StalePid)
            } else {
                Ok(StaleState::Healthy)
            }
        }
        (false, false) => {
            if pid_exists && socket_exists {
                Ok(StaleState::Both)
            } else if pid_exists {
                Ok(StaleState::StalePid)
            } else {
                Ok(StaleState::StaleSocket)
            }
        }
    }
}

/// Remove stale artifacts. Only deletes files that are verified stale —
/// never touches files belonging to a healthy daemon.
pub async fn recover() -> Result<RecoveryReport> {
    let pid_path = paths::daemon_pid_path()?;
    let socket_path = paths::socket_path()?;

    let state = inspect(&pid_path, &socket_path).await?;
    let mut report = RecoveryReport::default();

    match state {
        StaleState::Healthy | StaleState::Clean => {
            debug!("No stale daemon artifacts to clean up");
        }
        StaleState::StalePid => {
            report.stale_pid = read_pid(&pid_path);
            remove_file_logged(&pid_path, "PID file");
            report.removed_pid_file = true;
        }
        StaleState::StaleSocket => {
            remove_file_logged(&socket_path, "socket");
            report.removed_socket = true;
        }
        StaleState::Both => {
            report.stale_pid = read_pid(&pid_path);
            remove_file_logged(&pid_path, "PID file");
            report.removed_pid_file = true;
            remove_file_logged(&socket_path, "socket");
            report.removed_socket = true;
        }
    }

    if !report.is_clean() {
        info!("{}", report);
    }

    Ok(report)
}

fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

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
        use std::process::Command;
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

fn remove_file_logged(path: &Path, label: &str) {
    match std::fs::remove_file(path) {
        Ok(()) => info!("Removed stale {}: {}", label, path.display()),
        Err(e) => warn!(
            "Failed to remove stale {}: {} ({})",
            label,
            path.display(),
            e
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_paths(dir: &TempDir) -> (PathBuf, PathBuf) {
        let pid = dir.path().join("daemon.pid");
        let sock = dir.path().join("restflow.sock");
        (pid, sock)
    }

    #[tokio::test]
    async fn inspect_clean_returns_clean() {
        let dir = TempDir::new().unwrap();
        let (pid, sock) = make_paths(&dir);
        let state = inspect(&pid, &sock).await.unwrap();
        assert_eq!(state, StaleState::Clean);
    }

    #[tokio::test]
    async fn inspect_stale_pid_detected() {
        let dir = TempDir::new().unwrap();
        let (pid_path, sock_path) = make_paths(&dir);
        let mut f = std::fs::File::create(&pid_path).unwrap();
        write!(f, "999999999").unwrap();

        let state = inspect(&pid_path, &sock_path).await.unwrap();
        assert_eq!(state, StaleState::StalePid);
    }

    #[tokio::test]
    async fn inspect_stale_socket_detected() {
        let dir = TempDir::new().unwrap();
        let (pid_path, sock_path) = make_paths(&dir);
        std::fs::File::create(&sock_path).unwrap();

        let state = inspect(&pid_path, &sock_path).await.unwrap();
        assert_eq!(state, StaleState::StaleSocket);
    }

    #[tokio::test]
    async fn inspect_both_stale_detected() {
        let dir = TempDir::new().unwrap();
        let (pid_path, sock_path) = make_paths(&dir);
        let mut f = std::fs::File::create(&pid_path).unwrap();
        write!(f, "999999999").unwrap();
        std::fs::File::create(&sock_path).unwrap();

        let state = inspect(&pid_path, &sock_path).await.unwrap();
        assert_eq!(state, StaleState::Both);
    }

    #[test]
    fn recovery_report_display_clean() {
        let report = RecoveryReport::default();
        assert!(report.is_clean());
        assert_eq!(format!("{}", report), "No stale artifacts found");
    }

    #[test]
    fn recovery_report_display_removed() {
        let report = RecoveryReport {
            removed_pid_file: true,
            removed_socket: true,
            stale_pid: Some(12345),
        };
        let s = format!("{}", report);
        assert!(s.contains("stale PID file"));
        assert!(s.contains("12345"));
        assert!(s.contains("stale socket"));
    }

    #[test]
    fn read_pid_valid() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.pid");
        std::fs::write(&path, "42").unwrap();
        assert_eq!(read_pid(&path), Some(42));
    }

    #[test]
    fn read_pid_invalid() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.pid");
        std::fs::write(&path, "not-a-number").unwrap();
        assert_eq!(read_pid(&path), None);
    }

    #[test]
    fn read_pid_missing() {
        let path = PathBuf::from("/tmp/nonexistent-pid-file-restflow-test");
        assert_eq!(read_pid(&path), None);
    }
}
