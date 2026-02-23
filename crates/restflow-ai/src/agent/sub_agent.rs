//! Sub-agent spawning support for tool-based execution.

use crate::agent::executor::{AgentConfig, AgentExecutor, AgentResult};
use crate::error::{AiError, Result};
use crate::llm::LlmClient;
use crate::tools::{FilteredToolset, ToolRegistry, Toolset};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::{Duration, timeout};

// Re-export data types from restflow-traits
pub use restflow_traits::subagent::{
    SpawnHandle, SpawnPriority, SpawnRequest, SubagentCompletion, SubagentConfig,
    SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentManager, SubagentResult,
    SubagentSpawner, SubagentState, SubagentStatus,
};
use restflow_traits::ToolError;

/// Sub-agent tracker with concurrent access support
pub struct SubagentTracker {
    /// All sub-agent states
    states: DashMap<String, SubagentState>,

    /// Abort handles for cancelling running sub-agents
    abort_handles: DashMap<String, AbortHandle>,

    /// Completion waiters for sub-agent results
    completion_waiters: DashMap<String, oneshot::Receiver<SubagentResult>>,

    /// Completion notification sender
    completion_tx: mpsc::Sender<SubagentCompletion>,

    /// Completion notification receiver
    completion_rx: Mutex<mpsc::Receiver<SubagentCompletion>>,

    /// Lock to prevent TOCTOU race between running_count() check and register()
    spawn_lock: std::sync::Mutex<()>,
}

impl SubagentTracker {
    /// Create a new tracker
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
        }
    }

    /// Register a new sub-agent
    pub fn register(
        self: &Arc<Self>,
        id: String,
        agent_name: String,
        task: String,
        handle: JoinHandle<SubagentResult>,
        completion_rx: oneshot::Receiver<SubagentResult>,
    ) {
        // Opportunistic cleanup of completed entries older than 5 minutes
        self.cleanup_completed(300_000);

        let state = SubagentState {
            id: id.clone(),
            agent_name,
            task,
            status: SubagentStatus::Running,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: None,
            result: None,
        };

        let abort_handle = handle.abort_handle();

        self.states.insert(id.clone(), state);
        self.abort_handles.insert(id.clone(), abort_handle);
        self.completion_waiters.insert(id.clone(), completion_rx);

        let tracker = Arc::clone(self);
        let id_for_task = id.clone();
        tokio::spawn(async move {
            let join_result = handle.await;
            if tracker
                .get(&id_for_task)
                .and_then(|state| state.result.clone())
                .is_some()
            {
                return;
            }

            match join_result {
                Ok(result) => {
                    tracker.mark_completed(&id_for_task, result);
                }
                Err(e) => {
                    let result = SubagentResult {
                        success: false,
                        output: String::new(),
                        summary: None,
                        duration_ms: 0,
                        tokens_used: None,
                        cost_usd: None,
                        error: Some(format!("Task panicked: {}", e)),
                    };
                    tracker.mark_completed(&id_for_task, result);
                }
            }
        });
    }

    /// Atomically check the running count and register a new sub-agent.
    /// Returns Err if the parallel limit is reached.
    /// This prevents the TOCTOU race between running_count() and register().
    pub fn try_register(
        self: &Arc<Self>,
        max_parallel: usize,
        id: String,
        agent_name: String,
        task: String,
        handle: JoinHandle<SubagentResult>,
        completion_rx: oneshot::Receiver<SubagentResult>,
    ) -> std::result::Result<(), AiError> {
        let _guard = self
            .spawn_lock
            .lock()
            .map_err(|_| AiError::Agent("spawn lock poisoned".to_string()))?;

        let running = self.running_count();
        if running >= max_parallel {
            return Err(AiError::Agent(format!(
                "Max parallel agents ({}) reached",
                max_parallel
            )));
        }

        self.register(id, agent_name, task, handle, completion_rx);
        Ok(())
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
        if let Some(state) = self.states.get(id)
            && let Some(result) = state.result.clone()
        {
            self.completion_waiters.remove(id);
            return Some(result);
        }

        if let Some((_, receiver)) = self.completion_waiters.remove(id) {
            match receiver.await {
                Ok(result) => {
                    if self.states.get(id).and_then(|s| s.result.clone()).is_none() {
                        self.mark_completed(id, result.clone());
                    }
                    return Some(result);
                }
                Err(_) => {
                    return self.states.get(id).and_then(|s| s.result.clone());
                }
            }
        }

        self.states.get(id).and_then(|s| s.result.clone())
    }

    /// Wait for all running sub-agents to complete
    pub async fn wait_all(&self) -> Vec<SubagentResult> {
        let ids: Vec<String> = self.abort_handles.iter().map(|r| r.key().clone()).collect();

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
        let mut rx = self.completion_rx.lock().await;
        rx.recv()
            .await
            .map(|completion| (completion.id, completion.result))
    }

    /// Cancel a running sub-agent
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

    /// Cancel all running sub-agents
    pub fn cancel_all(&self) -> usize {
        let ids: Vec<String> = self.abort_handles.iter().map(|r| r.key().clone()).collect();
        let mut cancelled = 0;
        for id in ids {
            if self.cancel(&id) {
                cancelled += 1;
            }
        }
        cancelled
    }

    /// Mark a sub-agent as completed
    ///
    /// NOTE: This method will NOT overwrite status if the sub-agent was already
    /// cancelled or timed out.
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

    /// Mark a sub-agent as timed out
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

    /// Mark a sub-agent as timed out with a specific result
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

    /// Poll completion notifications without blocking
    pub async fn poll_completions(&self) -> Vec<SubagentCompletion> {
        let mut rx = self.completion_rx.lock().await;
        let mut completions = Vec::new();

        while let Ok(completion) = rx.try_recv() {
            completions.push(completion);
        }

        completions
    }
}

