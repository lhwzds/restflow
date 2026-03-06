use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::Duration;

use crate::error::AiError;
use crate::Result;

use super::trace::RunTraceSink;

pub use restflow_traits::subagent::{SubagentCompletion, SubagentResult, SubagentState, SubagentStatus};

/// Sub-agent tracker with concurrent access support.
pub struct SubagentTracker {
    /// All sub-agent states.
    states: DashMap<String, SubagentState>,

    /// Abort handles for cancelling running sub-agents.
    abort_handles: DashMap<String, AbortHandle>,

    /// Completion waiters for sub-agent results.
    completion_waiters: DashMap<String, oneshot::Receiver<SubagentResult>>,

    /// Completion notification sender.
    completion_tx: mpsc::Sender<SubagentCompletion>,

    /// Completion notification receiver.
    completion_rx: Mutex<mpsc::Receiver<SubagentCompletion>>,

    /// Lock to prevent TOCTOU race between running_count() check and register().
    spawn_lock: std::sync::Mutex<()>,

    /// Optional sink for persisting sub-agent traces outside in-memory tracker state.
    trace_sink: RwLock<Option<Arc<dyn RunTraceSink>>>,
}

impl SubagentTracker {
    /// Create a new tracker.
    pub fn new(
        completion_tx: mpsc::Sender<SubagentCompletion>,
        completion_rx: mpsc::Receiver<SubagentCompletion>,
    ) -> Self {
        Self {
            states: DashMap::new(),
            abort_handles: DashMap::new(),
            completion_waiters: DashMap::new(),
            completion_tx,
            completion_rx: Mutex::new(completion_rx),
            spawn_lock: std::sync::Mutex::new(()),
            trace_sink: RwLock::new(None),
        }
    }

    /// Install or replace the trace sink used for spawned sub-agents.
    pub fn set_run_trace_sink(&self, sink: Arc<dyn RunTraceSink>) {
        if let Ok(mut guard) = self.trace_sink.write() {
            *guard = Some(sink);
        }
    }

    pub(crate) fn trace_sink(&self) -> Option<Arc<dyn RunTraceSink>> {
        self.trace_sink.read().ok().and_then(|guard| guard.clone())
    }

    fn insert_running_state(&self, id: String, agent_name: String, task: String) {
        let state = SubagentState {
            id: id.clone(),
            agent_name,
            task,
            status: SubagentStatus::Running,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: None,
            result: None,
        };
        self.states.insert(id, state);
    }

    fn spawn_join_monitor(self: &Arc<Self>, id: String, handle: JoinHandle<SubagentResult>) {
        let tracker = Arc::clone(self);
        tokio::spawn(async move {
            let join_result = handle.await;
            if tracker
                .get(&id)
                .and_then(|state| state.result.clone())
                .is_some()
            {
                return;
            }

            match join_result {
                Ok(result) => {
                    tracker.mark_completed(&id, result);
                }
                Err(error) => {
                    let result = SubagentResult {
                        success: false,
                        output: String::new(),
                        summary: None,
                        duration_ms: 0,
                        tokens_used: None,
                        cost_usd: None,
                        error: Some(format!("Task panicked: {error}")),
                    };
                    tracker.mark_completed(&id, result);
                }
            }
        });
    }

    pub(crate) fn attach_execution(
        self: &Arc<Self>,
        id: String,
        handle: JoinHandle<SubagentResult>,
        completion_rx: oneshot::Receiver<SubagentResult>,
    ) -> Result<()> {
        if !self.states.contains_key(&id) {
            return Err(AiError::Agent(format!(
                "Cannot attach sub-agent execution for unknown id: {id}"
            )));
        }

        let abort_handle = handle.abort_handle();
        self.abort_handles.insert(id.clone(), abort_handle);
        self.completion_waiters.insert(id.clone(), completion_rx);
        self.spawn_join_monitor(id, handle);
        Ok(())
    }

