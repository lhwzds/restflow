//! Sub-agent tracking with concurrent access support.
//!
//! This module provides thread-safe tracking of running sub-agents
//! using DashMap for lock-free concurrent access.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use ts_rs::TS;

/// Sub-agent running state
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubagentState {
    /// Unique task ID
    pub id: String,

    /// Agent name (e.g., "researcher", "coder")
    pub agent_name: String,

    /// Task description
    pub task: String,

    /// Current status
    pub status: SubagentStatus,

    /// Start timestamp (Unix ms)
    pub started_at: i64,

    /// Completion timestamp (Unix ms)
    pub completed_at: Option<i64>,

    /// Result (when completed)
    pub result: Option<SubagentResult>,
}

/// Sub-agent status
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub enum SubagentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

/// Result from a sub-agent execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubagentResult {
    /// Whether execution succeeded
    pub success: bool,

    /// Output content
    pub output: String,

    /// Optional summary of the output
    pub summary: Option<String>,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Tokens used
    pub tokens_used: Option<u32>,

    /// Error message (if failed)
    pub error: Option<String>,
}

/// Completion notification
#[derive(Debug, Clone)]
pub struct SubagentCompletion {
    /// Task ID
    pub id: String,

    /// Execution result
    pub result: SubagentResult,
}

/// Sub-agent tracker with concurrent access support
pub struct SubagentTracker {
    /// All sub-agent states
    states: DashMap<String, SubagentState>,

    /// Tokio JoinHandles for waiting/cancelling
    handles: DashMap<String, JoinHandle<SubagentResult>>,

    /// Completion notification sender
    completion_tx: mpsc::Sender<SubagentCompletion>,
}

impl SubagentTracker {
    /// Create a new tracker
    pub fn new(completion_tx: mpsc::Sender<SubagentCompletion>) -> Self {
        Self {
            states: DashMap::new(),
            handles: DashMap::new(),
            completion_tx,
        }
    }

    /// Register a new sub-agent
    pub fn register(
        &self,
        id: String,
        agent_name: String,
        task: String,
        handle: JoinHandle<SubagentResult>,
    ) {
        let state = SubagentState {
            id: id.clone(),
            agent_name,
            task,
            status: SubagentStatus::Running,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: None,
            result: None,
        };

        self.states.insert(id.clone(), state);
        self.handles.insert(id, handle);
    }

    /// Get state of a specific sub-agent
    pub fn get(&self, id: &str) -> Option<SubagentState> {
        self.states.get(id).map(|r| r.clone())
    }

    /// Get all sub-agent states
    pub fn all(&self) -> Vec<SubagentState> {
        self.states.iter().map(|r| r.value().clone()).collect()
    }

