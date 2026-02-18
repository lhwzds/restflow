//! Agent executor with ReAct loop

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::agent::ExecutionStep;
use crate::agent::context::{AgentContext, ContextDiscoveryConfig, WorkspaceContextCache};
use crate::agent::deferred::{DeferredExecutionManager, DeferredStatus};
use crate::agent::history::{HistoryPipeline, HistoryProcessor};
use crate::agent::model_router::{ModelRoutingConfig, ModelSwitcher, classify_task, select_model};
use crate::agent::resource::{ResourceLimits, ResourceTracker, ResourceUsage};
use crate::agent::scratchpad::Scratchpad;
use crate::agent::state::{AgentState, AgentStatus};
use crate::agent::stream::{ChannelEmitter, NullEmitter, StreamEmitter, ToolCallAccumulator};
use crate::agent::streaming_buffer::{BufferMode, StreamingBuffer};
use crate::agent::stuck::{StuckAction, StuckDetector, StuckDetectorConfig};
use crate::error::{AiError, Result};
use crate::llm::{CompletionRequest, FinishReason, LlmClient, Message, Role, ToolCall};
use crate::memory::{CompactionConfig, CompactionResult, DEFAULT_MAX_MESSAGES, WorkingMemory};
use crate::steer::SteerMessage;
use crate::tools::{ToolErrorCategory, ToolRegistry};
use futures::{Stream, StreamExt};
use tokio::sync::{Mutex, mpsc};
use tokio::time::sleep;
use tracing::debug;

const MAX_TOOL_RETRIES: usize = 2;

/// Agent type for system prompt composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentType {
    #[default]
    Coder,
    Task,
    Summarizer,
    Title,
}

/// Persistence frequency for execution checkpoints.
#[derive(Debug, Clone)]
pub enum CheckpointDurability {
    /// Persist state after each ReAct turn.
    PerTurn,
    /// Persist state every N turns.
    Periodic { interval: usize },
    /// Persist state only on terminal completion/failure.
    OnComplete,
}

impl Default for CheckpointDurability {
    fn default() -> Self {
        Self::Periodic { interval: 5 }
    }
}

type CheckpointFuture = Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>;
type CheckpointCallback = Arc<dyn Fn(&AgentState) -> CheckpointFuture + Send + Sync>;

/// Configuration for agent execution
#[derive(Clone)]
pub struct AgentConfig {
    pub goal: String,
    pub system_prompt: Option<String>,
    pub max_iterations: usize,
    pub temperature: Option<f32>,
    /// Hidden context passed to tools but not shown to LLM (Swarm-inspired)
    pub context: HashMap<String, Value>,
    /// Timeout for each tool execution (default: 30s)
    pub tool_timeout: Duration,
    /// Max length for tool results to prevent context overflow (default: 4000)
    pub max_tool_result_length: usize,
    /// Maximum messages to retain in working memory (default: 100)
    /// When this limit is reached, oldest non-system messages are evicted.
    pub max_memory_messages: usize,
    /// Context window size for compaction decisions (default: 128000 tokens).
    pub context_window: usize,
    /// Optional compaction configuration for working memory.
    pub compaction_config: Option<CompactionConfig>,
    /// Optional history processors applied before each LLM request.
    pub history_pipeline: HistoryPipeline,
    /// Optional agent context injected into the system prompt.
    pub agent_context: Option<AgentContext>,
    /// Agent type for context injection rules.
    pub agent_type: AgentType,
    /// Resource limits for guardrails (tool calls, wall-clock, depth).
    pub resource_limits: ResourceLimits,
    /// Optional stuck detection configuration.
    /// When enabled, detects when the agent repeatedly calls the same tool
    /// with the same arguments and either nudges or stops execution.
    pub stuck_detection: Option<StuckDetectorConfig>,
    /// Optional append-only JSONL scratchpad for execution diagnostics.
    pub scratchpad: Option<Arc<Scratchpad>>,
    /// Optional model routing configuration for dynamic tier-based switching.
    pub model_routing: Option<ModelRoutingConfig>,
    /// Optional model switcher used when model routing is enabled.
    pub model_switcher: Option<Arc<dyn ModelSwitcher>>,
    /// Auto-approve security-gated tool calls (scheduled automation mode).
    pub yolo_mode: bool,
    /// Checkpoint persistence policy.
    pub checkpoint_durability: CheckpointDurability,
    /// Optional callback to persist agent state checkpoints.
    pub checkpoint_callback: Option<CheckpointCallback>,
    /// Hierarchical subflow path for nested sub-agent tracking.
    pub subflow_path: Vec<String>,
}

impl AgentConfig {
    /// Create a new agent config with a goal
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            system_prompt: None,
            max_iterations: 100,
            temperature: None, // None = use model default
            context: HashMap::new(),
            tool_timeout: Duration::from_secs(300),
            max_tool_result_length: 4000,
            max_memory_messages: DEFAULT_MAX_MESSAGES,
            context_window: 128_000,
            compaction_config: None,
            history_pipeline: HistoryPipeline::default(),
            agent_context: None,
            agent_type: AgentType::default(),
            resource_limits: ResourceLimits::default(),
            stuck_detection: Some(StuckDetectorConfig::default()),
            scratchpad: None,
            model_routing: None,
            model_switcher: None,
            yolo_mode: false,
            checkpoint_durability: CheckpointDurability::Periodic { interval: 5 },
            checkpoint_callback: None,
            subflow_path: Vec::new(),
        }
    }

    /// Set maximum messages in working memory
    pub fn with_max_memory_messages(mut self, max: usize) -> Self {
        self.max_memory_messages = max;
        self
    }

    /// Set context window size for compaction decisions.
    pub fn with_context_window(mut self, context_window: usize) -> Self {
        self.context_window = context_window;
        self
    }

    /// Enable working memory compaction.
    pub fn with_compaction_config(mut self, config: CompactionConfig) -> Self {
        self.compaction_config = Some(config);
        self
    }

    /// Register a history processor in the request pipeline.
    pub fn with_history_processor(mut self, processor: Arc<dyn HistoryProcessor>) -> Self {
        self.history_pipeline.add(processor);
        self
    }

    /// Override the full history pipeline.
    pub fn with_history_pipeline(mut self, pipeline: HistoryPipeline) -> Self {
        self.history_pipeline = pipeline;
        self
    }

    /// Set custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set max iterations
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Add context variable
    pub fn with_context(mut self, key: impl Into<String>, value: Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    /// Set subflow path for hierarchical tracking
    pub fn with_subflow_path(mut self, path: Vec<String>) -> Self {
        self.subflow_path = path;
        self
    }

    /// Set tool timeout
    pub fn with_tool_timeout(mut self, timeout: Duration) -> Self {
        self.tool_timeout = timeout;
        self
    }

    /// Set max tool result length
    pub fn with_max_tool_result_length(mut self, max: usize) -> Self {
        self.max_tool_result_length = max;
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set agent context for prompt injection
    pub fn with_agent_context(mut self, context: AgentContext) -> Self {
        self.agent_context = Some(context);
        self
    }

    /// Set agent type for context injection rules.
    pub fn with_agent_type(mut self, agent_type: AgentType) -> Self {
        self.agent_type = agent_type;
        self
    }

    /// Set resource limits for guardrails.
    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }

    /// Set stuck detection configuration.
    pub fn with_stuck_detection(mut self, config: StuckDetectorConfig) -> Self {
        self.stuck_detection = Some(config);
        self
    }

    /// Disable stuck detection.
    pub fn without_stuck_detection(mut self) -> Self {
        self.stuck_detection = None;
        self
    }

    /// Set scratchpad for append-only JSONL execution tracing.
    pub fn with_scratchpad(mut self, scratchpad: Arc<Scratchpad>) -> Self {
        self.scratchpad = Some(scratchpad);
        self
    }

    /// Set model routing configuration.
    pub fn with_model_routing(mut self, routing: ModelRoutingConfig) -> Self {
        self.model_routing = Some(routing);
        self
    }

    /// Set model switcher used by routing.
    pub fn with_model_switcher(mut self, switcher: Arc<dyn ModelSwitcher>) -> Self {
        self.model_switcher = Some(switcher);
        self
    }

    /// Enable or disable yolo mode (auto-approval execution mode).
    pub fn with_yolo_mode(mut self, yolo_mode: bool) -> Self {
        self.yolo_mode = yolo_mode;
        self
    }

    /// Set checkpoint durability policy.
    pub fn with_checkpoint_durability(mut self, durability: CheckpointDurability) -> Self {
        self.checkpoint_durability = durability;
        self
    }

    /// Set asynchronous checkpoint callback.
    pub fn with_checkpoint_callback<F, Fut>(mut self, callback: F) -> Self
    where
        F: Fn(&AgentState) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.checkpoint_callback = Some(Arc::new(move |state| Box::pin(callback(state))));
        self
    }
}