    /// Register a new sub-agent.
    pub fn register(
        self: &Arc<Self>,
        id: String,
        agent_name: String,
        task: String,
        handle: JoinHandle<SubagentResult>,
        completion_rx: oneshot::Receiver<SubagentResult>,
    ) {
        self.cleanup_completed(300_000);
        self.insert_running_state(id.clone(), agent_name, task);
        let _ = self.attach_execution(id, handle, completion_rx);
    }

    /// Atomically reserve a sub-agent slot and register running state.
    pub fn try_reserve(
        self: &Arc<Self>,
        max_parallel: usize,
        id: String,
        agent_name: String,
        task: String,
    ) -> Result<()> {
        let _guard = self
            .spawn_lock
            .lock()
            .map_err(|_| AiError::Agent("spawn lock poisoned".to_string()))?;

        self.cleanup_completed(300_000);

        let running = self.running_count();
        if running >= max_parallel {
            return Err(AiError::Agent(format!(
                "Max parallel agents ({max_parallel}) reached"
            )));
        }
        if self.states.contains_key(&id) {
            return Err(AiError::Agent(format!(
                "Sub-agent id already exists: {id}"
            )));
        }
        self.insert_running_state(id, agent_name, task);
        Ok(())
    }

    /// Atomically check the running count and register a new sub-agent.
    pub fn try_register(
        self: &Arc<Self>,
        max_parallel: usize,
        id: String,
        agent_name: String,
        task: String,
        handle: JoinHandle<SubagentResult>,
        completion_rx: oneshot::Receiver<SubagentResult>,
    ) -> Result<()> {
        self.try_reserve(max_parallel, id.clone(), agent_name, task)?;
        self.attach_execution(id, handle, completion_rx)
    }

    /// Get state of a specific sub-agent.
    pub fn get(&self, id: &str) -> Option<SubagentState> {
        self.states.get(id).map(|record| record.clone())
    }

    /// Get all sub-agent states.
    pub fn all(&self) -> Vec<SubagentState> {
        self.states.iter().map(|record| record.value().clone()).collect()
    }

    /// Get all running sub-agents.
    pub fn running(&self) -> Vec<SubagentState> {
        self.states
            .iter()
            .filter(|record| matches!(record.value().status, SubagentStatus::Running))
            .map(|record| record.value().clone())
            .collect()
    }

    /// Get count of running sub-agents.
    pub fn running_count(&self) -> usize {
        self.states
            .iter()
            .filter(|record| matches!(record.value().status, SubagentStatus::Running))
            .count()
    }

    /// Check if a sub-agent is running.
    pub fn is_running(&self, id: &str) -> bool {
        self.states
            .get(id)
            .map(|record| matches!(record.status, SubagentStatus::Running))
            .unwrap_or(false)
    }