    /// Get all running sub-agents
    pub fn running(&self) -> Vec<SubagentState> {
        self.states
            .iter()
            .filter(|r| matches!(r.value().status, SubagentStatus::Running))
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get count of running sub-agents
    pub fn running_count(&self) -> usize {
        self.states
            .iter()
            .filter(|r| matches!(r.value().status, SubagentStatus::Running))
            .count()
    }

    /// Check if a sub-agent is running
    pub fn is_running(&self, id: &str) -> bool {
        self.states
            .get(id)
            .map(|r| matches!(r.status, SubagentStatus::Running))
            .unwrap_or(false)
    }

    /// Wait for a specific sub-agent to complete
    pub async fn wait(&self, id: &str) -> Option<SubagentResult> {
        if let Some((_, handle)) = self.handles.remove(id) {
            match handle.await {
                Ok(result) => {
                    self.mark_completed(id, result.clone());
                    Some(result)
                }
                Err(e) => {
                    let result = SubagentResult {
                        success: false,
                        output: String::new(),
                        summary: None,
                        duration_ms: 0,
                        tokens_used: None,
                        error: Some(format!("Task panicked: {}", e)),
                    };
                    self.mark_completed(id, result.clone());
                    Some(result)
                }
            }
        } else {
            // Already completed, get from states
            self.states.get(id).and_then(|s| s.result.clone())
        }
    }

    /// Wait for all running sub-agents to complete
    pub async fn wait_all(&self) -> Vec<SubagentResult> {
        let ids: Vec<String> = self.handles.iter().map(|r| r.key().clone()).collect();

        let mut results = Vec::new();
        for id in ids {
            if let Some(result) = self.wait(&id).await {
                results.push(result);
            }
        }
        results
    }

    /// Wait for any sub-agent to complete
    pub async fn wait_any(&self) -> Option<(String, SubagentResult)> {
        let ids: Vec<String> = self.handles.iter().map(|r| r.key().clone()).collect();

        if ids.is_empty() {
            return None;
        }

        // Use select to wait for any completion
        // For simplicity, we just wait for the first one
        if let Some(id) = ids.first() {
            if let Some(result) = self.wait(id).await {
                return Some((id.clone(), result));
            }
        }

        None
    }

    /// Cancel a running sub-agent
    pub fn cancel(&self, id: &str) -> bool {
        if let Some((_, handle)) = self.handles.remove(id) {
            handle.abort();
            if let Some(mut state) = self.states.get_mut(id) {
                state.status = SubagentStatus::Cancelled;
                state.completed_at = Some(chrono::Utc::now().timestamp_millis());
            }
            true
        } else {
            false
        }
    }

    /// Cancel all running sub-agents
    pub fn cancel_all(&self) -> usize {
        let ids: Vec<String> = self.handles.iter().map(|r| r.key().clone()).collect();
        let mut cancelled = 0;
        for id in ids {
            if self.cancel(&id) {
                cancelled += 1;
            }
        }
        cancelled
    }

    /// Mark a sub-agent as completed
    pub fn mark_completed(&self, id: &str, result: SubagentResult) {
        if let Some(mut state) = self.states.get_mut(id) {
            state.status = if result.success {
                SubagentStatus::Completed
            } else {
                SubagentStatus::Failed
            };
            state.completed_at = Some(chrono::Utc::now().timestamp_millis());
            state.result = Some(result.clone());
        }

        // Remove the handle if it exists
        self.handles.remove(id);

        // Send completion notification
        let _ = self.completion_tx.try_send(SubagentCompletion {
            id: id.to_string(),
            result,
        });
    }

    /// Mark a sub-agent as timed out
    pub fn mark_timed_out(&self, id: &str) {
        if let Some(mut state) = self.states.get_mut(id) {
            state.status = SubagentStatus::TimedOut;
            state.completed_at = Some(chrono::Utc::now().timestamp_millis());
            state.result = Some(SubagentResult {
                success: false,
                output: String::new(),
                summary: None,
                duration_ms: 0,
                tokens_used: None,
                error: Some("Sub-agent timed out".to_string()),
            });
        }

        // Abort and remove the handle
        if let Some((_, handle)) = self.handles.remove(id) {
            handle.abort();
        }
    }

    /// Clean up completed sub-agents older than the given age
    pub fn cleanup_completed(&self, max_age_ms: i64) {
        let now = chrono::Utc::now().timestamp_millis();
        let to_remove: Vec<String> = self
            .states
            .iter()
            .filter(|r| {
                if let Some(completed_at) = r.completed_at {
                    now - completed_at > max_age_ms
                } else {
                    false
                }
            })
            .map(|r| r.key().clone())
            .collect();

        for id in to_remove {
            self.states.remove(&id);
        }
    }

    /// Get the completion sender for external use
    pub fn completion_sender(&self) -> mpsc::Sender<SubagentCompletion> {
        self.completion_tx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tracker_basic() {
        let (tx, _rx) = mpsc::channel(10);
        let tracker = SubagentTracker::new(tx);

        // Register a sub-agent
        let handle = tokio::spawn(async {
            SubagentResult {
                success: true,
                output: "Done".to_string(),
                summary: None,
                duration_ms: 100,
                tokens_used: Some(50),
                error: None,
            }
        });

        tracker.register(
            "task-1".to_string(),
            "researcher".to_string(),
            "Research X".to_string(),
            handle,
        );

        assert_eq!(tracker.running_count(), 1);
        assert!(tracker.is_running("task-1"));

        // Wait for completion
        let result = tracker.wait("task-1").await;
        assert!(result.is_some());
        assert!(result.unwrap().success);

        assert_eq!(tracker.running_count(), 0);
    }

    #[tokio::test]
    async fn test_tracker_cancel() {
        let (tx, _rx) = mpsc::channel(10);
        let tracker = SubagentTracker::new(tx);

        // Register a long-running sub-agent
        let handle = tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            SubagentResult {
                success: true,
                output: "Done".to_string(),
                summary: None,
                duration_ms: 60000,
                tokens_used: None,
                error: None,
            }
        });

        tracker.register(
            "task-1".to_string(),
            "coder".to_string(),
            "Code Y".to_string(),
            handle,
        );

        assert!(tracker.is_running("task-1"));

        // Cancel it
        assert!(tracker.cancel("task-1"));

        // Check state
        let state = tracker.get("task-1").unwrap();
        assert_eq!(state.status, SubagentStatus::Cancelled);
    }
}