/// Dependencies needed for sub-agent tools (spawn_agent, wait_agents, list_agents).
#[derive(Clone)]
pub struct SubagentDeps {
    pub tracker: Arc<SubagentTracker>,
    pub definitions: Arc<dyn SubagentDefLookup>,
    pub llm_client: Arc<dyn LlmClient>,
    pub tool_registry: Arc<ToolRegistry>,
    pub config: SubagentConfig,
}

/// Spawn a sub-agent with the given request.
pub fn spawn_subagent(
    tracker: Arc<SubagentTracker>,
    definitions: Arc<dyn SubagentDefLookup>,
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    config: SubagentConfig,
    request: SpawnRequest,
) -> Result<SpawnHandle> {
    let agent_def = definitions
        .lookup(&request.agent_id)
        .ok_or_else(|| AiError::Agent(format!("Unknown agent type: {}", request.agent_id)))?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let timeout_secs = request.timeout_secs.unwrap_or(config.subagent_timeout_secs);

    let agent_name_for_register = agent_def.name.clone();
    let agent_name_for_return = agent_def.name.clone();
    let task_for_register = request.task.clone();

    let task = request.task.clone();
    let tracker_clone = tracker.clone();
    let task_id_for_spawn = task_id.clone();
    let llm_client = llm_client.clone();
    let tool_registry = tool_registry.clone();
    let config_clone = config.clone();

    let (completion_tx, completion_rx) = oneshot::channel();
    let (start_tx, start_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let task_id = task_id_for_spawn;
        let _ = start_rx.await;
        let start = std::time::Instant::now();

        let result = timeout(
            Duration::from_secs(timeout_secs),
            execute_subagent(
                llm_client,
                tool_registry,
                agent_def,
                task.clone(),
                config_clone,
            ),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        let (subagent_result, timed_out) = match result {
            Ok(Ok(result)) => {
                let cost_usd = if result.total_cost_usd > 0.0 {
                    Some(result.total_cost_usd)
                } else {
                    None
                };
                (
                    SubagentResult {
                        success: true,
                        output: result.answer.unwrap_or_default(),
                        summary: None,
                        duration_ms,
                        tokens_used: Some(result.total_tokens),
                        cost_usd,
                        error: None,
                    },
                    false,
                )
            }
            Ok(Err(e)) => (
                SubagentResult {
                    success: false,
                    output: String::new(),
                    summary: None,
                    duration_ms,
                    tokens_used: None,
                    cost_usd: None,
                    error: Some(e.to_string()),
                },
                false,
            ),
            Err(_) => (
                SubagentResult {
                    success: false,
                    output: String::new(),
                    summary: None,
                    duration_ms,
                    tokens_used: None,
                    cost_usd: None,
                    error: Some("Sub-agent timed out".to_string()),
                },
                true,
            ),
        };

        if timed_out {
            tracker_clone.mark_timed_out_with_result(&task_id, subagent_result.clone());
        } else {
            tracker_clone.mark_completed(&task_id, subagent_result.clone());
        }

        let _ = completion_tx.send(subagent_result.clone());
        subagent_result
    });

    tracker.try_register(
        config.max_parallel_agents,
        task_id.clone(),
        agent_name_for_register,
        task_for_register,
        handle,
        completion_rx,
    )?;

    let _ = start_tx.send(());

    Ok(SpawnHandle {
        id: task_id,
        agent_name: agent_name_for_return,
    })
}

