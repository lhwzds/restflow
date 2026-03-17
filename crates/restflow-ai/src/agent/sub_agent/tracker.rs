use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::Duration;

use crate::Result;
use crate::error::AiError;
use restflow_trace::RunTraceLifecycleSink;

use super::trace::{RunTraceEmitterFactory, RunTraceSink};

pub use restflow_traits::subagent::{
    SubagentCompletion, SubagentResult, SubagentState, SubagentStatus,
};

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

    /// Optional lifecycle sink for persisting sub-agent run boundaries.
    trace_lifecycle_sink: RwLock<Option<Arc<dyn RunTraceLifecycleSink>>>,

    /// Optional emitter factory for streaming tool-call trace events.
    trace_emitter_factory: RwLock<Option<Arc<dyn RunTraceEmitterFactory>>>,
}

impl SubagentTracker {
    fn is_terminal_status(status: &SubagentStatus) -> bool {
        matches!(
            status,
            SubagentStatus::Completed
                | SubagentStatus::Failed
                | SubagentStatus::Interrupted
                | SubagentStatus::TimedOut
        )
    }

    fn completion_for_state(id: &str, state: &SubagentState) -> SubagentCompletion {
        SubagentCompletion {
            id: id.to_string(),
            status: state.status.clone(),
            result: state.result.clone(),
        }
    }

    fn interrupted_result() -> SubagentResult {
        SubagentResult {
            success: false,
            output: String::new(),
            summary: None,
            duration_ms: 0,
            tokens_used: None,
            cost_usd: None,
            error: Some("Sub-agent interrupted".to_string()),
        }
    }

