use anyhow::Result;
use restflow_core::daemon::{self, DaemonStatus};
use restflow_core::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningSource {
    PidFile,
    LockFile,
    SocketProbe,
}

impl RunningSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PidFile => "pid_file",
            Self::LockFile => "lock_file",
            Self::SocketProbe => "socket_probe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveDaemonStatus {
    Running {
        pid: Option<u32>,
        source: RunningSource,
    },
    NotRunning,
    Stale {
        pid: u32,
    },
}

#[derive(Debug, Clone)]
pub struct DaemonStatusSnapshot {
    pub daemon_status: EffectiveDaemonStatus,
    pub auto_recovery: Option<String>,
    pub stale_state: restflow_core::daemon::recovery::StaleState,
    pub socket_path: std::path::PathBuf,
    pub pid_path: std::path::PathBuf,
    pub db_path: std::path::PathBuf,
}

impl DaemonStatusSnapshot {
    pub fn is_running(&self) -> bool {
        matches!(self.daemon_status, EffectiveDaemonStatus::Running { .. })
    }
}

pub async fn collect_daemon_status_snapshot(
    auto_recover_stale: bool,
) -> Result<DaemonStatusSnapshot> {
    let socket_path = paths::socket_path()?;
    let pid_path = paths::daemon_pid_path()?;
    let db_path = paths::database_path()?;
    let lock_path = paths::daemon_lock_path()?;

    let mut daemon_status = daemon::check_daemon_status()?;
    let mut auto_recovery = None;

    if auto_recover_stale && matches!(daemon_status, DaemonStatus::Stale { .. }) {
        let report = restflow_core::daemon::recovery::recover().await?;
        if !report.is_clean() {
            auto_recovery = Some(report.to_string());
        }
        daemon_status = daemon::check_daemon_status()?;
    }

    let stale_state = restflow_core::daemon::recovery::inspect(&pid_path, &socket_path).await?;
    let socket_alive = daemon::is_daemon_available(&socket_path).await;
    let lock_pid = read_lock_pid(&lock_path);
    let lock_alive_pid = lock_pid.filter(|pid| is_process_alive(*pid));

    let daemon_status = resolve_effective_status(daemon_status, socket_alive, lock_alive_pid);

    Ok(DaemonStatusSnapshot {
        daemon_status,
        auto_recovery,
        stale_state,
        socket_path,
        pid_path,
        db_path,
    })
}

fn read_lock_pid(path: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| content.trim().parse::<u32>().ok())
}

fn resolve_effective_status(
    raw_status: DaemonStatus,
    socket_alive: bool,
    lock_alive_pid: Option<u32>,
) -> EffectiveDaemonStatus {
    match raw_status {
        DaemonStatus::Running { pid } => EffectiveDaemonStatus::Running {
            pid: Some(pid),
            source: RunningSource::PidFile,
        },
        DaemonStatus::Stale { pid } => EffectiveDaemonStatus::Stale { pid },
        DaemonStatus::NotRunning => {
            if let Some(pid) = lock_alive_pid {
                EffectiveDaemonStatus::Running {
                    pid: Some(pid),
                    source: RunningSource::LockFile,
                }
            } else if socket_alive {
                EffectiveDaemonStatus::Running {
                    pid: None,
                    source: RunningSource::SocketProbe,
                }
            } else {
                EffectiveDaemonStatus::NotRunning
            }
        }
    }
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
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_running_prefers_pid_file() {
        let status = resolve_effective_status(DaemonStatus::Running { pid: 42 }, false, None);
        assert_eq!(
            status,
            EffectiveDaemonStatus::Running {
                pid: Some(42),
                source: RunningSource::PidFile
            }
        );
    }

    #[test]
    fn resolve_running_from_lock_file_when_pid_missing() {
        let status = resolve_effective_status(DaemonStatus::NotRunning, false, Some(1234));
        assert_eq!(
            status,
            EffectiveDaemonStatus::Running {
                pid: Some(1234),
                source: RunningSource::LockFile
            }
        );
    }

    #[test]
    fn resolve_running_from_socket_probe_when_no_pid_available() {
        let status = resolve_effective_status(DaemonStatus::NotRunning, true, None);
        assert_eq!(
            status,
            EffectiveDaemonStatus::Running {
                pid: None,
                source: RunningSource::SocketProbe
            }
        );
    }

    #[test]
    fn resolve_not_running_when_no_evidence() {
        let status = resolve_effective_status(DaemonStatus::NotRunning, false, None);
        assert_eq!(status, EffectiveDaemonStatus::NotRunning);
    }

    #[test]
    fn resolve_stale_passthrough() {
        let status = resolve_effective_status(DaemonStatus::Stale { pid: 9 }, false, None);
        assert_eq!(status, EffectiveDaemonStatus::Stale { pid: 9 });
    }
}
