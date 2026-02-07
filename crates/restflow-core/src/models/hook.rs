//! Hook model for task lifecycle automation.
//!
//! Hooks let users execute custom actions when task lifecycle events happen.

use super::AgentTask;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Hook trigger event.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    TaskStarted,
    TaskCompleted,
    TaskFailed,
    TaskCancelled,
    ToolExecuted,
    ApprovalRequired,
}

impl HookEvent {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::TaskStarted => "task_started",
            Self::TaskCompleted => "task_completed",
            Self::TaskFailed => "task_failed",
            Self::TaskCancelled => "task_cancelled",
            Self::ToolExecuted => "tool_executed",
            Self::ApprovalRequired => "approval_required",
        }
    }
}

/// Hook action definition.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookAction {
    /// Send an HTTP request with the hook context as JSON body.
    Webhook {
        url: String,
        #[serde(default)]
        method: Option<String>,
        #[serde(default)]
        headers: Option<BTreeMap<String, String>>,
    },
    /// Execute a local script and pass context as environment variables.
    Script {
        path: String,
        #[serde(default)]
        args: Option<Vec<String>>,
        #[serde(default)]
        timeout_secs: Option<u64>,
    },
    /// Send a templated message via channel router.
    SendMessage {
        channel_type: String,
        message_template: String,
    },
    /// Trigger a follow-up task.
    RunTask {
        agent_id: String,
        input_template: String,
    },
}

/// Optional filter to limit when a hook is executed.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct HookFilter {
    #[serde(default)]
    pub task_name_pattern: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub success_only: Option<bool>,
}

/// Persisted hook definition.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct Hook {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub event: HookEvent,
    pub action: HookAction,
    #[serde(default)]
    pub filter: Option<HookFilter>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

fn default_enabled() -> bool {
    true
}

impl Hook {
    /// Create a new hook with default metadata.
    pub fn new(name: String, event: HookEvent, action: HookAction) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description: None,
            event,
            action,
            filter: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update timestamp after changes.
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

/// Runtime context passed to hook actions.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct HookContext {
    pub event: HookEvent,
    pub task_id: String,
    pub task_name: String,
    pub agent_id: String,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub duration_ms: Option<i64>,
    #[ts(type = "number")]
    pub timestamp: i64,
}

impl HookContext {
    pub fn from_started(task: &AgentTask) -> Self {
        Self {
            event: HookEvent::TaskStarted,
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            agent_id: task.agent_id.clone(),
            success: None,
            output: None,
            error: None,
            duration_ms: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn from_completed(task: &AgentTask, output: &str, duration_ms: i64) -> Self {
        Self {
            event: HookEvent::TaskCompleted,
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            agent_id: task.agent_id.clone(),
            success: Some(true),
            output: Some(output.to_string()),
            error: None,
            duration_ms: Some(duration_ms),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn from_failed(task: &AgentTask, error: &str, duration_ms: i64) -> Self {
        Self {
            event: HookEvent::TaskFailed,
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            agent_id: task.agent_id.clone(),
            success: Some(false),
            output: None,
            error: Some(error.to_string()),
            duration_ms: Some(duration_ms),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn from_cancelled(task: &AgentTask, error: &str, duration_ms: i64) -> Self {
        Self {
            event: HookEvent::TaskCancelled,
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            agent_id: task.agent_id.clone(),
            success: Some(false),
            output: None,
            error: Some(error.to_string()),
            duration_ms: Some(duration_ms),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TaskSchedule;

    #[test]
    fn test_hook_new_defaults() {
        let hook = Hook::new(
            "Notify".to_string(),
            HookEvent::TaskCompleted,
            HookAction::SendMessage {
                channel_type: "telegram".to_string(),
                message_template: "Done {{task_name}}".to_string(),
            },
        );

        assert!(hook.enabled);
        assert!(!hook.id.is_empty());
        assert_eq!(hook.event, HookEvent::TaskCompleted);
    }

    #[test]
    fn test_context_from_completed() {
        let task = AgentTask::new(
            "task-1".to_string(),
            "Daily Summary".to_string(),
            "agent-1".to_string(),
            TaskSchedule::Once { run_at: 0 },
        );

        let context = HookContext::from_completed(&task, "ok", 123);
        assert_eq!(context.event, HookEvent::TaskCompleted);
        assert_eq!(context.output.as_deref(), Some("ok"));
        assert_eq!(context.success, Some(true));
        assert_eq!(context.duration_ms, Some(123));
    }

    #[test]
    fn export_bindings_hook() {
        Hook::export().expect("export Hook");
    }

    #[test]
    fn export_bindings_hook_event() {
        HookEvent::export().expect("export HookEvent");
    }

    #[test]
    fn export_bindings_hook_action() {
        HookAction::export().expect("export HookAction");
    }

    #[test]
    fn export_bindings_hook_filter() {
        HookFilter::export().expect("export HookFilter");
    }

    #[test]
    fn export_bindings_hook_context() {
        HookContext::export().expect("export HookContext");
    }
}
