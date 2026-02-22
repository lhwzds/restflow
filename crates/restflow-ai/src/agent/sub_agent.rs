//! Sub-agent spawning support for tool-based execution.

use crate::agent::executor::{AgentConfig, AgentExecutor, AgentResult};
use crate::error::{AiError, Result};
use crate::llm::LlmClient;
use crate::tools::{FilteredToolset, ToolRegistry, Toolset};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::{Duration, timeout};

/// Snapshot of a sub-agent definition with all fields needed for execution.
///
/// This is a simple owned data struct that captures the fields from a concrete
/// agent definition. It decouples the restflow-ai crate from the full
/// `AgentDefinition` struct (which lives in restflow-core and carries
/// `#[derive(TS)]` and other derives that restflow-ai doesn't need).
#[derive(Debug, Clone)]
pub struct SubagentDefSnapshot {
    /// Display name
    pub name: String,
    /// System prompt for the agent
    pub system_prompt: String,
    /// Allowed tool names
    pub allowed_tools: Vec<String>,
    /// Maximum ReAct loop iterations
    pub max_iterations: Option<u32>,
}

/// Summary info for listing a sub-agent definition.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubagentDefSummary {
    /// Unique identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Description of when to use this agent
    pub description: String,
    /// Tags for categorization
    pub tags: Vec<String>,
}

/// Trait for looking up sub-agent definitions by ID.
///
/// Implemented by `AgentDefinitionRegistry` in restflow-core so that
/// restflow-ai can spawn sub-agents without depending on restflow-core.
pub trait SubagentDefLookup: Send + Sync {
    /// Look up a sub-agent definition by ID, returning a snapshot of the
    /// fields needed for execution.
    fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot>;

    /// List all callable sub-agent definitions (for display/listing purposes).
    fn list_callable(&self) -> Vec<SubagentDefSummary>;
}

/// Configuration for sub-agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    /// Maximum number of parallel sub-agents.
    pub max_parallel_agents: usize,
    /// Default timeout for sub-agents in seconds.
    pub subagent_timeout_secs: u64,
    /// Maximum iterations for sub-agents.
    pub max_iterations: usize,
    /// Maximum nesting depth for sub-agents.
    pub max_depth: usize,
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            max_parallel_agents: 5,
            subagent_timeout_secs: 600,
            max_iterations: 20,
            max_depth: 1,
        }
    }
}

/// Request to spawn a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    /// Agent type ID (e.g., "researcher", "coder").
    pub agent_id: String,

    /// Task description for the agent.
    pub task: String,

    /// Optional timeout in seconds.
    pub timeout_secs: Option<u64>,

    /// Optional priority level.
    pub priority: Option<SpawnPriority>,
}

/// Priority level for sub-agent spawning.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum SpawnPriority {
    Low,
    #[default]
    Normal,
    High,
}

/// Handle returned after spawning a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnHandle {
    /// Unique task ID.
    pub id: String,

    /// Agent name.
    pub agent_name: String,
}

/// Sub-agent running state
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubagentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