/// Result of agent execution
#[derive(Debug)]
pub struct AgentResult {
    pub success: bool,
    pub answer: Option<String>,
    pub error: Option<String>,
    pub iterations: usize,
    pub total_tokens: u32,
    pub total_cost_usd: f64,
    pub state: AgentState,
    /// Compaction operations performed during the run.
    pub compaction_results: Vec<CompactionResult>,
    /// Resource usage snapshot at end of run.
    pub resource_usage: ResourceUsage,
}

/// Agent executor implementing Swarm-style ReAct loop
pub struct AgentExecutor {
    llm: Arc<dyn LlmClient>,
    tools: Arc<ToolRegistry>,
    summarizer: Option<Arc<dyn LlmClient>>,
    context_cache: Option<WorkspaceContextCache>,
    steer_rx: Option<Mutex<mpsc::Receiver<SteerMessage>>>,
}

impl AgentExecutor {
    /// Create a new agent executor
    pub fn new(llm: Arc<dyn LlmClient>, tools: Arc<ToolRegistry>) -> Self {
        let context_cache = std::env::current_dir()
            .ok()
            .map(|workdir| WorkspaceContextCache::new(ContextDiscoveryConfig::default(), workdir));

        Self {
            llm,
            tools,
            summarizer: None,
            context_cache,
            steer_rx: None,
        }
    }

    /// Configure a dedicated summarizer LLM.
    pub fn with_summarizer(mut self, summarizer: Arc<dyn LlmClient>) -> Self {
        self.summarizer = Some(summarizer);
        self
    }

    /// Attach a steer channel for live instruction updates.
    pub fn with_steer_channel(mut self, rx: mpsc::Receiver<SteerMessage>) -> Self {
        self.steer_rx = Some(Mutex::new(rx));
        self
    }

    async fn drain_steer_messages(&self) -> Vec<SteerMessage> {
        let Some(rx) = &self.steer_rx else {
            return Vec::new();
        };

        let mut rx = rx.lock().await;
        let mut messages = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(msg) => messages.push(msg),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        messages
    }

    async fn apply_steer_messages(
        &self,
        state: &mut AgentState,
        memory: &mut WorkingMemory,
        deferred_manager: &DeferredExecutionManager,
    ) {
        let messages = self.drain_steer_messages().await;
        if messages.is_empty() {
            return;
        }

        for steer in messages {
            match &steer.command {
                crate::steer::SteerCommand::Message { instruction } => {
                    if let Some((approval_id, approved, reason)) =
                        parse_approval_resolution(instruction)
                    {
                        let _ = deferred_manager
                            .resolve_by_approval_id(&approval_id, approved, reason.clone())
                            .await;
                        tracing::info!(
                            approval_id = %approval_id,
                            approved = approved,
                            "Received approval resolution steer message"
                        );
                        let text = if approved {
                            format!("[Approval Update]: {approval_id} approved.")
                        } else {
                            format!(
                                "[Approval Update]: {approval_id} denied. {}",
                                reason
                                    .clone()
                                    .unwrap_or_else(|| "No reason provided.".to_string())
                            )
                        };
                        let msg = Message::system(text);
                        state.add_message(msg.clone());
                        memory.add(msg);
                        continue;
                    }
                    tracing::info!(
                        instruction = %instruction,
                        source = ?steer.source,
                        "Received steer message, injecting into conversation"
                    );
                    let msg = Message::user(format!("[User Update]: {}", instruction));
                    state.add_message(msg.clone());
                    memory.add(msg);
                }
                crate::steer::SteerCommand::Interrupt { reason, .. } => {
                    tracing::info!(
                        reason = %reason,
                        source = ?steer.source,
                        "Received interrupt command"
                    );
                    state.interrupt(reason);
                }
            }
        }
    }

    async fn process_resolved_deferred_calls(
        &self,
        deferred_manager: &DeferredExecutionManager,
        state: &mut AgentState,
        memory: &mut WorkingMemory,
        tool_timeout: Duration,
        max_tool_result_length: usize,
    ) {
        let resolved_calls = deferred_manager.drain_resolved().await;
        if resolved_calls.is_empty() {
            return;
        }

        for deferred in resolved_calls {
            match deferred.status {
                DeferredStatus::Approved => {
                    let result = tokio::time::timeout(
                        tool_timeout,
                        self.execute_tool_call(&[], &deferred.tool_name, deferred.args.clone(), false),
                    )
                    .await
                    .map_err(|_| AiError::Tool(format!("Tool {} timed out", deferred.tool_name)))
                    .and_then(|result| result);
                    let mut text = match result {
                        Ok(output) if output.success => {
                            let value = serde_json::to_string(&output.result).unwrap_or_default();
                            format!(
                                "Deferred tool call '{}' was approved and executed successfully. Result: {}",
                                deferred.tool_name, value
                            )
                        }
                        Ok(output) => format!(
                            "Deferred tool call '{}' was approved but failed: {}",
                            deferred.tool_name,
                            output.error.unwrap_or_else(|| "unknown error".to_string())
                        ),
                        Err(error) => format!(
                            "Deferred tool call '{}' failed after approval: {}",
                            deferred.tool_name, error
                        ),
                    };
                    if text.len() > max_tool_result_length {
                        let safe_len = text
                            .char_indices()
                            .map(|(i, _)| i)
                            .take_while(|&i| i <= max_tool_result_length)
                            .last()
                            .unwrap_or(0);
                        text = format!(
                            "{}...[truncated, {} chars total]",
                            &text[..safe_len],
                            text.len()
                        );
                    }
                    let msg = Message::system(text);
                    state.add_message(msg.clone());
                    memory.add(msg);
                }
                DeferredStatus::Denied { reason } => {
                    let msg = Message::system(format!(
                        "Deferred tool call '{}' was denied: {}",
                        deferred.tool_name, reason
                    ));
                    state.add_message(msg.clone());
                    memory.add(msg);
                }
                DeferredStatus::TimedOut => {
                    let msg = Message::system(format!(
                        "Approval timed out for deferred tool call '{}'.",
                        deferred.tool_name
                    ));
                    state.add_message(msg.clone());
                    memory.add(msg);
                }
                DeferredStatus::Pending => {}
            }
        }
    }

