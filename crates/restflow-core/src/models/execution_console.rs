use serde::{Deserialize, Serialize};
use specta::Type;
use ts_rs::TS;

use crate::models::{ChatSessionSource, ExecutionTimeline};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[specta(skip_attr = "ts")]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionContainerKind {
    Workspace,
    BackgroundTask,
    ExternalChannel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[specta(skip_attr = "ts")]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    WorkspaceRun,
    BackgroundRun,
    ExternalRun,
    SubagentRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[specta(skip_attr = "ts")]
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
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct ExecutionContainerRef {
    pub kind: ExecutionContainerKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct RunSummary {
    pub id: String,
    pub kind: RunKind,
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
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct RunListQuery {
    pub container: ExecutionContainerRef,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct ChildRunListQuery {
    pub parent_run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct ExecutionThread {
    pub focus: RunSummary,
    pub timeline: ExecutionTimeline,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_types_expose_canonical_surface() {
        let summary = RunSummary {
            id: "run-1".to_string(),
            kind: RunKind::BackgroundRun,
            container_id: "task-1".to_string(),
            root_run_id: Some("run-1".to_string()),
            title: "Example Run".to_string(),
            subtitle: None,
            status: "completed".to_string(),
            updated_at: 1,
            started_at: Some(1),
            ended_at: Some(2),
            session_id: Some("session-1".to_string()),
            run_id: Some("run-1".to_string()),
            task_id: Some("task-1".to_string()),
            parent_run_id: None,
            agent_id: Some("agent-1".to_string()),
            source_channel: None,
            source_conversation_id: None,
            effective_model: Some("gpt-5.4".to_string()),
            provider: Some("openai".to_string()),
            event_count: 3,
        };
        let query = RunListQuery {
            container: ExecutionContainerRef {
                kind: ExecutionContainerKind::BackgroundTask,
                id: "task-1".to_string(),
            },
        };
        let child_query = ChildRunListQuery {
            parent_run_id: "run-1".to_string(),
        };

        assert_eq!(summary.run_id.as_deref(), Some("run-1"));
        assert_eq!(query.container.id, "task-1");
        assert_eq!(child_query.parent_run_id, "run-1");
    }
}
