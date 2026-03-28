use serde::{Deserialize, Serialize};
use specta::Type;
use ts_rs::TS;

use crate::models::{ChatSessionSource, ExecutionTimeline};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionContainerKind {
    Workspace,
    BackgroundTask,
    ExternalChannel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionSessionKind {
    WorkspaceRun,
    BackgroundRun,
    ExternalRun,
    SubagentRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ExecutionContainerSummary {
    pub id: String,
    pub kind: ExecutionContainerKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[ts(type = "number")]
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default)]
    pub session_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_channel: Option<ChatSessionSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ExecutionContainerRef {
    pub kind: ExecutionContainerKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ExecutionSessionSummary {
    pub id: String,
    pub kind: ExecutionSessionKind,
    pub container_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_run_id: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub status: String,
    #[ts(type = "number")]
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub ended_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_channel: Option<ChatSessionSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default)]
    #[ts(type = "bigint | number")]
    pub event_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ExecutionSessionListQuery {
    pub container: ExecutionContainerRef,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ChildExecutionSessionQuery {
    pub parent_run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct ExecutionThread {
    pub focus: ExecutionSessionSummary,
    pub timeline: ExecutionTimeline,
    #[serde(default)]
    pub child_sessions: Vec<ExecutionSessionSummary>,
}