    /// Execute agent - simplified Swarm-style loop
    pub async fn run(&self, config: AgentConfig) -> Result<AgentResult> {
        let mut emitter = NullEmitter;
        self.execute_with_mode(config, &mut emitter, false, None, None)
            .await
    }

    #[deprecated(note = "Use run() or stream-based execution APIs")]
    pub async fn execute_streaming(
        &self,
        config: AgentConfig,
        emitter: &mut dyn StreamEmitter,
    ) -> Result<AgentResult> {
        self.execute_with_mode(config, emitter, true, None, None)
            .await
    }

    /// Resume execution from an existing state snapshot.
    pub async fn execute_from_state(
        &self,
        config: AgentConfig,
        mut state: AgentState,
        emitter: &mut dyn StreamEmitter,
    ) -> Result<AgentResult> {
        state.status = AgentStatus::Running;
        state.ended_at = None;
        let execution_id = state.execution_id.clone();
        self.execute_with_mode(config, emitter, true, Some(execution_id), Some(state))
            .await
    }

    async fn execute_with_mode(
        &self,
        config: AgentConfig,
        emitter: &mut dyn StreamEmitter,
        stream_llm: bool,
        execution_id_override: Option<String>,
        initial_state: Option<AgentState>,
    ) -> Result<AgentResult> {
        let execution_id =
            execution_id_override.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let mut streaming_buffer = StreamingBuffer::default();
        let mut state =
            initial_state.unwrap_or_else(|| AgentState::new(execution_id, config.max_iterations));
        if let Some(scratchpad) = &config.scratchpad {
            scratchpad.log_start(&state.execution_id, self.llm.model(), &config.goal);
        }
        state.max_iterations = config.max_iterations;
        state.context.extend(config.context.clone());
        let mut total_tokens: u32 = 0;
        let mut total_cost_usd: f64 = 0.0;
        let mut compaction_results = Vec::new();
        let tracker = ResourceTracker::new(config.resource_limits.clone());

        // Initialize working memory for context window management
        let mut memory = WorkingMemory::new(config.max_memory_messages);
        if let Some(compaction_config) = config.compaction_config.clone() {
            memory.enable_compaction(compaction_config);
        }
        for msg in &state.messages {
            memory.add(msg.clone());
        }

        // Initialize stuck detector
        let mut stuck_detector = config.stuck_detection.clone().map(StuckDetector::new);
        let mut had_failure = false;
        let mut last_tool_names: Vec<String> = Vec::new();
        let deferred_manager = DeferredExecutionManager::new(Duration::from_secs(300));

        // Initialize conversation only for fresh executions.
        if state.messages.is_empty() {
            let system_prompt = self.build_system_prompt(&config).await;
            let system_msg = Message::system(&system_prompt);
            let user_msg = Message::user(&config.goal);

            state.add_message(system_msg.clone());
            state.add_message(user_msg.clone());
            memory.add(system_msg);
            memory.add(user_msg);
        }

        // Core loop (Swarm-inspired simplicity)
        while state.iteration < state.max_iterations && !state.is_terminal() {
            if let Some(scratchpad) = &config.scratchpad {
                scratchpad.log_iteration_begin(state.iteration + 1);
            }
            let summarizer = self.summarizer.as_deref().unwrap_or(self.llm.as_ref());
            if let Some(result) = memory
                .auto_compact_if_needed(summarizer, config.context_window)
                .await?
            {
                // Compaction affects working memory only; full state history remains intact.
                compaction_results.push(result);
            }

            self.apply_steer_messages(&mut state, &mut memory, &deferred_manager)
                .await;
            self.process_resolved_deferred_calls(
                &deferred_manager,
                &mut state,
                &mut memory,
                config.tool_timeout,
                config.max_tool_result_length,
            )
            .await;

            // Check wall-clock before LLM call
            if let Err(e) = tracker.check_wall_clock() {
                state.resource_exhaust(e.to_string());
                break;
            }

            // 1. LLM call - use working memory for context (handles overflow)
            if let Some(routing) = config
                .model_routing
                .as_ref()
                .filter(|routing| routing.enabled)
                && let Some(switcher) = config.model_switcher.as_ref()
            {
                let tool_names: Vec<&str> = last_tool_names.iter().map(String::as_str).collect();
                let messages = memory.get_messages();
                let latest_signal = messages
                    .iter()
                    .rev()
                    .find(|message| matches!(message.role, Role::User | Role::Assistant))
                    .map(|message| message.content.as_str())
                    .unwrap_or(config.goal.as_str());
                let should_escalate = routing.escalate_on_failure && had_failure;
                let tier =
                    classify_task(&tool_names, latest_signal, state.iteration, should_escalate);
                let current_model = switcher.current_model();
                let target_model = select_model(routing, tier, &current_model);
                if target_model != current_model {
                    if let Err(error) = switcher.switch_model(&target_model).await {
                        debug!(
                            current_model = %current_model,
                            target_model = %target_model,
                            tier = ?tier,
                            error = %error,
                            "Failed to switch routed model"
                        );
                    } else {
                        debug!(
                            current_model = %current_model,
                            target_model = %target_model,
                            tier = ?tier,
                            "Switched model via router"
                        );
                    }
                }
            }

            let request_messages = config.history_pipeline.apply(memory.get_messages());
            let mut request =
                CompletionRequest::new(request_messages).with_tools(self.tools.schemas());

            // Only set temperature if explicitly configured (some models don't support it)
            if let Some(temp) = config.temperature {
                request = request.with_temperature(temp);
            }

            let response = if stream_llm {
                self.get_streaming_completion(
                    request,
                    emitter,
                    config.scratchpad.as_deref(),
                    state.iteration + 1,
                    &state.execution_id,
                    &mut streaming_buffer,
                )
                .await?
            } else {
                self.llm.complete(request).await?
            };

            // Track token usage
            if let Some(usage) = &response.usage {
                total_tokens += usage.total_tokens;
                if let Some(cost) = usage.cost_usd {
                    total_cost_usd += cost;
                    tracker.record_cost(cost);
                }
            }
            if let Err(e) = tracker.check_cost() {
                state.resource_exhaust(e.to_string());
                break;
            }

            // 2. No tool calls → check finish reason and complete
            if response.tool_calls.is_empty() {
                let answer = response.content.unwrap_or_default();
                if let Some(scratchpad) = &config.scratchpad
                    && !answer.is_empty()
                {
                    scratchpad.log_text_delta(state.iteration + 1, &answer);
                }
                let assistant_msg = Message::assistant(&answer);
                state.add_message(assistant_msg.clone());
                memory.add(assistant_msg);
                last_tool_names.clear();

                match response.finish_reason {
                    FinishReason::MaxTokens => {
                        state.fail("Response truncated due to max token limit");
                        if let Some(scratchpad) = &config.scratchpad {
                            scratchpad.log_error(
                                state.iteration + 1,
                                "Response truncated due to max token limit",
                            );
                        }
                        break;
                    }
                    FinishReason::Error => {
                        state.fail("LLM returned an error");
                        if let Some(scratchpad) = &config.scratchpad {
                            scratchpad.log_error(state.iteration + 1, "LLM returned an error");
                        }
                        break;
                    }
                    _ => {
                        if answer.trim().is_empty() && state.iteration == 0 {
                            tracing::warn!("Empty LLM response on first iteration, retrying");
                            state.iteration += 1;
                            continue;
                        }
                        emitter.emit_complete().await;
                        state.complete(&answer);
                        break;
                    }
                }
            }

            // Add assistant message WITH tool_calls to maintain proper conversation history
            // This is required by OpenAI/Anthropic APIs to correlate tool results with their calls
            let tool_call_msg = Message::assistant_with_tool_calls(
                response.content.clone(),
                response.tool_calls.clone(),
            );
            state.add_message(tool_call_msg.clone());
            memory.add(tool_call_msg);

            // Check all resource limits before tool execution
            if let Err(e) = tracker.check() {
                state.resource_exhaust(e.to_string());
                if let Some(scratchpad) = &config.scratchpad {
                    scratchpad.log_error(state.iteration + 1, &e.to_string());
                }
                break;
            }

            if let Some(scratchpad) = &config.scratchpad {
                for call in &response.tool_calls {
                    let arguments = serde_json::to_string(&call.arguments).unwrap_or_default();
                    scratchpad.log_tool_call(state.iteration + 1, &call.id, &call.name, &arguments);
                }
            }

            // 3. Execute tools with timeout and optional stream events.
            let results = self
                .execute_tools_with_events(
                    &config.subflow_path,
                    &response.tool_calls,
                    emitter,
                    config.tool_timeout,
                    config.yolo_mode,
                )
                .await;
            tracker.record_tool_calls(results.len());
            last_tool_names = response
                .tool_calls
                .iter()
                .map(|call| call.name.clone())
                .collect();
            let mut tool_failed = false;

            for (tool_call_id, result) in results {
                let tool_call = response.tool_calls.iter().find(|tc| tc.id == tool_call_id);
                let mut result_str = match result {
                    Ok(output) if output.success => {
                        serde_json::to_string(&output.result).unwrap_or_default()
                    }
                    Ok(output) => {
                        if output
                            .result
                            .get("pending_approval")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                        {
                            if let Some(tool_call) = tool_call {
                                let approval_id = output
                                    .result
                                    .get("approval_id")
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                deferred_manager
                                    .defer(
                                        &tool_call_id,
                                        &tool_call.name,
                                        tool_call.arguments.clone(),
                                        approval_id.clone(),
                                    )
                                    .await;
                                format!(
                                    "Deferred execution for tool '{}' (approval_id: {}). Continuing with other work.",
                                    tool_call.name,
                                    approval_id.unwrap_or_else(|| "unknown".to_string())
                                )
                            } else {
                                "Deferred execution pending user approval.".to_string()
                            }
                        } else {
                            tool_failed = true;
                            format!("Error: {}", output.error.unwrap_or_default())
                        }
                    }
                    Err(e) => {
                        tool_failed = true;
                        format!("Error: {}", e)
                    }
                };

                // Truncate long results to prevent context overflow
                if result_str.len() > config.max_tool_result_length {
                    // Find a safe UTF-8 character boundary
                    let safe_len = result_str
                        .char_indices()
                        .map(|(i, _)| i)
                        .take_while(|&i| i <= config.max_tool_result_length)
                        .last()
                        .unwrap_or(0);
                    result_str = format!(
                        "{}...[truncated, {} chars total]",
                        &result_str[..safe_len],
                        result_str.len()
                    );
                }

                // Record tool call for stuck detection
                if let Some(ref mut detector) = stuck_detector {
                    let args_json = tool_call
                        .map(|tc| serde_json::to_string(&tc.arguments).unwrap_or_default())
                        .unwrap_or_default();
                    let tool_name = tool_call.map(|tc| tc.name.as_str()).unwrap_or("unknown");
                    detector.record(tool_name, &args_json);
                }

                if let Some(scratchpad) = &config.scratchpad {
                    let (tool_name, success) = tool_call
                        .map(|tc| (tc.name.as_str(), !result_str.starts_with("Error: ")))
                        .unwrap_or(("unknown", !result_str.starts_with("Error: ")));
                    scratchpad.log_tool_result(
                        state.iteration + 1,
                        &tool_call_id,
                        tool_name,
                        success,
                        &result_str,
                    );
                }

                // Add tool result to both state and working memory
                let tool_result_msg = Message::tool_result(tool_call_id.clone(), result_str);
                state.add_message(tool_result_msg.clone());
                memory.add(tool_result_msg);
            }
            had_failure = tool_failed;

            // Check for stuck agent after tool execution
            if let Some(ref detector) = stuck_detector
                && let Some(stuck_info) = detector.is_stuck()
            {
                match detector.config().action {
                    StuckAction::Nudge => {
                        tracing::warn!(
                            tool = %stuck_info.repeated_tool,
                            count = stuck_info.repeat_count,
                            "Agent stuck detected, injecting nudge message"
                        );
                        let nudge_msg = Message::system(stuck_info.message);
                        state.add_message(nudge_msg.clone());
                        memory.add(nudge_msg);
                    }
                    StuckAction::Stop => {
                        tracing::warn!(
                            tool = %stuck_info.repeated_tool,
                            count = stuck_info.repeat_count,
                            "Agent stuck detected, stopping execution"
                        );
                        state.fail(format!(
                            "Agent stuck: repeated '{}' {} times",
                            stuck_info.repeated_tool, stuck_info.repeat_count
                        ));
                        if let Some(scratchpad) = &config.scratchpad {
                            scratchpad.log_error(
                                state.iteration + 1,
                                &format!(
                                    "Agent stuck: repeated '{}' {} times",
                                    stuck_info.repeated_tool, stuck_info.repeat_count
                                ),
                            );
                        }
                        break;
                    }
                }
            }

            state.increment_iteration();
            self.maybe_checkpoint(&config, &state, false).await?;

            for (_id, content) in streaming_buffer.flush_all() {
                emitter.emit_text_delta(&content).await;
            }
        }

        for (_id, content) in streaming_buffer.flush_all() {
            emitter.emit_text_delta(&content).await;
        }

        // Build result
        let resource_usage = tracker.usage_snapshot();
        if let Some(scratchpad) = &config.scratchpad {
            scratchpad.log_complete(state.iteration, total_tokens, total_cost_usd);
        }
        self.maybe_checkpoint(&config, &state, true).await?;
        Ok(AgentResult {
            success: matches!(state.status, AgentStatus::Completed),
            answer: state.final_answer.clone(),
            error: match &state.status {
                AgentStatus::Failed { error } => Some(error.clone()),
                AgentStatus::MaxIterations => Some("Max iterations reached".to_string()),
                AgentStatus::Interrupted { reason } => Some(format!("Interrupted: {}", reason)),
                AgentStatus::ResourceExhausted { error } => Some(error.clone()),
                _ => None,
            },
            iterations: state.iteration,
            total_tokens,
            total_cost_usd,
            state,
            compaction_results,
            resource_usage,
        })
    }

