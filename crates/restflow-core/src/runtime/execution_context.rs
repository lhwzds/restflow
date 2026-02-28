//! Shared execution context metadata across main, background, and sub-agent flows.

use serde::{Deserialize, Serialize};

/// High-level runtime role for an execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionRole {
    /// Foreground interactive chat turn.
    MainAgent,
    /// Scheduled or manually triggered background task run.
    BackgroundAgent,
    /// Child agent spawned by another agent.
    Subagent,
}

impl ExecutionRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MainAgent => "main_agent",
            Self::BackgroundAgent => "background_agent",
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
    pub background_task_id: Option<String>,
    pub parent_execution_id: Option<String>,
}

impl ExecutionContext {
    pub fn main(agent_id: impl Into<String>, chat_session_id: impl Into<String>) -> Self {
        Self {
            role: ExecutionRole::MainAgent,
            agent_id: agent_id.into(),
            chat_session_id: Some(chat_session_id.into()),
            background_task_id: None,
            parent_execution_id: None,
        }
    }

    pub fn background(
        agent_id: impl Into<String>,
        chat_session_id: impl Into<String>,
        background_task_id: impl Into<String>,
    ) -> Self {
        Self {
            role: ExecutionRole::BackgroundAgent,
            agent_id: agent_id.into(),
            chat_session_id: Some(chat_session_id.into()),
            background_task_id: Some(background_task_id.into()),
            parent_execution_id: None,
        }
    }

    pub fn subagent(agent_id: impl Into<String>, parent_execution_id: impl Into<String>) -> Self {
        Self {
            role: ExecutionRole::Subagent,
            agent_id: agent_id.into(),
            chat_session_id: None,
            background_task_id: None,
            parent_execution_id: Some(parent_execution_id.into()),
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
        assert!(context.background_task_id.is_none());
    }

    #[test]
    fn background_context_sets_task_and_session() {
        let context = ExecutionContext::background("agent-1", "session-1", "task-1");
        assert_eq!(context.role, ExecutionRole::BackgroundAgent);
        assert_eq!(context.chat_session_id.as_deref(), Some("session-1"));
        assert_eq!(context.background_task_id.as_deref(), Some("task-1"));
    }

    #[test]
    fn subagent_context_sets_parent_execution() {
        let context = ExecutionContext::subagent("agent-2", "exec-1");
        assert_eq!(context.role, ExecutionRole::Subagent);
        assert_eq!(context.parent_execution_id.as_deref(), Some("exec-1"));
        assert!(context.chat_session_id.is_none());
        assert!(context.background_task_id.is_none());
    }

    #[test]
    fn role_as_str_is_stable() {
        assert_eq!(ExecutionRole::MainAgent.as_str(), "main_agent");
        assert_eq!(ExecutionRole::BackgroundAgent.as_str(), "background_agent");
        assert_eq!(ExecutionRole::Subagent.as_str(), "subagent");
    }

    #[test]
    fn context_serializes_to_json_value() {
        let context = ExecutionContext::background("agent-1", "session-1", "task-1");
        let value = context.to_value();
        assert_eq!(value["role"], "background_agent");
        assert_eq!(value["agent_id"], "agent-1");
        assert_eq!(value["chat_session_id"], "session-1");
        assert_eq!(value["background_task_id"], "task-1");
    }
}
