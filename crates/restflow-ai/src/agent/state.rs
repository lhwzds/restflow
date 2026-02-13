//! Agent state management

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::Message;

/// Agent execution status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentStatus {
    Running,
    Completed,
    Failed {
        error: String,
    },
    MaxIterations,
    /// Execution paused, awaiting external input before resuming.
    Interrupted {
        reason: String,
    },
    ResourceExhausted {
        error: String,
    },
}

/// Complete agent state - simplified Swarm-style design
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Execution ID
    pub execution_id: String,

    /// Current status
    pub status: AgentStatus,

    /// Message history (replaces separate thoughts/actions/observations)
    pub messages: Vec<Message>,

    /// Current iteration number
    pub iteration: usize,

    /// Maximum iterations allowed
    pub max_iterations: usize,

    /// Version counter for state changes (LangGraph-inspired, for Phase 3 checkpointing)
    pub version: u64,

    /// Hidden context not exposed to LLM (Swarm-inspired)
    pub context: HashMap<String, Value>,

    /// Final answer (if completed)
    pub final_answer: Option<String>,

    /// Execution timestamps
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl AgentState {
    /// Create a new agent state
    pub fn new(execution_id: String, max_iterations: usize) -> Self {
        Self {
            execution_id,
            status: AgentStatus::Running,
            messages: vec![],
            iteration: 0,
            max_iterations,
            version: 0,
            context: HashMap::new(),
            final_answer: None,
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    /// Add a message and bump version
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.version += 1;
    }

    /// Add tool result message
    pub fn add_tool_result(&mut self, tool_call_id: String, result: String) {
        self.add_message(Message::tool_result(tool_call_id, result));
    }

    /// Complete with final answer
    pub fn complete(&mut self, answer: impl Into<String>) {
        self.final_answer = Some(answer.into());
        self.status = AgentStatus::Completed;
        self.ended_at = Some(Utc::now());
        self.version += 1;
    }

    /// Mark as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = AgentStatus::Failed {
            error: error.into(),
        };
        self.ended_at = Some(Utc::now());
        self.version += 1;
    }

    /// Interrupt execution (for checkpoint/resume).
    pub fn interrupt(&mut self, reason: impl Into<String>) {
        self.status = AgentStatus::Interrupted {
            reason: reason.into(),
        };
        self.ended_at = Some(Utc::now());
        self.version += 1;
    }

    /// Mark as resource exhausted
    pub fn resource_exhaust(&mut self, error: impl Into<String>) {
        self.status = AgentStatus::ResourceExhausted {
            error: error.into(),
        };
        self.ended_at = Some(Utc::now());
        self.version += 1;
    }

    /// Check if the agent is interrupted.
    pub fn is_interrupted(&self) -> bool {
        matches!(self.status, AgentStatus::Interrupted { .. })
    }

    /// Check if terminal state
    pub fn is_terminal(&self) -> bool {
        !matches!(self.status, AgentStatus::Running)
    }

    /// Increment iteration, returns false if max reached
    pub fn increment_iteration(&mut self) -> bool {
        self.iteration += 1;
        if self.iteration >= self.max_iterations {
            self.status = AgentStatus::MaxIterations;
            self.ended_at = Some(Utc::now());
            self.version += 1;
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_state_new() {
        let state = AgentState::new("test-id".to_string(), 10);
        assert_eq!(state.execution_id, "test-id");
        assert_eq!(state.iteration, 0);
        assert_eq!(state.max_iterations, 10);
        assert_eq!(state.status, AgentStatus::Running);
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_agent_state_complete() {
        let mut state = AgentState::new("test-id".to_string(), 10);
        state.complete("done");

        assert_eq!(state.status, AgentStatus::Completed);
        assert_eq!(state.final_answer, Some("done".to_string()));
        assert!(state.is_terminal());
        assert!(state.ended_at.is_some());
    }

    #[test]
    fn test_agent_state_fail() {
        let mut state = AgentState::new("test-id".to_string(), 10);
        state.fail("error message");

        assert!(matches!(state.status, AgentStatus::Failed { .. }));
        assert!(state.is_terminal());
    }

    #[test]
    fn test_agent_state_interrupted() {
        let mut state = AgentState::new("test-id".to_string(), 10);
        state.interrupt("security approval needed");

        assert!(matches!(
            state.status,
            AgentStatus::Interrupted { ref reason } if reason == "security approval needed"
        ));
        assert!(state.is_terminal());
        assert!(state.is_interrupted());
        assert!(state.ended_at.is_some());
    }

    #[test]
    fn test_interrupt_increments_version() {
        let mut state = AgentState::new("test-id".to_string(), 10);
        let v_before = state.version;
        state.interrupt("test");
        assert_eq!(state.version, v_before + 1);
    }

    #[test]
    fn test_agent_state_max_iterations() {
        let mut state = AgentState::new("test-id".to_string(), 2);

        assert!(state.increment_iteration()); // iteration = 1
        assert!(!state.increment_iteration()); // iteration = 2, hits max

        assert_eq!(state.status, AgentStatus::MaxIterations);
        assert!(state.is_terminal());
    }

    #[test]
    fn test_agent_state_resource_exhausted() {
        let mut state = AgentState::new("test-id".to_string(), 10);
        state.resource_exhaust("Exceeded tool call limit: 201 calls (limit: 200)");

        assert!(matches!(
            state.status,
            AgentStatus::ResourceExhausted { .. }
        ));
        assert!(state.is_terminal());
        assert!(state.ended_at.is_some());
    }
}