    async fn maybe_checkpoint(
        &self,
        config: &AgentConfig,
        state: &AgentState,
        terminal: bool,
    ) -> Result<()> {
        let Some(callback) = &config.checkpoint_callback else {
            return Ok(());
        };
        let should_checkpoint = if terminal {
            matches!(
                config.checkpoint_durability,
                CheckpointDurability::OnComplete
            )
        } else {
            match config.checkpoint_durability {
                CheckpointDurability::PerTurn => true,
                CheckpointDurability::Periodic { interval } => {
                    let interval = interval.max(1);
                    state.iteration > 0 && state.iteration.is_multiple_of(interval)
                }
                CheckpointDurability::OnComplete => false,
            }
        };
        if should_checkpoint {
            callback(state).await?;
        }
        Ok(())
    }

    /// Execute agent and return execution steps as an async stream.
    pub fn run_stream(
        self: Arc<Self>,
        config: AgentConfig,
    ) -> Pin<Box<dyn Stream<Item = ExecutionStep> + Send>> {
        let (tx, mut rx) = mpsc::channel::<ExecutionStep>(128);
        let executor = Arc::clone(&self);

        tokio::spawn(async move {
            let started_execution_id = uuid::Uuid::new_v4().to_string();
            if tx
                .send(ExecutionStep::Started {
                    execution_id: started_execution_id.clone(),
                })
                .await
                .is_err()
            {
                return;
            }

            let mut emitter = ChannelEmitter::new(tx.clone());
            let execution = executor.execute_with_mode(
                config,
                &mut emitter,
                true,
                Some(started_execution_id),
                None,
            );
            tokio::pin!(execution);
            let result = tokio::select! {
                result = &mut execution => result,
                _ = tx.closed() => return,
            };
            match result {
                Ok(result) => {
                    let _ = tx
                        .send(ExecutionStep::Completed {
                            result: Box::new(result),
                        })
                        .await;
                }
                Err(error) => {
                    let _ = tx
                        .send(ExecutionStep::Failed {
                            error: error.to_string(),
                        })
                        .await;
                }
            }
        });

        Box::pin(async_stream::stream! {
            while let Some(step) = rx.recv().await {
                yield step;
            }
        })
    }

