//! Sub-agent spawning support for tool-based execution.

use crate::agent::PromptFlags;
use crate::agent::executor::{AgentConfig, AgentExecutor, AgentResult};
use crate::error::{AiError, Result};
use crate::llm::{LlmClient, LlmClientFactory};
use crate::tools::{FilteredToolset, ToolRegistry, Toolset};
use dashmap::DashMap;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::{Duration, timeout};

// Re-export data types from restflow-traits
use restflow_traits::ToolError;
pub use restflow_traits::subagent::{
    SpawnHandle, SpawnPriority, SpawnRequest, SubagentCompletion, SubagentConfig,
    SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentManager, SubagentResult,
    SubagentSpawner, SubagentState, SubagentStatus,
};

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
    /// Optional factory for creating LLM clients when a per-spawn model is requested.
    pub llm_client_factory: Option<Arc<dyn LlmClientFactory>>,
}

/// Resolve which LLM client to use for a sub-agent spawn.
///
/// Priority: request.model > agent_def.default_model > parent llm_client.
fn resolve_llm_client(
    request_model: Option<&str>,
    def_default_model: Option<&str>,
    parent_client: &Arc<dyn LlmClient>,
    factory: Option<&Arc<dyn LlmClientFactory>>,
) -> Result<Arc<dyn LlmClient>> {
    let chosen_model = request_model.or(def_default_model);
    let Some(model) = chosen_model else {
        return Ok(parent_client.clone());
    };
    let Some(factory) = factory else {
        // No factory available — fall back to parent even if a model was requested.
        return Ok(parent_client.clone());
    };

    let resolved_model = resolve_model_name(model, factory.as_ref())?;
    let provider = factory
        .provider_for_model(&resolved_model)
        .ok_or_else(|| AiError::Agent(format!("Unknown model for sub-agent: {model}")))?;
    let api_key = factory.resolve_api_key(provider);
    factory.create_client(&resolved_model, api_key.as_deref())
}

fn resolve_model_name(model: &str, factory: &dyn LlmClientFactory) -> Result<String> {
    let query = model.trim();
    if query.is_empty() {
        return Err(AiError::Agent(
            "Unknown model for sub-agent: empty model".to_string(),
        ));
    }

    let available = factory.available_models();
    if available.is_empty() {
        return Err(AiError::Agent(format!(
            "Unknown model for sub-agent: {model}. No model catalog is available."
        )));
    }

    if let Some(exact) = available
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(query))
    {
        return Ok(exact.clone());
    }

    if factory.provider_for_model(query).is_some() {
        return Ok(query.to_string());
    }

    let normalized_query = normalize_model_identifier(query);
    if normalized_query.is_empty() {
        return Err(AiError::Agent(format!(
            "Unknown model for sub-agent: {model}"
        )));
    }

    let normalized_exact_matches: Vec<&String> = available
        .iter()
        .filter(|candidate| normalize_model_identifier(candidate) == normalized_query)
        .collect();
    if normalized_exact_matches.len() == 1 {
        return Ok(normalized_exact_matches[0].clone());
    }

    if let Some((provider, model_name)) = query.split_once(':') {
        let provider_joined = normalize_model_identifier(&format!("{provider}-{model_name}"));
        let canonical_matches: Vec<&String> = available
            .iter()
            .filter(|candidate| normalize_model_identifier(candidate) == provider_joined)
            .collect();
        if canonical_matches.len() == 1 {
            return Ok(canonical_matches[0].clone());
        }
    }

    if let Some(alias_resolved) = resolve_model_alias(&normalized_query, &available) {
        return Ok(alias_resolved);
    }

    let suggestions = available
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    Err(AiError::Agent(format!(
        "Unknown model for sub-agent: {model}. Try one of: {suggestions}"
    )))
}

