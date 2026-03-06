use anyhow::Result;
use chrono::Utc;
use restflow_core::{
    daemon::IpcClient,
    models::{
        BackgroundAgent, BackgroundAgentStatus, ExecutionTraceCategory, ExecutionTraceEvent,
        ExecutionTraceQuery,
    },
};
use serde::Serialize;

use crate::commands::daemon_state::{self, EffectiveDaemonStatus};
use crate::output::{json::print_json, OutputFormat};

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
    let summary = load_summary(&snapshot).await.unwrap_or_else(|err| {
        eprintln!("Failed to load UI summary via daemon IPC: {err}");
        empty_summary()
    });
    let output = build_ui_snapshot_output(&snapshot, summary);

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
    println!(
        "Tokens: in={} out={} total={}",
        output.summary.tokens.input, output.summary.tokens.output, output.summary.tokens.total
    );
    println!("Cost: usd={}", output.summary.cost.usd);
    println!(
        "Tasks: active={} queued={} completed_today={}",
        output.summary.tasks.active,
        output.summary.tasks.queued,
        output.summary.tasks.completed_today
    );

    Ok(())
}

fn build_ui_snapshot_output(
    snapshot: &daemon_state::DaemonStatusSnapshot,
    summary: SummarySection,
) -> UiSnapshotOutput {
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
        summary,
    }
}

async fn load_summary(snapshot: &daemon_state::DaemonStatusSnapshot) -> Result<SummarySection> {
    if !snapshot.is_running() {
        return Ok(empty_summary());
    }

    let mut client = IpcClient::connect(&snapshot.socket_path).await?;
    let tasks = client.list_background_agents(None).await?;
    let llm_events = client
        .query_execution_traces(ExecutionTraceQuery {
            category: Some(ExecutionTraceCategory::LlmCall),
            limit: None,
            ..Default::default()
        })
        .await?;

    Ok(summarize_data(&tasks, &llm_events))
}

fn summarize_data(tasks: &[BackgroundAgent], llm_events: &[ExecutionTraceEvent]) -> SummarySection {
    let today_start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| dt.and_utc().timestamp_millis())
        .unwrap_or(0);

    let active = tasks
        .iter()
        .filter(|task| task.status == BackgroundAgentStatus::Running)
        .count() as u32;
    let queued = tasks
        .iter()
        .filter(|task| task.status == BackgroundAgentStatus::Active)
        .count() as u32;
    let completed_today = tasks
        .iter()
        .filter(|task| {
            task.status == BackgroundAgentStatus::Completed && task.updated_at >= today_start
        })
        .count() as u32;

    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut total_tokens = 0u64;
    let mut total_cost_usd = 0.0f64;

    for event in llm_events {
        if let Some(llm) = event.llm_call.as_ref() {
            let input = llm.input_tokens.unwrap_or(0) as u64;
            let output = llm.output_tokens.unwrap_or(0) as u64;
            let total = llm
                .total_tokens
                .map(|value| value as u64)
                .unwrap_or(input + output);

            input_tokens += input;
            output_tokens += output;
            total_tokens += total;
            total_cost_usd += llm.cost_usd.unwrap_or(0.0);
        }
    }

    if input_tokens == 0 && output_tokens == 0 && total_tokens == 0 {
        total_tokens = tasks
            .iter()
            .map(|task| task.total_tokens_used as u64)
            .sum::<u64>();
    }

    if total_cost_usd == 0.0 {
        total_cost_usd = tasks.iter().map(|task| task.total_cost_usd).sum::<f64>();
    }

    SummarySection {
        tokens: TokenSummary {
            input: input_tokens,
            output: output_tokens,
            total: total_tokens,
        },
        cost: CostSummary {
            usd: total_cost_usd,
        },
        tasks: TaskSummary {
            active,
            queued,
            completed_today,
        },
    }
}

fn empty_summary() -> SummarySection {
    SummarySection {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::daemon_state::{DaemonStatusSnapshot, RunningSource};
    use restflow_core::{
        daemon::recovery::StaleState,
        models::{BackgroundAgent, ExecutionTraceEvent, LlmCallTrace, TaskSchedule},
    };
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
        let output = build_ui_snapshot_output(&snapshot, empty_summary());

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
        let output = build_ui_snapshot_output(&snapshot, empty_summary());

        let value = serde_json::to_value(&output).expect("serialize stale snapshot");

        assert_eq!(value["daemon"]["status"], "stale");
        assert_eq!(value["daemon"]["pid"], serde_json::Value::Null);
        assert_eq!(value["daemon"]["source"], serde_json::Value::Null);
        assert_eq!(value["daemon"]["stale_pid"], 99);
    }

    #[test]
    fn summarizes_background_tasks_and_execution_traces() {
        let running = make_task("task-running", BackgroundAgentStatus::Running, 0, 0.0);
        let queued = make_task("task-queued", BackgroundAgentStatus::Active, 0, 0.0);
        let completed = make_task(
            "task-completed",
            BackgroundAgentStatus::Completed,
            300,
            1.25,
        );
        let llm_events = vec![ExecutionTraceEvent::llm_call(
            "task-completed",
            "agent-1",
            LlmCallTrace {
                model: "gpt-5".to_string(),
                input_tokens: Some(200),
                output_tokens: Some(100),
                total_tokens: Some(300),
                cost_usd: Some(1.25),
                duration_ms: Some(1200),
                is_reasoning: Some(false),
                message_count: Some(4),
            },
        )];

        let summary = summarize_data(&[running, queued, completed], &llm_events);

        assert_eq!(summary.tasks.active, 1);
        assert_eq!(summary.tasks.queued, 1);
        assert_eq!(summary.tasks.completed_today, 1);
        assert_eq!(summary.tokens.input, 200);
        assert_eq!(summary.tokens.output, 100);
        assert_eq!(summary.tokens.total, 300);
        assert_eq!(summary.cost.usd, 1.25);
    }

    #[test]
    fn falls_back_to_task_totals_when_llm_traces_are_missing() {
        let completed = make_task("task-completed", BackgroundAgentStatus::Completed, 450, 2.5);

        let summary = summarize_data(&[completed], &[]);

        assert_eq!(summary.tokens.input, 0);
        assert_eq!(summary.tokens.output, 0);
        assert_eq!(summary.tokens.total, 450);
        assert_eq!(summary.cost.usd, 2.5);
    }

    fn make_task(
        id: &str,
        status: BackgroundAgentStatus,
        total_tokens_used: u32,
        total_cost_usd: f64,
    ) -> BackgroundAgent {
        let mut task = BackgroundAgent::new(
            id.to_string(),
            format!("Task {id}"),
            "agent-1".to_string(),
            TaskSchedule::default(),
        );
        task.status = status;
        task.updated_at = Utc::now().timestamp_millis();
        task.total_tokens_used = total_tokens_used;
        task.total_cost_usd = total_cost_usd;
        task
    }
}