    async fn get_streaming_completion(
        &self,
        request: CompletionRequest,
        emitter: &mut dyn StreamEmitter,
        scratchpad: Option<&Scratchpad>,
        iteration: usize,
        execution_id: &str,
        streaming_buffer: &mut StreamingBuffer,
    ) -> Result<crate::llm::CompletionResponse> {
        if !self.llm.supports_streaming() {
            let response = self.llm.complete(request).await?;
            if let Some(content) = &response.content {
                if let Some(flushed) =
                    streaming_buffer.append(execution_id, content, BufferMode::Replace)
                {
                    emitter.emit_text_delta(&flushed).await;
                }
                if let Some(scratchpad) = scratchpad {
                    scratchpad.log_text_delta(iteration, content);
                }
            }
            if let Some(flushed) = streaming_buffer.flush(execution_id) {
                emitter.emit_text_delta(&flushed).await;
            }
            return Ok(response);
        }

        let mut stream = self.llm.complete_stream(request);
        let mut text = String::new();
        let mut accumulator = ToolCallAccumulator::new();
        let mut usage = None;
        let mut finish_reason = None;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;

            if !chunk.text.is_empty() {
                text.push_str(&chunk.text);
                if let Some(flushed) =
                    streaming_buffer.append(execution_id, &chunk.text, BufferMode::Accumulate)
                {
                    emitter.emit_text_delta(&flushed).await;
                }
                if let Some(scratchpad) = scratchpad {
                    scratchpad.log_text_delta(iteration, &chunk.text);
                }
            }

            if let Some(thinking) = &chunk.thinking {
                emitter.emit_thinking_delta(thinking).await;
                if let Some(scratchpad) = scratchpad {
                    scratchpad.log_thinking(iteration, thinking);
                }
            }

            if let Some(delta) = &chunk.tool_call_delta {
                accumulator.accumulate(delta);
            }

            if let Some(chunk_usage) = chunk.usage {
                usage = Some(chunk_usage);
            }

            if let Some(reason) = chunk.finish_reason {
                finish_reason = Some(reason);
            }
        }

        if let Some(flushed) = streaming_buffer.flush(execution_id) {
            emitter.emit_text_delta(&flushed).await;
        }

