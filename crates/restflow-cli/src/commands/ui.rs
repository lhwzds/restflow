use anyhow::Result;
use serde::Serialize;

use crate::commands::daemon_state::{self, EffectiveDaemonStatus};
use crate::output::{OutputFormat, json::print_json};

#[derive(Debug, Serialize, PartialEq)]
pub struct UiSnapshotOutput {
    pub daemon: DaemonSection,
    pub paths: PathsSection,
    pub summary: SummarySection,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct DaemonSection {
    pub status: &'static str,
    pub pid: Option<u32>,
    pub source: Option<&'static str>,
    pub stale_pid: Option<u32>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PathsSection {
    pub socket: String,
    pub pid_file: String,
    pub db: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SummarySection {
    pub tokens: TokenSummary,
    pub cost: CostSummary,
    pub tasks: TaskSummary,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TokenSummary {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CostSummary {
    pub usd: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TaskSummary {
    pub active: u32,
    pub queued: u32,
    pub completed_today: u32,
}

pub async fn snapshot(format: OutputFormat) -> Result<()> {
    let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
    let output = build_ui_snapshot_output(&snapshot);

    if format.is_json() {
        return print_json(&output);
    }

    println!("UI Snapshot");
    println!("Daemon status: {}", output.daemon.status);
    if let Some(pid) = output.daemon.pid {
        println!("Daemon PID: {pid}");
    }
    if let Some(source) = output.daemon.source {
        println!("Daemon source: {source}");
    }
    if let Some(stale_pid) = output.daemon.stale_pid {
        println!("Stale PID: {stale_pid}");
    }
    println!("Socket: {}", output.paths.socket);
    println!("PID file: {}", output.paths.pid_file);
    println!("DB path: {}", output.paths.db);
    println!("Tokens: in=0 out=0 total=0");
    println!("Cost: usd=0.0");
    println!("Tasks: active=0 queued=0 completed_today=0");

    Ok(())
}

fn build_ui_snapshot_output(snapshot: &daemon_state::DaemonStatusSnapshot) -> UiSnapshotOutput {
    let (status, pid, source, stale_pid) = match snapshot.daemon_status {
        EffectiveDaemonStatus::Running { pid, source } => {
            ("running", pid, Some(source.as_str()), None)
        }
        EffectiveDaemonStatus::NotRunning => ("not_running", None, None, None),
        EffectiveDaemonStatus::Stale { pid } => ("stale", None, None, Some(pid)),
    };

    UiSnapshotOutput {
        daemon: DaemonSection {
            status,
            pid,
            source,
            stale_pid,
        },
        paths: PathsSection {
            socket: snapshot.socket_path.display().to_string(),
            pid_file: snapshot.pid_path.display().to_string(),
            db: snapshot.db_path.display().to_string(),
        },
        summary: SummarySection {
            tokens: TokenSummary {
                input: 0,
                output: 0,
                total: 0,
            },
            cost: CostSummary { usd: 0.0 },
            tasks: TaskSummary {
                active: 0,
                queued: 0,
                completed_today: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::daemon_state::{DaemonStatusSnapshot, RunningSource};
    use restflow_core::daemon::recovery::StaleState;
    use std::path::PathBuf;

    fn fake_snapshot(status: EffectiveDaemonStatus) -> DaemonStatusSnapshot {
        DaemonStatusSnapshot {
            daemon_status: status,
            auto_recovery: None,
            stale_state: StaleState::Clean,
            socket_path: PathBuf::from("/tmp/restflow.sock"),
            pid_path: PathBuf::from("/tmp/restflow.pid"),
            db_path: PathBuf::from("/tmp/restflow.db"),
        }
    }

    #[test]
    fn serializes_running_snapshot_with_stable_fields() {
        let snapshot = fake_snapshot(EffectiveDaemonStatus::Running {
            pid: Some(4242),
            source: RunningSource::PidFile,
        });
        let output = build_ui_snapshot_output(&snapshot);

        let value = serde_json::to_value(&output).expect("serialize ui snapshot");

        assert_eq!(value["daemon"]["status"], "running");
        assert_eq!(value["daemon"]["pid"], 4242);
        assert_eq!(value["daemon"]["source"], "pid_file");
        assert_eq!(value["paths"]["socket"], "/tmp/restflow.sock");
        assert_eq!(value["paths"]["pid_file"], "/tmp/restflow.pid");
        assert_eq!(value["paths"]["db"], "/tmp/restflow.db");
        assert_eq!(value["summary"]["tokens"]["input"], 0);
        assert_eq!(value["summary"]["tokens"]["output"], 0);
        assert_eq!(value["summary"]["tokens"]["total"], 0);
        assert_eq!(value["summary"]["cost"]["usd"], 0.0);
        assert_eq!(value["summary"]["tasks"]["active"], 0);
        assert_eq!(value["summary"]["tasks"]["queued"], 0);
        assert_eq!(value["summary"]["tasks"]["completed_today"], 0);
    }

    #[test]
    fn serializes_stale_snapshot_with_stale_pid() {
        let snapshot = fake_snapshot(EffectiveDaemonStatus::Stale { pid: 99 });
        let output = build_ui_snapshot_output(&snapshot);

        let value = serde_json::to_value(&output).expect("serialize stale snapshot");

        assert_eq!(value["daemon"]["status"], "stale");
        assert_eq!(value["daemon"]["pid"], serde_json::Value::Null);
        assert_eq!(value["daemon"]["source"], serde_json::Value::Null);
        assert_eq!(value["daemon"]["stale_pid"], 99);
    }
}