/// Result from a sub-agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Cost in USD
    pub cost_usd: Option<f64>,

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
    /// cancelled or timed out. This prevents a race condition where:
    /// 1. cancel() sets status to Cancelled and aborts the task
    /// 2. The spawned task completes and calls mark_completed()
    /// 3. Without this check, mark_completed() would overwrite Cancelled with Failed
    pub fn mark_completed(&self, id: &str, result: SubagentResult) {
        // Check if already cancelled or timed out - don't overwrite the status
        if let Some(state) = self.states.get(id)
            && matches!(
                state.status,
                SubagentStatus::Cancelled | SubagentStatus::TimedOut
            )
        {
            // Already marked as cancelled/timed out, just clean up handles
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

/// Sub-agent manager to spawn ReAct executors.
pub struct SubAgentManager {
    tracker: Arc<SubagentTracker>,
    definitions: Arc<dyn SubagentDefLookup>,
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    config: SubagentConfig,
    current_depth: usize,
}

impl SubAgentManager {
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
            current_depth: 0,
        }
    }

    pub fn with_depth(mut self, depth: usize) -> Self {
        self.current_depth = depth;
        self
    }

    pub fn spawn(&self, request: SpawnRequest) -> Result<SpawnHandle> {
        if self.current_depth >= self.config.max_depth {
            return Err(AiError::Agent("Max sub-agent depth reached".to_string()));
        }

        spawn_subagent(
            self.tracker.clone(),
            self.definitions.clone(),
            self.llm_client.clone(),
            self.tool_registry.clone(),
            self.config.clone(),
            request,
        )
    }

    pub async fn wait(&self, task_id: &str) -> Option<SubagentResult> {
        self.tracker.wait(task_id).await
    }

    pub fn tracker(&self) -> Arc<SubagentTracker> {
        self.tracker.clone()
    }
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
        // Create a tracker with channels
        let (tx, _rx) = mpsc::channel(1);
        let (_completion_tx, completion_rx) = mpsc::channel(1);
        let tracker = Arc::new(SubagentTracker::new(tx, completion_rx));

        // Insert a state with Cancelled status
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

        // Try to mark as completed with success=true (would normally set to Completed)
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

        // The status should still be Cancelled, not Completed
        let final_state = tracker.states.get("test-id").unwrap();
        assert_eq!(
            final_state.status,
            SubagentStatus::Cancelled,
            "mark_completed should not overwrite Cancelled status"
        );
    }

    #[tokio::test]
    async fn test_mark_completed_does_not_overwrite_timed_out() {
        // Create a tracker with channels
        let (tx, _rx) = mpsc::channel(1);
        let (_completion_tx, completion_rx) = mpsc::channel(1);
        let tracker = Arc::new(SubagentTracker::new(tx, completion_rx));

        // Insert a state with TimedOut status
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

        // Try to mark as completed with success=true
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

        // The status should still be TimedOut, not Completed
        let final_state = tracker.states.get("test-id-2").unwrap();
        assert_eq!(
            final_state.status,
            SubagentStatus::TimedOut,
            "mark_completed should not overwrite TimedOut status"
        );
    }

    #[tokio::test]
    async fn test_cancel_then_complete_race() {
        // This test simulates the race condition:
        // 1. cancel() is called, sets status to Cancelled
        // 2. The spawned task completes and calls mark_completed()
        // 3. mark_completed() should NOT overwrite Cancelled with Failed

        let (tx, _rx) = mpsc::channel(1);
        let (_completion_tx, completion_rx) = mpsc::channel(1);
        let tracker = Arc::new(SubagentTracker::new(tx, completion_rx));

        // We need a real AbortHandle, so we create a dummy task
        let (abort_tx, abort_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async {
            let _ = abort_rx.await;
        });
        let abort_handle = handle.abort_handle();

        // Simulate: task is running
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

        // Step 1: cancel() is called
        tracker.cancel("race-test");

        // Verify status is Cancelled
        let state_after_cancel = tracker.states.get("race-test").unwrap();
        assert_eq!(state_after_cancel.status, SubagentStatus::Cancelled);

        // Step 2: Task completes with success=false (as if it was aborted)
        let result = SubagentResult {
            success: false,
            output: String::new(),
            summary: None,
            duration_ms: 50,
            tokens_used: None,
            cost_usd: None,
            error: Some("Task aborted".to_string()),
        };

        // Step 3: mark_completed() is called (as in the spawned task)
        tracker.mark_completed("race-test", result);

        // The status should still be Cancelled, NOT Failed
        let final_state = tracker.states.get("race-test").unwrap();
        assert_eq!(
            final_state.status,
            SubagentStatus::Cancelled,
            "Race condition: cancelled task should stay cancelled even when mark_completed is called"
        );

        // Clean up the spawned task
        let _ = abort_tx.send(());
    }
}