fn resolve_model_alias(normalized_query: &str, available: &[String]) -> Option<String> {
    let mut normalized_available = available
        .iter()
        .map(|candidate| (normalize_model_identifier(candidate), candidate.clone()))
        .collect::<Vec<(String, String)>>();

    // Prefer stronger variants for coding-plan aliases.
    // MiniMax coding plan -> prefer M2.5 when no exact suffix was provided.
    if matches!(
        normalized_query,
        "minimax-coding-plan" | "minimax-coding" | "coding-plan-minimax"
    ) || normalized_query.starts_with("minimax-coding-plan")
    {
        let mut matches = normalized_available
            .iter()
            .filter(|(normalized, _)| normalized.starts_with("minimax-coding-plan-"))
            .map(|(_, original)| original.clone())
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return None;
        }
        matches.sort();
        return matches
            .iter()
            .find(|candidate| candidate.contains("m2-5"))
            .cloned()
            .or_else(|| matches.last().cloned());
    }

    // GLM coding-plan aliases.
    if matches!(
        normalized_query,
        "glm5-coding-plan"
            | "glm-5-coding-plan"
            | "zai-coding-plan"
            | "zai-coding-plan-glm5"
            | "zai-coding-plan-glm-5"
    ) && let Some(exact) = normalized_available
        .iter()
        .find(|(normalized, _)| normalized == "zai-coding-plan-glm-5")
        .map(|(_, original)| original.clone())
    {
        return Some(exact);
    }

    if matches!(
        normalized_query,
        "glm5-coding-plan-code" | "glm-5-coding-plan-code"
    ) && let Some(exact) = normalized_available
        .iter()
        .find(|(normalized, _)| normalized == "zai-coding-plan-glm-5-code")
        .map(|(_, original)| original.clone())
    {
        return Some(exact);
    }

    // Generic prefix fallback for normalized identifiers.
    let mut prefix_matches = normalized_available
        .drain(..)
        .filter(|(normalized, _)| normalized.starts_with(normalized_query))
        .map(|(_, original)| original)
        .collect::<Vec<_>>();
    if prefix_matches.is_empty() {
        return None;
    }
    prefix_matches.sort();
    Some(prefix_matches[0].clone())
}

fn normalize_model_identifier(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_dash = false;

    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_dash = false;
            continue;
        }
        if !previous_dash {
            normalized.push('-');
            previous_dash = true;
        }
    }

    normalized.trim_matches('-').to_string()
}