async fn execute_subagent(
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    agent_def: SubagentDefSnapshot,
    task: String,
    config: SubagentConfig,
) -> Result<AgentResult> {
    let registry = build_registry_for_agent(&tool_registry, &agent_def.allowed_tools);
    let registry = Arc::new(registry);

    let max_iterations = agent_def
        .max_iterations
        .map(|value| value as usize)
        .unwrap_or(config.max_iterations);

    let mut agent_config = AgentConfig::new(task.clone());
    agent_config.system_prompt = Some(agent_def.system_prompt.clone());
    agent_config.max_iterations = max_iterations;

    let executor = AgentExecutor::new(llm_client, registry);
    let result = executor.run(agent_config).await?;

    Ok(result)
}

fn build_registry_for_agent(parent: &Arc<ToolRegistry>, allowed_tools: &[String]) -> ToolRegistry {
    let filtered = FilteredToolset::from_allowlist(parent.clone(), allowed_tools);
    let mut registry = ToolRegistry::new();

    for schema in filtered.list_tools() {
        if let Some(tool) = parent.get(&schema.name) {
            registry.register_arc(tool);
        }
    }

    registry
}

/// Concrete implementation of [`SubagentManager`] that wraps
/// `SubagentTracker`, `SubagentDefLookup`, and `spawn_subagent`.
#[derive(Clone)]
pub struct SubagentManagerImpl {
    pub tracker: Arc<SubagentTracker>,
    pub definitions: Arc<dyn SubagentDefLookup>,
    pub llm_client: Arc<dyn LlmClient>,
    pub tool_registry: Arc<ToolRegistry>,
    pub config: SubagentConfig,
}

impl SubagentManagerImpl {
    pub fn new(
        tracker: Arc<SubagentTracker>,
        definitions: Arc<dyn SubagentDefLookup>,
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<ToolRegistry>,
        config: SubagentConfig,
    ) -> Self {
        Self {
            tracker,
            definitions,
            llm_client,
            tool_registry,
            config,
        }
    }

    /// Create from existing [`SubagentDeps`].
    pub fn from_deps(deps: &SubagentDeps) -> Self {
        Self {
            tracker: deps.tracker.clone(),
            definitions: deps.definitions.clone(),
            llm_client: deps.llm_client.clone(),
            tool_registry: deps.tool_registry.clone(),
            config: deps.config.clone(),
        }
    }
}

#[async_trait::async_trait]
impl SubagentManager for SubagentManagerImpl {
    fn spawn(&self, request: SpawnRequest) -> std::result::Result<SpawnHandle, ToolError> {
        spawn_subagent(
            self.tracker.clone(),
            self.definitions.clone(),
            self.llm_client.clone(),
            self.tool_registry.clone(),
            self.config.clone(),
            request,
        )
        .map_err(|e| ToolError::Tool(e.to_string()))
    }

    fn list_callable(&self) -> Vec<SubagentDefSummary> {
        self.definitions.list_callable()
    }

    fn list_running(&self) -> Vec<SubagentState> {
        self.tracker.running()
    }

    fn running_count(&self) -> usize {
        self.tracker.running_count()
    }

    async fn wait(&self, task_id: &str) -> Option<SubagentResult> {
        self.tracker.wait(task_id).await
    }

    fn config(&self) -> &SubagentConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_request_serialization() {
        let request = SpawnRequest {
            agent_id: "researcher".to_string(),
            task: "Research topic X".to_string(),
            timeout_secs: Some(300),
            priority: Some(SpawnPriority::High),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("researcher"));

        let parsed: SpawnRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, "researcher");
    }

    #[test]
    fn test_spawn_handle_serialization() {
        let handle = SpawnHandle {
            id: "task-123".to_string(),
            agent_name: "Researcher".to_string(),
        };

        let json = serde_json::to_string(&handle).unwrap();
        assert!(json.contains("task-123"));
    }

    #[tokio::test]
    async fn test_mark_completed_does_not_overwrite_cancelled() {
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
        assert_eq!(
            final_state.status,
            SubagentStatus::Cancelled,
            "mark_completed should not overwrite Cancelled status"
        );
    }

    #[tokio::test]
    async fn test_mark_completed_does_not_overwrite_timed_out() {
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
        assert_eq!(
            final_state.status,
            SubagentStatus::TimedOut,
            "mark_completed should not overwrite TimedOut status"
        );
    }

    #[tokio::test]
    async fn test_cancel_then_complete_race() {
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
        assert_eq!(
            final_state.status,
            SubagentStatus::Cancelled,
            "Race condition: cancelled task should stay cancelled even when mark_completed is called"
        );

        let _ = abort_tx.send(());
    }
}