    /// Wait for a specific sub-agent to complete.
    pub async fn wait(&self, id: &str) -> Option<SubagentResult> {
        loop {
            let state = self.states.get(id)?;
            if let Some(result) = state.result.clone() {
                return Some(result);
            }

            if !matches!(
                state.status,
                SubagentStatus::Pending | SubagentStatus::Running
            ) {
                return None;
            }

            drop(state);
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Wait for all running sub-agents to complete.
    pub async fn wait_all(&self) -> Vec<SubagentResult> {
        let ids: Vec<String> = self.abort_handles.iter().map(|record| record.key().clone()).collect();

        let mut results = Vec::new();
        for id in ids {
            if let Some(result) = self.wait(&id).await {
                results.push(result);
            }
        }
        results
    }

    /// Wait for any sub-agent to complete.
    pub async fn wait_any(&self) -> Option<(String, SubagentResult)> {
        let mut rx = self.completion_rx.lock().await;
        rx.recv()
            .await
            .map(|completion| (completion.id, completion.result))
    }

    /// Cancel a running sub-agent.
    pub fn cancel(&self, id: &str) -> bool {
        if let Some((_, handle)) = self.abort_handles.remove(id) {
            handle.abort();
            self.completion_waiters.remove(id);
            if let Some(mut state) = self.states.get_mut(id) {
                state.status = SubagentStatus::Cancelled;
                state.completed_at = Some(chrono::Utc::now().timestamp_millis());
            }
            true
        } else {
            false
        }
    }

    /// Cancel all running sub-agents.
    pub fn cancel_all(&self) -> usize {
        let ids: Vec<String> = self.abort_handles.iter().map(|record| record.key().clone()).collect();
        let mut cancelled = 0;
        for id in ids {
            if self.cancel(&id) {
                cancelled += 1;
            }
        }
        cancelled
    }

    /// Mark a sub-agent as completed.
    ///
    /// This will not overwrite status if the sub-agent was already cancelled or timed out.
    pub fn mark_completed(&self, id: &str, result: SubagentResult) {
        if let Some(state) = self.states.get(id)
            && matches!(
                state.status,
                SubagentStatus::Cancelled | SubagentStatus::TimedOut
            )
        {
            self.abort_handles.remove(id);
            self.completion_waiters.remove(id);
            return;
        }

        if let Some(mut state) = self.states.get_mut(id) {
            state.status = if result.success {
                SubagentStatus::Completed
            } else {
                SubagentStatus::Failed
            };
            state.completed_at = Some(chrono::Utc::now().timestamp_millis());
            state.result = Some(result.clone());
        }

        self.abort_handles.remove(id);
        self.completion_waiters.remove(id);

        let _ = self.completion_tx.try_send(SubagentCompletion {
            id: id.to_string(),
            result,
        });
    }

    /// Mark a sub-agent as timed out.
    pub fn mark_timed_out(&self, id: &str) {
        self.mark_timed_out_with_result(
            id,
            SubagentResult {
                success: false,
                output: String::new(),
                summary: None,
                duration_ms: 0,
                tokens_used: None,
                cost_usd: None,
                error: Some("Sub-agent timed out".to_string()),
            },
        );
    }

    /// Mark a sub-agent as timed out with a specific result.
    pub fn mark_timed_out_with_result(&self, id: &str, result: SubagentResult) {
        if let Some(mut state) = self.states.get_mut(id) {
            state.status = SubagentStatus::TimedOut;
            state.completed_at = Some(chrono::Utc::now().timestamp_millis());
            state.result = Some(result.clone());
        }

        self.abort_handles.remove(id);
        self.completion_waiters.remove(id);

        let _ = self.completion_tx.try_send(SubagentCompletion {
            id: id.to_string(),
            result,
        });
    }

    /// Clean up completed sub-agents older than the given age.
    pub fn cleanup_completed(&self, max_age_ms: i64) {
        let now = chrono::Utc::now().timestamp_millis();
        let to_remove: Vec<String> = self
            .states
            .iter()
            .filter(|record| {
                if let Some(completed_at) = record.completed_at {
                    now - completed_at > max_age_ms
                } else {
                    false
                }
            })
            .map(|record| record.key().clone())
            .collect();

        for id in to_remove {
            self.states.remove(&id);
        }
    }

    /// Get the completion sender for external use.
    pub fn completion_sender(&self) -> mpsc::Sender<SubagentCompletion> {
        self.completion_tx.clone()
    }

    /// Poll completion notifications without blocking.
    pub async fn poll_completions(&self) -> Vec<SubagentCompletion> {
        let mut rx = self.completion_rx.lock().await;
        let mut completions = Vec::new();

        while let Ok(completion) = rx.try_recv() {
            completions.push(completion);
        }

        completions
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::{mpsc, oneshot};
    use tokio::time::Duration;

    use super::*;

    #[tokio::test]
    async fn mark_completed_does_not_overwrite_cancelled() {
        let (tx, _rx) = mpsc::channel(1);
        let (_completion_tx, completion_rx) = mpsc::channel(1);
        let tracker = Arc::new(SubagentTracker::new(tx, completion_rx));

        let state = SubagentState {
            id: "test-id".to_string(),
            agent_name: "test-agent".to_string(),
            task: "test task".to_string(),
            status: SubagentStatus::Cancelled,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: Some(chrono::Utc::now().timestamp_millis()),
            result: None,
        };
        tracker.states.insert("test-id".to_string(), state);

        let result = SubagentResult {
            success: true,
            output: "should not overwrite".to_string(),
            summary: None,
            duration_ms: 100,
            tokens_used: None,
            cost_usd: None,
            error: None,
        };
        tracker.mark_completed("test-id", result);

        let final_state = tracker.states.get("test-id").unwrap();
        assert_eq!(final_state.status, SubagentStatus::Cancelled);
    }

    #[tokio::test]
    async fn mark_completed_does_not_overwrite_timed_out() {
        let (tx, _rx) = mpsc::channel(1);
        let (_completion_tx, completion_rx) = mpsc::channel(1);
        let tracker = Arc::new(SubagentTracker::new(tx, completion_rx));

        let state = SubagentState {
            id: "test-id-2".to_string(),
            agent_name: "test-agent".to_string(),
            task: "test task".to_string(),
            status: SubagentStatus::TimedOut,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: Some(chrono::Utc::now().timestamp_millis()),
            result: None,
        };
        tracker.states.insert("test-id-2".to_string(), state);

        let result = SubagentResult {
            success: true,
            output: "should not overwrite".to_string(),
            summary: None,
            duration_ms: 100,
            tokens_used: None,
            cost_usd: None,
            error: None,
        };
        tracker.mark_completed("test-id-2", result);

        let final_state = tracker.states.get("test-id-2").unwrap();
        assert_eq!(final_state.status, SubagentStatus::TimedOut);
    }

    #[tokio::test]
    async fn cancel_then_complete_race_keeps_cancelled_status() {
        let (tx, _rx) = mpsc::channel(1);
        let (_completion_tx, completion_rx) = mpsc::channel(1);
        let tracker = Arc::new(SubagentTracker::new(tx, completion_rx));

        let (abort_tx, abort_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async {
            let _ = abort_rx.await;
        });
        let abort_handle = handle.abort_handle();

        let state = SubagentState {
            id: "race-test".to_string(),
            agent_name: "test-agent".to_string(),
            task: "test task".to_string(),
            status: SubagentStatus::Running,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: None,
            result: None,
        };
        tracker.states.insert("race-test".to_string(), state);
        tracker
            .abort_handles
            .insert("race-test".to_string(), abort_handle);

        tracker.cancel("race-test");

        let state_after_cancel = tracker.states.get("race-test").unwrap();
        assert_eq!(state_after_cancel.status, SubagentStatus::Cancelled);

        let result = SubagentResult {
            success: false,
            output: String::new(),
            summary: None,
            duration_ms: 50,
            tokens_used: None,
            cost_usd: None,
            error: Some("Task aborted".to_string()),
        };

        tracker.mark_completed("race-test", result);

        let final_state = tracker.states.get("race-test").unwrap();
        assert_eq!(final_state.status, SubagentStatus::Cancelled);

        let _ = abort_tx.send(());
    }

    #[tokio::test]
    async fn wait_timeout_is_retryable() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));

        let (completion_tx, completion_rx) = oneshot::channel();
        let task_id = "wait-retry-test".to_string();

        let handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(120)).await;
            let result = SubagentResult {
                success: true,
                output: "done".to_string(),
                summary: None,
                duration_ms: 120,
                tokens_used: None,
                cost_usd: None,
                error: None,
            };
            let _ = completion_tx.send(result.clone());
            result
        });

        tracker.register(
            task_id.clone(),
            "tester".to_string(),
            "retry wait".to_string(),
            handle,
            completion_rx,
        );

        let first_wait =
            tokio::time::timeout(Duration::from_millis(20), tracker.wait(&task_id)).await;
        assert!(first_wait.is_err());

        let second_wait =
            tokio::time::timeout(Duration::from_secs(1), tracker.wait(&task_id)).await;
        assert!(second_wait.is_ok());

        let result = second_wait
            .expect("second wait future should finish")
            .expect("completed task should return result");
        assert!(result.success);
        assert_eq!(result.output, "done");
    }
}