    fn try_mark_terminal(
        &self,
        id: &str,
        status: SubagentStatus,
        result: Option<SubagentResult>,
    ) -> bool {
        if let Some(mut state) = self.states.get_mut(id) {
            if Self::is_terminal_status(&state.status) {
                self.abort_handles.remove(id);
                self.completion_waiters.remove(id);
                return false;
            }

            state.status = status.clone();
            state.completed_at = Some(chrono::Utc::now().timestamp_millis());
            state.result = result.clone();

            self.abort_handles.remove(id);
            self.completion_waiters.remove(id);

            let _ = self.completion_tx.try_send(SubagentCompletion {
                id: id.to_string(),
                status,
                result,
            });
            return true;
        }

        false
    }

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
            trace_lifecycle_sink: RwLock::new(None),
            trace_emitter_factory: RwLock::new(None),
        }
    }

    /// Install or replace the trace sink used for spawned sub-agents.
    pub fn set_run_trace_sink<T>(&self, sink: Arc<T>)
    where
        T: RunTraceSink + 'static,
    {
        let lifecycle_sink: Arc<dyn RunTraceLifecycleSink> = sink.clone();
        let emitter_factory: Arc<dyn RunTraceEmitterFactory> = sink;

        if let Ok(mut guard) = self.trace_lifecycle_sink.write() {
            *guard = Some(lifecycle_sink);
        }

        if let Ok(mut guard) = self.trace_emitter_factory.write() {
            *guard = Some(emitter_factory);
        }
    }

    pub(crate) fn trace_lifecycle_sink(&self) -> Option<Arc<dyn RunTraceLifecycleSink>> {
        self.trace_lifecycle_sink
            .read()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub(crate) fn trace_emitter_factory(&self) -> Option<Arc<dyn RunTraceEmitterFactory>> {
        self.trace_emitter_factory
            .read()
            .ok()
            .and_then(|guard| guard.clone())
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
            return Err(AiError::Agent(format!("Sub-agent id already exists: {id}")));
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
        self.states
            .iter()
            .map(|record| record.value().clone())
            .collect()
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
    pub async fn wait(&self, id: &str) -> Option<SubagentCompletion> {
        loop {
            let state = self.states.get(id)?;
            if state.result.is_some() {
                return Some(Self::completion_for_state(id, &state));
            }

            if !matches!(
                state.status,
                SubagentStatus::Pending | SubagentStatus::Running
            ) {
                return Some(Self::completion_for_state(id, &state));
            }

            drop(state);
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Wait for all running sub-agents to complete.
    pub async fn wait_all(&self) -> Vec<SubagentCompletion> {
        let ids: Vec<String> = self
            .abort_handles
            .iter()
            .map(|record| record.key().clone())
            .collect();

        let mut results = Vec::new();
        for id in ids {
            if let Some(result) = self.wait(&id).await {
                results.push(result);
            }
        }
        results
    }

    /// Wait for any sub-agent to complete.
    pub async fn wait_any(&self) -> Option<SubagentCompletion> {
        let mut rx = self.completion_rx.lock().await;
        rx.recv().await
    }

    /// Cancel a running sub-agent.
    pub fn cancel(&self, id: &str) -> bool {
        if let Some((_, handle)) = self.abort_handles.remove(id) {
            handle.abort();
            self.completion_waiters.remove(id);
            let _ = self.try_mark_terminal(
                id,
                SubagentStatus::Interrupted,
                Some(Self::interrupted_result()),
            );
            true
        } else {
            false
        }
    }

    /// Cancel all running sub-agents.
    pub fn cancel_all(&self) -> usize {
        let ids: Vec<String> = self
            .abort_handles
            .iter()
            .map(|record| record.key().clone())
            .collect();
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
    /// This will not overwrite status if the sub-agent was already interrupted or timed out.
    pub fn mark_completed(&self, id: &str, result: SubagentResult) {
        let status = if result.success {
            SubagentStatus::Completed
        } else {
            SubagentStatus::Failed
        };
        let _ = self.try_mark_terminal(id, status, Some(result));
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
        let _ = self.try_mark_terminal(id, SubagentStatus::TimedOut, Some(result));
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::{mpsc, oneshot};
    use tokio::time::Duration;

    use super::*;
    use crate::agent::{NullEmitter, StreamEmitter};
    use restflow_trace::{
        RunTraceContext, RunTraceOutcome, TraceEvent, TraceEventKind, TraceEventSink,
    };

    #[derive(Default)]
    struct MockRunTraceSink {
        started: AtomicUsize,
        finished: AtomicUsize,
        emitters_built: AtomicUsize,
    }

    impl TraceEventSink for MockRunTraceSink {
        fn record_trace_event(&self, event: &TraceEvent) {
            match event.kind {
                TraceEventKind::RunStarted => {
                    self.started.fetch_add(1, Ordering::Relaxed);
                }
                TraceEventKind::RunCompleted { .. } | TraceEventKind::RunFailed { .. } => {
                    self.finished.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }
        }
    }

    impl RunTraceEmitterFactory for MockRunTraceSink {
        fn build_run_emitter(&self, _context: &RunTraceContext) -> Box<dyn StreamEmitter> {
            self.emitters_built.fetch_add(1, Ordering::Relaxed);
            Box::new(NullEmitter)
        }
    }

    #[tokio::test]
    async fn mark_completed_does_not_overwrite_interrupted() {
        let (tx, _rx) = mpsc::channel(1);
        let (_completion_tx, completion_rx) = mpsc::channel(1);
        let tracker = Arc::new(SubagentTracker::new(tx, completion_rx));

        let state = SubagentState {
            id: "test-id".to_string(),
            agent_name: "test-agent".to_string(),
            task: "test task".to_string(),
            status: SubagentStatus::Interrupted,
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
        assert_eq!(final_state.status, SubagentStatus::Interrupted);
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
    async fn mark_timed_out_does_not_overwrite_interrupted() {
        let (tx, _rx) = mpsc::channel(1);
        let (_completion_tx, completion_rx) = mpsc::channel(1);
        let tracker = Arc::new(SubagentTracker::new(tx, completion_rx));

        let state = SubagentState {
            id: "test-id-3".to_string(),
            agent_name: "test-agent".to_string(),
            task: "test task".to_string(),
            status: SubagentStatus::Interrupted,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: Some(chrono::Utc::now().timestamp_millis()),
            result: Some(SubagentTracker::interrupted_result()),
        };
        tracker.states.insert("test-id-3".to_string(), state);

        let result = SubagentResult {
            success: false,
            output: String::new(),
            summary: None,
            duration_ms: 100,
            tokens_used: None,
            cost_usd: None,
            error: Some("Sub-agent timed out".to_string()),
        };
        tracker.mark_timed_out_with_result("test-id-3", result);

        let final_state = tracker.states.get("test-id-3").unwrap();
        assert_eq!(final_state.status, SubagentStatus::Interrupted);
        assert_eq!(
            final_state
                .result
                .as_ref()
                .and_then(|value| value.error.as_deref()),
            Some("Sub-agent interrupted")
        );
    }

    #[tokio::test]
    async fn cancel_then_complete_race_keeps_interrupted_status() {
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

        {
            let state_after_cancel = tracker.states.get("race-test").unwrap();
            assert_eq!(state_after_cancel.status, SubagentStatus::Interrupted);
        }

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
        assert_eq!(final_state.status, SubagentStatus::Interrupted);

        let _ = abort_tx.send(());
    }

    #[tokio::test]
    async fn cancel_then_timeout_race_keeps_interrupted_status() {
        let (tx, _rx) = mpsc::channel(1);
        let (_completion_tx, completion_rx) = mpsc::channel(1);
        let tracker = Arc::new(SubagentTracker::new(tx, completion_rx));

        let (abort_tx, abort_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async {
            let _ = abort_rx.await;
        });
        let abort_handle = handle.abort_handle();

        let state = SubagentState {
            id: "timeout-race".to_string(),
            agent_name: "test-agent".to_string(),
            task: "test task".to_string(),
            status: SubagentStatus::Running,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: None,
            result: None,
        };
        tracker.states.insert("timeout-race".to_string(), state);
        tracker
            .abort_handles
            .insert("timeout-race".to_string(), abort_handle);

        tracker.cancel("timeout-race");
        tracker.mark_timed_out_with_result(
            "timeout-race",
            SubagentResult {
                success: false,
                output: String::new(),
                summary: None,
                duration_ms: 50,
                tokens_used: None,
                cost_usd: None,
                error: Some("Sub-agent timed out".to_string()),
            },
        );

        let final_state = tracker.states.get("timeout-race").unwrap();
        assert_eq!(final_state.status, SubagentStatus::Interrupted);
        assert_eq!(
            final_state
                .result
                .as_ref()
                .and_then(|value| value.error.as_deref()),
            Some("Sub-agent interrupted")
        );

        let _ = abort_tx.send(());
    }

    #[tokio::test]
    async fn wait_returns_interrupted_completion_after_cancel() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));

        let handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            SubagentResult {
                success: true,
                output: "late".to_string(),
                summary: None,
                duration_ms: 10_000,
                tokens_used: None,
                cost_usd: None,
                error: None,
            }
        });
        let (_completion_tx, completion_rx) = oneshot::channel();

        tracker.register(
            "cancelled".to_string(),
            "tester".to_string(),
            "cancel me".to_string(),
            handle,
            completion_rx,
        );

        assert!(tracker.cancel("cancelled"));
        let completion = tracker
            .wait("cancelled")
            .await
            .expect("cancelled task should yield a terminal completion");
        assert_eq!(completion.status, SubagentStatus::Interrupted);
        assert_eq!(
            completion.result.and_then(|result| result.error).as_deref(),
            Some("Sub-agent interrupted")
        );
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
        let result = result.result.expect("completed task payload");
        assert!(result.success);
        assert_eq!(result.output, "done");
    }

    #[tokio::test]
    async fn set_run_trace_sink_installs_lifecycle_and_emitter_dependencies() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let sink = Arc::new(MockRunTraceSink::default());
        tracker.set_run_trace_sink(sink.clone());

        let context = RunTraceContext {
            run_id: "run-1".to_string(),
            actor_id: "worker".to_string(),
            parent_run_id: Some("parent-1".to_string()),
            session_id: "session-1".to_string(),
            scope_id: "scope-1".to_string(),
        };

        let lifecycle_sink = tracker
            .trace_lifecycle_sink()
            .expect("lifecycle sink should be installed");
        lifecycle_sink.on_run_started(&context);
        lifecycle_sink.on_run_finished(
            &context,
            &RunTraceOutcome {
                success: true,
                error: None,
            },
        );

        let emitter_factory = tracker
            .trace_emitter_factory()
            .expect("emitter factory should be installed");
        let mut emitter = emitter_factory.build_run_emitter(&context);
        emitter.emit_complete().await;

        assert_eq!(sink.started.load(Ordering::Relaxed), 1);
        assert_eq!(sink.finished.load(Ordering::Relaxed), 1);
        assert_eq!(sink.emitters_built.load(Ordering::Relaxed), 1);
    }
}
