use crate::StreamEnvelope;
use serde::{Deserialize, Serialize};
use specta::Type;
use ts_rs::TS;

pub const TASK_STREAM_EVENT: &str = "task:stream";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ChatSessionEvent {
    Created { session_id: String },
    Updated { session_id: String },
    MessageAdded { session_id: String, source: String },
    Deleted { session_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcStreamEvent {
    Task(TaskStreamEvent),
    Session(ChatSessionEvent),
}

pub type StreamFrame = StreamEnvelope<IpcStreamEvent>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionScope {
    Foreground {
        client_id: String,
        terminal_id: String,
    },
    DurableBackground {
        task_id: String,
    },
    Subagent {
        parent_run_id: String,
    },
}

impl ExecutionScope {
    pub fn foreground(client_id: impl Into<String>, terminal_id: impl Into<String>) -> Self {
        Self::Foreground {
            client_id: client_id.into(),
            terminal_id: terminal_id.into(),
        }
    }

    pub fn durable_background(task_id: impl Into<String>) -> Self {
        Self::DurableBackground {
            task_id: task_id.into(),
        }
    }

    pub fn subagent(parent_run_id: impl Into<String>) -> Self {
        Self::Subagent {
            parent_run_id: parent_run_id.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct TaskStreamEvent {
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ExecutionScope>,
    #[ts(type = "number")]
    pub timestamp: i64,
    pub kind: StreamEventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEventKind {
    Started {
        task_name: String,
        agent_id: String,
        execution_mode: String,
    },
    Output {
        text: String,
        is_stderr: bool,
        is_complete: bool,
    },
    Progress {
        phase: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        percent: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
    },
    Completed {
        result: String,
        #[ts(type = "number")]
        duration_ms: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        stats: Option<ExecutionStats>,
    },
    Failed {
        error: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_code: Option<String>,
        #[ts(type = "number")]
        duration_ms: i64,
        recoverable: bool,
    },
    Interrupted {
        reason: String,
        #[ts(type = "number")]
        duration_ms: i64,
    },
    Heartbeat {
        #[ts(type = "number")]
        elapsed_ms: i64,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct ExecutionStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_lines: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_calls: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

impl TaskStreamEvent {
    pub fn new(task_id: impl Into<String>, kind: StreamEventKind) -> Self {
        Self {
            task_id: task_id.into(),
            run_id: None,
            session_id: None,
            parent_run_id: None,
            scope: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind,
        }
    }

    pub fn with_run_context(
        mut self,
        run_id: Option<String>,
        session_id: Option<String>,
        parent_run_id: Option<String>,
        scope: Option<ExecutionScope>,
    ) -> Self {
        self.run_id = run_id;
        self.session_id = session_id;
        self.parent_run_id = parent_run_id;
        self.scope = scope;
        self
    }

    pub fn started(
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        agent_id: impl Into<String>,
        execution_mode: impl Into<String>,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Started {
                task_name: task_name.into(),
                agent_id: agent_id.into(),
                execution_mode: execution_mode.into(),
            },
        )
    }

    pub fn output(task_id: impl Into<String>, text: impl Into<String>, is_stderr: bool) -> Self {
        let text = text.into();
        let is_complete = text.ends_with('\n');
        Self::new(
            task_id,
            StreamEventKind::Output {
                text,
                is_stderr,
                is_complete,
            },
        )
    }

    pub fn output_partial(
        task_id: impl Into<String>,
        text: impl Into<String>,
        is_stderr: bool,
        is_complete: bool,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Output {
                text: text.into(),
                is_stderr,
                is_complete,
            },
        )
    }

    pub fn progress(
        task_id: impl Into<String>,
        phase: impl Into<String>,
        percent: Option<u8>,
        details: Option<String>,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Progress {
                phase: phase.into(),
                percent,
                details,
            },
        )
    }

    pub fn completed(
        task_id: impl Into<String>,
        result: impl Into<String>,
        duration_ms: i64,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Completed {
                result: result.into(),
                duration_ms,
                stats: None,
            },
        )
    }

    pub fn completed_with_stats(
        task_id: impl Into<String>,
        result: impl Into<String>,
        duration_ms: i64,
        stats: ExecutionStats,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Completed {
                result: result.into(),
                duration_ms,
                stats: Some(stats),
            },
        )
    }

    pub fn failed(
        task_id: impl Into<String>,
        error: impl Into<String>,
        duration_ms: i64,
        recoverable: bool,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Failed {
                error: error.into(),
                error_code: None,
                duration_ms,
                recoverable,
            },
        )
    }

    pub fn failed_with_code(
        task_id: impl Into<String>,
        error: impl Into<String>,
        error_code: impl Into<String>,
        duration_ms: i64,
        recoverable: bool,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Failed {
                error: error.into(),
                error_code: Some(error_code.into()),
                duration_ms,
                recoverable,
            },
        )
    }

    pub fn interrupted(
        task_id: impl Into<String>,
        reason: impl Into<String>,
        duration_ms: i64,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Interrupted {
                reason: reason.into(),
                duration_ms,
            },
        )
    }

    pub fn timeout(task_id: impl Into<String>, timeout_secs: u64, duration_ms: i64) -> Self {
        Self::interrupted(
            task_id,
            format!("Task timed out after {timeout_secs} seconds"),
            duration_ms,
        )
    }

    pub fn heartbeat(task_id: impl Into<String>, elapsed_ms: i64) -> Self {
        Self::new(task_id, StreamEventKind::Heartbeat { elapsed_ms })
    }
}