/// Spawn a sub-agent with the given request.
pub fn spawn_subagent(
    tracker: Arc<SubagentTracker>,
    definitions: Arc<dyn SubagentDefLookup>,
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    config: SubagentConfig,
    request: SpawnRequest,
    llm_client_factory: Option<Arc<dyn LlmClientFactory>>,
) -> Result<SpawnHandle> {
    let agent_def = definitions
        .lookup(&request.agent_id)
        .ok_or_else(|| AiError::Agent(format!("Unknown agent type: {}", request.agent_id)))?;

    // Resolve the LLM client: request.model > def.default_model > parent
    let llm_client = resolve_llm_client(
        request.model.as_deref(),
        agent_def.default_model.as_deref(),
        &llm_client,
        llm_client_factory.as_ref(),
    )?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let timeout_secs = request.timeout_secs.unwrap_or(config.subagent_timeout_secs);

    let agent_name_for_register = agent_def.name.clone();
    let agent_name_for_return = agent_def.name.clone();
    let task_for_register = request.task.clone();
    let parent_execution_id = request.parent_execution_id.clone();

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
        // Wait for registration confirmation. If the channel is closed
        // (e.g. try_register failed due to slot limit), abort immediately
        // to avoid orphaned execution.
        if start_rx.await.is_err() {
            return SubagentResult {
                success: false,
                output: String::new(),
                summary: None,
                duration_ms: 0,
                tokens_used: None,
                cost_usd: None,
                error: Some("Sub-agent registration cancelled".to_string()),
            };
        }
        let start = std::time::Instant::now();

        let result = timeout(
            Duration::from_secs(timeout_secs),
            execute_subagent(
                llm_client,
                tool_registry,
                agent_def,
                task.clone(),
                config_clone,
                parent_execution_id.clone(),
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
    parent_execution_id: Option<String>,
) -> Result<AgentResult> {
    // depth=1: direct child of the parent agent
    let registry = build_registry_for_agent(
        &tool_registry,
        &agent_def.allowed_tools,
        1,
        config.max_depth,
    );
    let registry = Arc::new(registry);

    let max_iterations = agent_def
        .max_iterations
        .map(|value| value as usize)
        .unwrap_or(config.max_iterations);

    let agent_config = build_subagent_agent_config(
        task.clone(),
        agent_def.system_prompt.clone(),
        max_iterations,
        parent_execution_id.as_deref(),
    );

    let executor = AgentExecutor::new(llm_client, registry);
    let result = executor.run(agent_config).await?;

    Ok(result)
}

fn build_subagent_agent_config(
    task: String,
    system_prompt: String,
    max_iterations: usize,
    parent_execution_id: Option<&str>,
) -> AgentConfig {
    let mut agent_config = AgentConfig::new(task);
    agent_config.system_prompt = Some(system_prompt);
    agent_config.max_iterations = max_iterations;
    // AGENTS/workspace instructions should only be injected for the main agent.
    agent_config.prompt_flags = PromptFlags::new().without_workspace_context();
    // Subagents run autonomously — there is no approval channel from the
    // parent, so security-gated tools must be auto-approved.
    agent_config.yolo_mode = true;
    agent_config = agent_config.with_context(
        "execution_context",
        json!({
            "role": "subagent",
            "parent_execution_id": parent_execution_id,
        }),
    );
    agent_config = agent_config.with_context("execution_role", json!("subagent"));
    agent_config
}

fn build_registry_for_agent(
    parent: &Arc<ToolRegistry>,
    allowed_tools: &[String],
    current_depth: usize,
    max_depth: usize,
) -> ToolRegistry {
    let filtered = FilteredToolset::from_allowlist(parent.clone(), allowed_tools);
    let mut registry = ToolRegistry::new();

    // Sub-agent management tools to exclude when at the depth limit,
    // so the LLM won't waste a tool call only to get a runtime error.
    const COLLAB_TOOLS: &[&str] = &[
        "spawn_agent",
        "wait_agents",
        "list_agents",
        "cancel_agent",
        "send_input",
    ];
    let at_depth_limit = max_depth > 0 && current_depth >= max_depth;

    for schema in filtered.list_tools() {
        if at_depth_limit && COLLAB_TOOLS.contains(&schema.name.as_str()) {
            continue;
        }
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
    /// Optional factory for creating LLM clients when a per-spawn model is requested.
    pub llm_client_factory: Option<Arc<dyn LlmClientFactory>>,
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
            llm_client_factory: None,
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
            llm_client_factory: deps.llm_client_factory.clone(),
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
            self.llm_client_factory.clone(),
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
    use crate::llm::{LlmProvider, MockLlmClient, MockStep};
    use std::collections::HashMap;

    #[test]
    fn build_subagent_agent_config_sets_execution_context() {
        let config = build_subagent_agent_config(
            "Sub-task".to_string(),
            "System prompt".to_string(),
            3,
            None,
        );

        assert_eq!(
            config.context.get("execution_role"),
            Some(&serde_json::Value::String("subagent".to_string()))
        );
        assert_eq!(config.context["execution_context"]["role"], "subagent");
    }

    #[test]
    fn build_subagent_agent_config_sets_parent_execution_id_when_provided() {
        let config = build_subagent_agent_config(
            "Sub-task".to_string(),
            "System prompt".to_string(),
            3,
            Some("exec-parent-1"),
        );

        assert_eq!(
            config.context["execution_context"]["parent_execution_id"],
            "exec-parent-1"
        );
    }

    /// Minimal mock for sub-agent definitions used in integration tests.
    struct MockDefLookup {
        defs: HashMap<String, SubagentDefSnapshot>,
    }

    impl MockDefLookup {
        fn with_agent(id: &str) -> Self {
            let mut defs = HashMap::new();
            defs.insert(
                id.to_string(),
                SubagentDefSnapshot {
                    name: id.to_string(),
                    system_prompt: "You are a test agent.".to_string(),
                    allowed_tools: vec![],
                    max_iterations: Some(1),
                    default_model: None,
                },
            );
            Self { defs }
        }
    }

    impl SubagentDefLookup for MockDefLookup {
        fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
            self.defs.get(id).cloned()
        }
        fn list_callable(&self) -> Vec<SubagentDefSummary> {
            vec![]
        }
    }

    struct AliasOnlyFactory {
        models: Vec<String>,
    }

    impl AliasOnlyFactory {
        fn new(models: Vec<&str>) -> Self {
            Self {
                models: models.into_iter().map(str::to_string).collect(),
            }
        }
    }

    impl LlmClientFactory for AliasOnlyFactory {
        fn create_client(
            &self,
            _model: &str,
            _api_key: Option<&str>,
        ) -> Result<Arc<dyn LlmClient>> {
            Err(AiError::Llm(
                "create_client is not used in alias tests".to_string(),
            ))
        }

        fn available_models(&self) -> Vec<String> {
            self.models.clone()
        }

        fn resolve_api_key(&self, _provider: LlmProvider) -> Option<String> {
            None
        }

        fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
            self.models
                .iter()
                .find(|candidate| candidate.eq_ignore_ascii_case(model.trim()))
                .map(|_| LlmProvider::OpenAI)
        }

        fn is_codex_cli_model(&self, _model: &str) -> bool {
            false
        }

        fn is_opencode_cli_model(&self, _model: &str) -> bool {
            false
        }

        fn is_gemini_cli_model(&self, _model: &str) -> bool {
            false
        }
    }

    #[test]
    fn test_spawn_request_serialization() {
        let request = SpawnRequest {
            agent_id: "researcher".to_string(),
            task: "Research topic X".to_string(),
            timeout_secs: Some(300),
            priority: Some(SpawnPriority::High),
            model: None,
            parent_execution_id: None,
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

    #[tokio::test]
    async fn test_spawn_over_max_parallel_does_not_execute() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        // Two steps so two agents can be spawned
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps(
            "mock",
            vec![
                MockStep::text("result-1").with_delay(2000),
                MockStep::text("result-2"),
            ],
        ));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 1,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        // First spawn should succeed
        let result1 = spawn_subagent(
            tracker.clone(),
            definitions.clone(),
            llm_client.clone(),
            tool_registry.clone(),
            config.clone(),
            SpawnRequest {
                agent_id: "tester".to_string(),
                task: "first task".to_string(),
                timeout_secs: Some(10),
                priority: None,
                model: None,
                parent_execution_id: None,
            },
            None,
        );
        assert!(result1.is_ok(), "First spawn should succeed");

        // Second spawn should fail because max_parallel_agents is 1
        let result2 = spawn_subagent(
            tracker.clone(),
            definitions.clone(),
            llm_client.clone(),
            tool_registry.clone(),
            config.clone(),
            SpawnRequest {
                agent_id: "tester".to_string(),
                task: "second task (should not execute)".to_string(),
                timeout_secs: Some(10),
                priority: None,
                model: None,
                parent_execution_id: None,
            },
            None,
        );
        assert!(
            result2.is_err(),
            "Second spawn should fail at max parallel limit"
        );

        // The orphaned tokio task (from the failed spawn) should not run.
        // Give it a moment to potentially execute if the bug existed.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Only 1 agent should be tracked
        assert_eq!(tracker.all().len(), 1);
    }

    #[test]
    fn test_build_registry_excludes_collab_tools_at_depth_limit() {
        let mut parent = ToolRegistry::new();

        // Minimal Tool impl for registry testing
        struct DummyTool(&'static str);
        #[async_trait::async_trait]
        impl restflow_traits::Tool for DummyTool {
            fn name(&self) -> &str {
                self.0
            }
            fn description(&self) -> &str {
                ""
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn execute(
                &self,
                _input: serde_json::Value,
            ) -> std::result::Result<restflow_traits::ToolOutput, restflow_traits::ToolError>
            {
                unimplemented!()
            }
        }

        parent.register(DummyTool("http"));
        parent.register(DummyTool("bash"));
        parent.register(DummyTool("spawn_agent"));
        parent.register(DummyTool("wait_agents"));
        parent.register(DummyTool("list_agents"));
        parent.register(DummyTool("cancel_agent"));
        parent.register(DummyTool("send_input"));

        let parent = Arc::new(parent);
        let all_tools: Vec<String> = vec![
            "http",
            "bash",
            "spawn_agent",
            "wait_agents",
            "list_agents",
            "cancel_agent",
            "send_input",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        // At depth limit: collab tools should be excluded
        let registry = build_registry_for_agent(&parent, &all_tools, 1, 1);
        let names: Vec<String> = registry.list_tools().into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"http".to_string()));
        assert!(names.contains(&"bash".to_string()));
        assert!(!names.contains(&"spawn_agent".to_string()));
        assert!(!names.contains(&"wait_agents".to_string()));
        assert!(!names.contains(&"list_agents".to_string()));
        assert!(!names.contains(&"cancel_agent".to_string()));
        assert!(!names.contains(&"send_input".to_string()));

        // Not at depth limit: all tools should be included
        let registry = build_registry_for_agent(&parent, &all_tools, 0, 2);
        let names: Vec<String> = registry.list_tools().into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"spawn_agent".to_string()));
        assert!(names.contains(&"wait_agents".to_string()));
    }

    #[test]
    fn test_subagent_config_disables_workspace_instruction_injection() {
        let config = build_subagent_agent_config(
            "task".to_string(),
            "You are subagent".to_string(),
            7,
            None,
        );
        assert_eq!(config.max_iterations, 7);
        assert_eq!(config.system_prompt.as_deref(), Some("You are subagent"));
        assert!(!config.prompt_flags.include_workspace_context);
        assert!(config.yolo_mode);
    }

    #[test]
    fn test_resolve_model_name_accepts_case_insensitive_match() {
        let factory = AliasOnlyFactory::new(vec!["gpt-5", "minimax-coding-plan-m2-5"]);
        let resolved = resolve_model_name("GPT-5", &factory).unwrap();
        assert_eq!(resolved, "gpt-5");
    }

    #[test]
    fn test_resolve_model_name_maps_minimax_coding_plan_alias() {
        let factory =
            AliasOnlyFactory::new(vec!["minimax-coding-plan-m2-1", "minimax-coding-plan-m2-5"]);
        let resolved = resolve_model_name("minimax/coding-plan", &factory).unwrap();
        assert_eq!(resolved, "minimax-coding-plan-m2-5");
    }

    #[test]
    fn test_resolve_model_name_maps_glm5_coding_plan_alias() {
        let factory =
            AliasOnlyFactory::new(vec!["zai-coding-plan-glm-5", "zai-coding-plan-glm-5-code"]);
        let resolved = resolve_model_name("glm5 coding plan", &factory).unwrap();
        assert_eq!(resolved, "zai-coding-plan-glm-5");
    }

    #[test]
    fn test_resolve_model_name_maps_glm5_coding_plan_code_alias() {
        let factory =
            AliasOnlyFactory::new(vec!["zai-coding-plan-glm-5", "zai-coding-plan-glm-5-code"]);
        let resolved = resolve_model_name("glm-5 coding-plan code", &factory).unwrap();
        assert_eq!(resolved, "zai-coding-plan-glm-5-code");
    }

    #[test]
    fn test_resolve_model_name_returns_helpful_error_for_unknown_model() {
        let factory = AliasOnlyFactory::new(vec!["gpt-5", "minimax-coding-plan-m2-5"]);
        let error = resolve_model_name("unknown-model", &factory).unwrap_err();
        assert!(error.to_string().contains("Try one of"));
    }
}