        Ok(crate::llm::CompletionResponse {
            content: if text.is_empty() { None } else { Some(text) },
            tool_calls: accumulator.finalize(),
            finish_reason: finish_reason.unwrap_or(FinishReason::Stop),
            usage,
        })
    }

    async fn execute_tools_with_events(
        &self,
        subflow_path: &[String],
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        tool_timeout: Duration,
        yolo_mode: bool,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        let all_parallel = tool_calls.iter().all(|call| {
            self.tools
                .get(&call.name)
                .map(|tool| tool.supports_parallel_for(&call.arguments))
                .unwrap_or(false)
        });

        if all_parallel && tool_calls.len() > 1 {
            self.execute_tools_parallel(subflow_path, tool_calls, emitter, tool_timeout, yolo_mode)
                .await
        } else {
            self.execute_tools_sequential(subflow_path, tool_calls, emitter, tool_timeout, yolo_mode)
                .await
        }
    }

    async fn execute_tool_call(
        &self,
        subflow_path: &[String],
        name: &str,
        args: Value,
        yolo_mode: bool,
    ) -> Result<crate::tools::ToolOutput> {
        let mut retry_count = 0usize;

        loop {
            let output = self
                .execute_tool_call_once(subflow_path, name, args.clone(), yolo_mode)
                .await?;
            if output.success {
                return Ok(output);
            }

            let pending_approval = output
                .result
                .get("pending_approval")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if pending_approval {
                return Ok(output);
            }

            let retryable = output.retryable.unwrap_or(false);
            if retryable && retry_count < MAX_TOOL_RETRIES {
                retry_count += 1;
                if let Some(wait_ms) = output.retry_after_ms {
                    sleep(Duration::from_millis(wait_ms)).await;
                }
                continue;
            }

            if matches!(
                output.error_category,
                Some(ToolErrorCategory::Auth | ToolErrorCategory::Config)
            ) {
                let detail = output
                    .error
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string());
                return Ok(output.with_error_message(format!(
                    "Non-retryable error: {}. Try a different approach.",
                    detail
                )));
            }

            return Ok(output);
        }
    }

    async fn execute_tool_call_once(
        &self,
        subflow_path: &[String],
        name: &str,
        mut args: Value,
        yolo_mode: bool,
    ) -> Result<crate::tools::ToolOutput> {
        if yolo_mode
            && name == "bash"
            && let Some(map) = args.as_object_mut()
        {
            map.insert("yolo_mode".to_string(), Value::Bool(true));
        }
        // Inject parent_subflow_path for spawn_agent tool
        if name == "spawn_agent"
            && let Some(map) = args.as_object_mut()
        {
            map.entry("parent_subflow_path".to_string())
                .or_insert_with(|| Value::Array(subflow_path.iter().map(|s| Value::String(s.clone())).collect()));
        }
        self.tools.execute_safe(name, args).await
    }

    async fn execute_tools_sequential(
        &self,
        subflow_path: &[String],
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        tool_timeout: Duration,
        yolo_mode: bool,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        let mut results = Vec::new();

        for call in tool_calls {
            let arguments = serde_json::to_string(&call.arguments).unwrap_or_default();
            emitter
                .emit_tool_call_start(&call.id, &call.name, &arguments)
                .await;

            let result = tokio::time::timeout(
                tool_timeout,
                self.execute_tool_call(subflow_path, &call.name, call.arguments.clone(), yolo_mode),
            )
            .await
            .map_err(|_| AiError::Tool(format!("Tool {} timed out", call.name)))
            .and_then(|result| result);

            if let Ok(output) = &result {
                let result_str = if output.success {
                    serde_json::to_string(&output.result).unwrap_or_default()
                } else {
                    format!("Error: {}", output.error.clone().unwrap_or_default())
                };
                emitter
                    .emit_tool_call_result(&call.id, &call.name, &result_str, output.success)
                    .await;
            } else if let Err(error) = &result {
                let result_str = format!("Error: {}", error);
                emitter
                    .emit_tool_call_result(&call.id, &call.name, &result_str, false)
                    .await;
            }

            results.push((call.id.clone(), result));
        }

        results
    }

    async fn execute_tools_parallel(
        &self,
        subflow_path: &[String],
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        tool_timeout: Duration,
        yolo_mode: bool,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        for call in tool_calls {
            let arguments = serde_json::to_string(&call.arguments).unwrap_or_default();
            emitter
                .emit_tool_call_start(&call.id, &call.name, &arguments)
                .await;
        }

        let executor = self;
        let futures: Vec<_> = tool_calls
            .iter()
            .map(|call| {
                let name = call.name.clone();
                let args = call.arguments.clone();
                let id = call.id.clone();
                let timeout_dur = tool_timeout;
                async move {
                    let result = tokio::time::timeout(
                        timeout_dur,
                        executor.execute_tool_call(&subflow_path, &name, args, yolo_mode),
                    )
                    .await
                    .map_err(|_| AiError::Tool(format!("Tool {} timed out", name)))
                    .and_then(|r| r);
                    (id, name, result)
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;
        let mut output = Vec::new();

        for (id, name, result) in results {
            let (result_str, success) = match &result {
                Ok(output) if output.success => (
                    serde_json::to_string(&output.result).unwrap_or_default(),
                    true,
                ),
                Ok(output) => (
                    format!("Error: {}", output.error.clone().unwrap_or_default()),
                    false,
                ),
                Err(error) => (format!("Error: {}", error), false),
            };

            emitter
                .emit_tool_call_result(&id, &name, &result_str, success)
                .await;
            output.push((id, result));
        }

        output
    }

    async fn build_system_prompt(&self, config: &AgentConfig) -> String {
        let mut sections = Vec::new();

        let base = config
            .system_prompt
            .as_deref()
            .unwrap_or("You are a helpful AI assistant that can use tools to accomplish tasks.");
        sections.push(base.to_string());

        let tools_desc: Vec<String> = self
            .tools
            .list()
            .iter()
            .filter_map(|name| self.tools.get(name))
            .map(|t| format!("- {}: {}", t.name(), t.description()))
            .collect();

        if !tools_desc.is_empty() {
            sections.push(format!("## Available Tools\n\n{}", tools_desc.join("\n")));
        }

        if let Some(cache) = &self.context_cache {
            let context = cache.get().await;
            if !context.content.is_empty() {
                debug!(
                    files = ?context.loaded_files,
                    bytes = context.total_bytes,
                    "Loaded workspace context"
                );
                sections.push(context.content.clone());
            }
        }
        if matches!(config.agent_type, AgentType::Coder | AgentType::Task)
            && let Some(ref context) = config.agent_context
        {
            let context_str = context.format_for_prompt();
            if !context_str.is_empty() {
                sections.push(context_str);
            }
        }

        sections.join("\n\n")
    }
}

fn parse_approval_resolution(instruction: &str) -> Option<(String, bool, Option<String>)> {
    let trimmed = instruction.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("approval ") {
        return None;
    }

    let mut parts = trimmed.splitn(4, ' ');
    let _ = parts.next();
    let approval_id = parts.next()?.trim();
    let action = parts.next()?.trim().to_ascii_lowercase();
    let reason = parts.next().map(|s| s.trim().to_string());

    if action == "approved" {
        Some((approval_id.to_string(), true, reason))
    } else if action == "denied" || action == "rejected" {
        Some((approval_id.to_string(), false, reason))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::TrimOldMessagesProcessor;
    use crate::llm::{
        CompletionResponse, FinishReason, Role, StreamChunk, StreamResult, TokenUsage, ToolCall,
    };
    use crate::tools::{Tool, ToolErrorCategory, ToolOutput};
    use async_trait::async_trait;
    use futures::stream;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex as AsyncMutex;

    /// Mock LLM client for testing
    struct MockLlmClient {
        responses: Mutex<Vec<CompletionResponse>>,
        call_count: AtomicUsize,
        supports_streaming: bool,
        /// Captured requests for verification
        captured_requests: Mutex<Vec<Vec<Message>>>,
    }

    impl MockLlmClient {
        fn new(responses: Vec<CompletionResponse>) -> Self {
            Self::with_streaming(responses, true)
        }

        fn with_streaming(responses: Vec<CompletionResponse>, supports_streaming: bool) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
                supports_streaming,
                captured_requests: Mutex::new(Vec::new()),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }

        fn captured_requests(&self) -> Vec<Vec<Message>> {
            self.captured_requests.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl LlmClient for MockLlmClient {
        fn provider(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-model"
        }

        async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
            self.call_count.fetch_add(1, Ordering::SeqCst);

            // Capture the messages sent to the LLM
            self.captured_requests
                .lock()
                .unwrap()
                .push(request.messages.clone());

            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                Ok(CompletionResponse {
                    content: Some("Done".to_string()),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                    usage: Some(TokenUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        total_tokens: 15,
                        cost_usd: None,
                    }),
                })
            } else {
                Ok(responses.remove(0))
            }
        }

        fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
            // For mock: convert the sync response to a single-chunk stream
            let response = futures::executor::block_on(self.complete(request));
            match response {
                Ok(resp) => {
                    let chunk = StreamChunk {
                        text: resp.content.unwrap_or_default(),
                        thinking: None,
                        tool_call_delta: None,
                        finish_reason: Some(resp.finish_reason),
                        usage: resp.usage,
                    };
                    Box::pin(stream::once(async move { Ok(chunk) }))
                }
                Err(e) => Box::pin(stream::once(async move { Err(e) })),
            }
        }

        fn supports_streaming(&self) -> bool {
            self.supports_streaming
        }
    }

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echo the input payload"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                }
            })
        }

        async fn execute(&self, input: Value) -> Result<ToolOutput> {
            Ok(ToolOutput::success(input))
        }
    }

    struct PendingApprovalTool;

    #[async_trait]
    impl Tool for PendingApprovalTool {
        fn name(&self) -> &str {
            "approval_tool"
        }

        fn description(&self) -> &str {
            "Always returns pending approval"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                }
            })
        }

        async fn execute(&self, _input: Value) -> Result<ToolOutput> {
            Ok(ToolOutput {
                success: false,
                result: serde_json::json!({
                    "pending_approval": true,
                    "approval_id": "approval-test-1"
                }),
                error: Some("Approval required".to_string()),
                error_category: None,
                retryable: None,
                retry_after_ms: None,
            })
        }
    }

    struct RetryThenSuccessTool {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Tool for RetryThenSuccessTool {
        fn name(&self) -> &str {
            "retry_once_tool"
        }

        fn description(&self) -> &str {
            "Fails once with retryable error then succeeds"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type":"object"})
        }

        async fn execute(&self, _input: Value) -> Result<ToolOutput> {
            let current = self.calls.fetch_add(1, Ordering::SeqCst);
            if current == 0 {
                Ok(ToolOutput::retryable_error(
                    "temporary network failure",
                    ToolErrorCategory::Network,
                ))
            } else {
                Ok(ToolOutput::success(serde_json::json!({"ok": true})))
            }
        }
    }

    struct NonRetryableTool {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Tool for NonRetryableTool {
        fn name(&self) -> &str {
            "non_retryable_tool"
        }

        fn description(&self) -> &str {
            "Always fails with non-retryable config error"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type":"object"})
        }

        async fn execute(&self, _input: Value) -> Result<ToolOutput> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ToolOutput::non_retryable_error(
                "missing required config",
                ToolErrorCategory::Config,
            ))
        }
    }

    struct CapturingEmitter {
        text: Arc<AsyncMutex<Vec<String>>>,
        completed: Arc<AtomicUsize>,
    }

    impl CapturingEmitter {
        fn new() -> Self {
            Self {
                text: Arc::new(AsyncMutex::new(Vec::new())),
                completed: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait]
    impl StreamEmitter for CapturingEmitter {
        async fn emit_text_delta(&mut self, text: &str) {
            self.text.lock().await.push(text.to_string());
        }

        async fn emit_thinking_delta(&mut self, _text: &str) {}

        async fn emit_tool_call_start(&mut self, _id: &str, _name: &str, _arguments: &str) {}

        async fn emit_tool_call_result(
            &mut self,
            _id: &str,
            _name: &str,
            _result: &str,
            _success: bool,
        ) {
        }

        async fn emit_complete(&mut self) {
            self.completed.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn test_agent_config_max_memory_messages() {
        let config = AgentConfig::new("Test goal").with_max_memory_messages(50);
        assert_eq!(config.max_memory_messages, 50);
    }

    #[tokio::test]
    async fn test_agent_config_default_memory_messages() {
        let config = AgentConfig::new("Test goal");
        assert_eq!(config.max_memory_messages, DEFAULT_MAX_MESSAGES);
    }

    #[tokio::test]
    async fn test_executor_simple_completion() {
        let response = CompletionResponse {
            content: Some("Hello, I'm done!".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: Some(TokenUsage {
                prompt_tokens: 20,
                completion_tokens: 10,
                total_tokens: 30,
                cost_usd: None,
            }),
        };

        let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm.clone(), tools);

        let config = AgentConfig::new("Say hello");
        let result = executor.run(config).await.unwrap();

        assert!(result.success);
        assert_eq!(result.answer, Some("Hello, I'm done!".to_string()));
        assert_eq!(mock_llm.call_count(), 1);
    }

    #[tokio::test]
    async fn test_execute_from_state_resumes_without_reinjecting_prompt() {
        let response = CompletionResponse {
            content: Some("Resumed done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        };

        let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm.clone(), tools);

        let mut state = AgentState::new("resume-exec-1".to_string(), 10);
        state.iteration = 3;
        state.add_message(Message::system("Existing system"));
        state.add_message(Message::user("Existing user"));
        state.add_message(Message::assistant("Existing assistant"));

        let mut emitter = NullEmitter;
        let result = executor
            .execute_from_state(AgentConfig::new("ignored new goal"), state, &mut emitter)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.state.execution_id, "resume-exec-1");
        assert_eq!(mock_llm.call_count(), 1);
        assert!(
            result
                .state
                .messages
                .iter()
                .any(|msg| msg.content == "Resumed done")
        );
    }

    #[tokio::test]
    async fn test_checkpoint_durability_per_turn_triggers_callback() {
        let responses = vec![
            CompletionResponse {
                content: Some("Tool".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "echo".to_string(),
                    arguments: serde_json::json!({"message":"hello"}),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            CompletionResponse {
                content: Some("Done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];
        let mock_llm = Arc::new(MockLlmClient::new(responses));
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);
        let executor = AgentExecutor::new(mock_llm, Arc::new(registry));

        let checkpoint_count = Arc::new(AtomicUsize::new(0));
        let count_ref = checkpoint_count.clone();
        let config = AgentConfig::new("checkpoint")
            .with_checkpoint_durability(CheckpointDurability::PerTurn)
            .with_checkpoint_callback(move |_| {
                let count_ref = count_ref.clone();
                async move {
                    count_ref.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            });

        let result = executor.run(config).await.unwrap();
        assert!(result.success);
        assert_eq!(checkpoint_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_executor_uses_working_memory() {
        // Create a response that completes immediately
        let response = CompletionResponse {
            content: Some("Done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        };

        let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm.clone(), tools);

        let config = AgentConfig::new("Test task")
            .with_system_prompt("You are a test assistant")
            .with_max_memory_messages(10);

        let result = executor.run(config).await.unwrap();
        assert!(result.success);

        // Verify the messages sent to LLM
        let requests = mock_llm.captured_requests();
        assert_eq!(requests.len(), 1);

        let messages = &requests[0];
        assert_eq!(messages.len(), 2); // system + user
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        assert!(messages[1].content.contains("Test task"));
    }

    #[tokio::test]
    async fn test_executor_applies_history_pipeline_before_llm_call() {
        let responses = vec![
            CompletionResponse {
                content: Some("step".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "unknown_tool".to_string(),
                    arguments: serde_json::json!({}),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            CompletionResponse {
                content: Some("done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];

        let mock_llm = Arc::new(MockLlmClient::new(responses));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm.clone(), tools);

        let config = AgentConfig::new("Test history pipeline")
            .with_max_memory_messages(20)
            .with_history_processor(Arc::new(TrimOldMessagesProcessor::new(1)));

        let result = executor.run(config).await.unwrap();
        assert!(result.success);

        let requests = mock_llm.captured_requests();
        let second_request = &requests[1];

        assert!(
            second_request
                .iter()
                .any(|msg| msg.content == "[Earlier conversation trimmed]")
        );
    }

    #[tokio::test]
    async fn test_executor_memory_window_limits_context() {
        // Create responses for a multi-turn conversation
        let responses = vec![
            // First response with tool call
            CompletionResponse {
                content: Some("Let me help".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "unknown_tool".to_string(),
                    arguments: serde_json::json!({}),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            // Second response (completion)
            CompletionResponse {
                content: Some("All done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];

        let mock_llm = Arc::new(MockLlmClient::new(responses));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm.clone(), tools);

        // Set a very small memory limit
        let config = AgentConfig::new("Multi-turn task").with_max_memory_messages(4); // system + user + assistant + tool_result

        let result = executor.run(config).await.unwrap();
        assert!(result.success);
        assert_eq!(mock_llm.call_count(), 2);

        // Second call should have all messages (within limit)
        let requests = mock_llm.captured_requests();
        let second_request = &requests[1];

        // Should have: system, user, assistant (with tool calls), tool result
        assert_eq!(second_request.len(), 4);
    }

    #[tokio::test]
    async fn test_executor_state_tracks_full_history() {
        let responses = vec![
            CompletionResponse {
                content: Some("Step 1".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "test".to_string(),
                    arguments: serde_json::json!({}),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            CompletionResponse {
                content: Some("Done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];

        let mock_llm = Arc::new(MockLlmClient::new(responses));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm, tools);

        let config = AgentConfig::new("Test").with_max_memory_messages(100); // Large enough to hold all

        let result = executor.run(config).await.unwrap();

        // State should have full history
        // system + user + assistant(tool_call) + tool_result + assistant(final)
        assert_eq!(result.state.messages.len(), 5);
    }

    #[tokio::test]
    async fn test_executor_defers_approval_and_continues() {
        let responses = vec![
            CompletionResponse {
                content: Some("Need a tool".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "approval_tool".to_string(),
                    arguments: serde_json::json!({"command": "danger"}),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            CompletionResponse {
                content: Some("continued".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];

        let mock_llm = Arc::new(MockLlmClient::new(responses));
        let mut registry = ToolRegistry::new();
        registry.register(PendingApprovalTool);
        let executor = AgentExecutor::new(mock_llm.clone(), Arc::new(registry));

        let result = executor
            .run(AgentConfig::new("test deferred"))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(mock_llm.call_count(), 2);
        assert!(result.state.messages.iter().any(|m| {
            m.content
                .contains("Deferred execution for tool 'approval_tool'")
        }));
    }

    #[tokio::test]
    async fn test_executor_retries_retryable_tool_errors() {
        let responses = vec![
            CompletionResponse {
                content: Some("try tool".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "retry_once_tool".to_string(),
                    arguments: serde_json::json!({}),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            CompletionResponse {
                content: Some("done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];

        let calls = Arc::new(AtomicUsize::new(0));
        let tool = RetryThenSuccessTool {
            calls: calls.clone(),
        };
        let mock_llm = Arc::new(MockLlmClient::new(responses));
        let mut registry = ToolRegistry::new();
        registry.register(tool);
        let executor = AgentExecutor::new(mock_llm, Arc::new(registry));

        let result = executor.run(AgentConfig::new("retry test")).await.unwrap();
        assert!(result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_executor_skips_retry_for_non_retryable_errors() {
        let responses = vec![
            CompletionResponse {
                content: Some("try tool".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "non_retryable_tool".to_string(),
                    arguments: serde_json::json!({}),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            CompletionResponse {
                content: Some("done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];

        let calls = Arc::new(AtomicUsize::new(0));
        let tool = NonRetryableTool {
            calls: calls.clone(),
        };
        let mock_llm = Arc::new(MockLlmClient::new(responses));
        let mut registry = ToolRegistry::new();
        registry.register(tool);
        let executor = AgentExecutor::new(mock_llm, Arc::new(registry));

        let result = executor
            .run(AgentConfig::new("non retry test"))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_run_stream_basic() {
        let response = CompletionResponse {
            content: Some("stream-finished".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        };

        let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
        let tools = Arc::new(ToolRegistry::new());
        let executor = Arc::new(AgentExecutor::new(mock_llm, tools));

        let mut stream = executor.run_stream(AgentConfig::new("Say hello"));
        let mut saw_text_delta = false;
        let mut saw_completed = false;

        while let Some(step) = stream.next().await {
            match step {
                ExecutionStep::TextDelta { content } => {
                    saw_text_delta = true;
                    assert_eq!(content, "stream-finished");
                }
                ExecutionStep::Completed { result } => {
                    assert!(result.success);
                    saw_completed = true;
                    break;
                }
                ExecutionStep::Failed { error } => panic!("unexpected failure: {error}"),
                _ => {}
            }
        }

        assert!(saw_text_delta);
        assert!(saw_completed);
    }

    #[tokio::test]
    async fn test_run_stream_with_tools() {
        let responses = vec![
            CompletionResponse {
                content: Some("Calling tool".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "echo".to_string(),
                    arguments: serde_json::json!({ "message": "hello" }),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            CompletionResponse {
                content: Some("done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];

        let mock_llm = Arc::new(MockLlmClient::with_streaming(responses, false));
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);
        let executor = Arc::new(AgentExecutor::new(mock_llm, Arc::new(registry)));

        let mut stream = executor.run_stream(AgentConfig::new("Run echo"));
        let mut saw_tool_start = false;
        let mut saw_tool_result = false;
        let mut saw_completed = false;

        while let Some(step) = stream.next().await {
            match step {
                ExecutionStep::ToolCallStart { name, .. } => {
                    if name == "echo" {
                        saw_tool_start = true;
                    }
                }
                ExecutionStep::ToolCallResult { name, success, .. } => {
                    if name == "echo" {
                        saw_tool_result = true;
                        assert!(success);
                    }
                }
                ExecutionStep::Completed { result } => {
                    saw_completed = true;
                    assert!(result.success);
                    break;
                }
                ExecutionStep::Failed { error } => panic!("unexpected failure: {error}"),
                _ => {}
            }
        }

        assert!(saw_tool_start);
        assert!(saw_tool_result);
        assert!(saw_completed);
    }

    #[tokio::test]
    async fn test_utf8_truncation_chinese_chars() {
        // Create a tool result containing Chinese characters at boundary
        let chinese_text = "这是一个包含中文字符的测试）。".repeat(200); // ~4000 bytes

        let response = CompletionResponse {
            content: Some("Calling tool".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "test".to_string(),
                arguments: serde_json::json!({"result": chinese_text}),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        };

        let mock_llm = Arc::new(MockLlmClient::new(vec![
            response,
            CompletionResponse {
                content: Some("Done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ]));

        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm, tools);

        // Set max_tool_result_length to a value that would split Chinese chars
        let config = AgentConfig::new("Test UTF-8 safety").with_max_tool_result_length(4000);

        // This should NOT panic even with Chinese characters at byte boundary
        let result = executor.run(config).await;
        assert!(result.is_ok(), "Should handle Chinese characters safely");
        assert!(result.unwrap().success);
    }

    #[tokio::test]
    #[allow(deprecated)]
    async fn test_run_via_stream_matches_run_direct() {
        let response = CompletionResponse {
            content: Some("Unified path".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        };

        let direct_llm = Arc::new(MockLlmClient::new(vec![response.clone()]));
        let streaming_llm = Arc::new(MockLlmClient::new(vec![response]));
        let tools = Arc::new(ToolRegistry::new());

        let direct_executor = AgentExecutor::new(direct_llm, tools.clone());
        let streaming_executor = AgentExecutor::new(streaming_llm, tools);
        let config = AgentConfig::new("match");

        let direct = direct_executor.run(config.clone()).await.unwrap();
        let mut emitter = CapturingEmitter::new();
        let streamed = streaming_executor
            .execute_streaming(config, &mut emitter)
            .await
            .unwrap();

        assert_eq!(direct.success, streamed.success);
        assert_eq!(direct.answer, streamed.answer);
        assert_eq!(direct.error, streamed.error);
        assert_eq!(direct.iterations, streamed.iterations);
    }

    #[tokio::test]
    #[allow(deprecated)]
    async fn test_backward_compat_execute_streaming_emits_complete() {
        let response = CompletionResponse {
            content: Some("done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        };

        let llm = Arc::new(MockLlmClient::new(vec![response]));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(llm, tools);
        let mut emitter = CapturingEmitter::new();

        let result = executor
            .execute_streaming(AgentConfig::new("compat"), &mut emitter)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(emitter.completed.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_parse_approval_resolution() {
        assert_eq!(
            parse_approval_resolution("approval abc approved"),
            Some(("abc".to_string(), true, None))
        );
        assert_eq!(
            parse_approval_resolution("approval id-1 denied too dangerous"),
            Some(("id-1".to_string(), false, Some("too dangerous".to_string())))
        );
        assert!(parse_approval_resolution("hello world").is_none());
    }

    #[tokio::test]
    async fn test_run_writes_jsonl_scratchpad_events() {
        let response = CompletionResponse {
            content: Some("done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        };

        let llm = Arc::new(MockLlmClient::new(vec![response]));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(llm, tools);

        let dir = tempfile::tempdir().unwrap();
        let scratchpad_path = dir.path().join("exec.jsonl");
        let scratchpad = Arc::new(Scratchpad::new(scratchpad_path.clone()).unwrap());
        let config = AgentConfig::new("scratchpad").with_scratchpad(scratchpad);

        let result = executor.run(config).await.unwrap();
        assert!(result.success);

        let content = std::fs::read_to_string(scratchpad_path).unwrap();
        assert!(content.contains("\"event_type\":\"execution_start\""));
        assert!(content.contains("\"event_type\":\"iteration_begin\""));
        assert!(content.contains("\"event_type\":\"execution_complete\""));
    }
}
