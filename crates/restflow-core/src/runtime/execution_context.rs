//! Shared execution context metadata across main, background, and sub-agent flows.

use serde::{Deserialize, Serialize};

/// High-level runtime role for an execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionRole {
    /// Foreground interactive chat turn.
    MainAgent,
    /// Scheduled or manually triggered task run.
    Task,
    /// Child agent spawned by another agent.
    Subagent,
}

impl ExecutionRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MainAgent => "main_agent",
            Self::Task => "task",
            Self::Subagent => "subagent",
        }
    }
}

/// Common context envelope used to describe an execution identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub role: ExecutionRole,
    pub agent_id: String,
    pub chat_session_id: Option<String>,
    #[serde(alias = "task_id")]
    pub task_id: Option<String>,
    #[serde(rename = "parent_run_id", alias = "parent_execution_id")]
    pub parent_run_id: Option<String>,
}

impl ExecutionContext {
    pub fn main(agent_id: impl Into<String>, chat_session_id: impl Into<String>) -> Self {
        Self {
            role: ExecutionRole::MainAgent,
            agent_id: agent_id.into(),
            chat_session_id: Some(chat_session_id.into()),
            task_id: None,
            parent_run_id: None,
        }
    }

    pub fn background(
        agent_id: impl Into<String>,
        chat_session_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        Self {
            role: ExecutionRole::Task,
            agent_id: agent_id.into(),
            chat_session_id: Some(chat_session_id.into()),
            task_id: Some(task_id.into()),
            parent_run_id: None,
        }
    }

    pub fn subagent(agent_id: impl Into<String>, parent_run_id: impl Into<String>) -> Self {
        Self {
            role: ExecutionRole::Subagent,
            agent_id: agent_id.into(),
            chat_session_id: None,
            task_id: None,
            parent_run_id: Some(parent_run_id.into()),
        }
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_context_sets_session() {
        let context = ExecutionContext::main("agent-1", "session-1");
        assert_eq!(context.role, ExecutionRole::MainAgent);
        assert_eq!(context.chat_session_id.as_deref(), Some("session-1"));
        assert!(context.task_id.is_none());
    }

    #[test]
    fn background_context_sets_task_and_session() {
        let context = ExecutionContext::background("agent-1", "session-1", "task-1");
        assert_eq!(context.role, ExecutionRole::Task);
        assert_eq!(context.chat_session_id.as_deref(), Some("session-1"));
        assert_eq!(context.task_id.as_deref(), Some("task-1"));
    }

    #[test]
    fn subagent_context_sets_parent_run() {
        let context = ExecutionContext::subagent("agent-2", "exec-1");
        assert_eq!(context.role, ExecutionRole::Subagent);
        assert_eq!(context.parent_run_id.as_deref(), Some("exec-1"));
        assert!(context.chat_session_id.is_none());
        assert!(context.task_id.is_none());
    }

    #[test]
    fn subagent_context_serializes_parent_run_id() {
        let context = ExecutionContext::subagent("agent-2", "exec-1");
        let value = context.to_value();
        assert_eq!(value["parent_run_id"], "exec-1");
        assert!(value.get("parent_execution_id").is_none());
    }

    #[test]
    fn role_as_str_is_stable() {
        assert_eq!(ExecutionRole::MainAgent.as_str(), "main_agent");
        assert_eq!(ExecutionRole::Task.as_str(), "task");
        assert_eq!(ExecutionRole::Subagent.as_str(), "subagent");
    }

    #[test]
    fn context_serializes_to_json_value() {
        let context = ExecutionContext::background("agent-1", "session-1", "task-1");
        let value = context.to_value();
        assert_eq!(value["role"], "task");
        assert_eq!(value["agent_id"], "agent-1");
        assert_eq!(value["chat_session_id"], "session-1");
        assert_eq!(value["task_id"], "task-1");
    }
}
