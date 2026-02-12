//! Agent executor with ReAct loop

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::agent::ExecutionStep;
use crate::agent::context::{AgentContext, ContextDiscoveryConfig, WorkspaceContextCache};
use crate::agent::resource::{ResourceLimits, ResourceTracker, ResourceUsage};
use crate::agent::state::{AgentState, AgentStatus};
use crate::agent::stream::{ChannelEmitter, StreamEmitter, ToolCallAccumulator};
use crate::agent::stuck::{StuckAction, StuckDetector, StuckDetectorConfig};
use crate::error::{AiError, Result};
use crate::llm::{CompletionRequest, FinishReason, LlmClient, Message, ToolCall};
use crate::memory::{CompactionConfig, CompactionResult, DEFAULT_MAX_MESSAGES, WorkingMemory};
use crate::steer::SteerMessage;
use crate::tools::ToolRegistry;
use futures::{Stream, StreamExt};
use tokio::sync::{Mutex, mpsc};
use tracing::debug;

/// Agent type for system prompt composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentType {
    #[default]
    Coder,
    Task,
    Summarizer,
    Title,
}

/// Configuration for agent execution
#[derive(Debug, Clone)]
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
}

impl AgentConfig {
    /// Create a new agent config with a goal
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            system_prompt: None,
            max_iterations: 25,
            temperature: None, // None = use model default
            context: HashMap::new(),
            tool_timeout: Duration::from_secs(30),
            max_tool_result_length: 4000,
            max_memory_messages: DEFAULT_MAX_MESSAGES,
            context_window: 128_000,
            compaction_config: None,
            agent_context: None,
            agent_type: AgentType::default(),
            resource_limits: ResourceLimits::default(),
            stuck_detection: Some(StuckDetectorConfig::default()),
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

    async fn apply_steer_messages(&self, state: &mut AgentState, memory: &mut WorkingMemory) {
        let messages = self.drain_steer_messages().await;
        if messages.is_empty() {
            return;
        }

        for steer in messages {
            match &steer.command {
                crate::steer::SteerCommand::Message { instruction } => {
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

    /// Execute agent - simplified Swarm-style loop
    pub async fn run(&self, config: AgentConfig) -> Result<AgentResult> {
        let execution_id = uuid::Uuid::new_v4().to_string();
        let mut state = AgentState::new(execution_id, config.max_iterations);
        state.context = config.context.clone();
        let mut total_tokens: u32 = 0;
        let mut total_cost_usd: f64 = 0.0;
        let mut compaction_results = Vec::new();
        let tracker = ResourceTracker::new(config.resource_limits.clone());

        // Initialize working memory for context window management
        let mut memory = WorkingMemory::new(config.max_memory_messages);
        if let Some(compaction_config) = config.compaction_config.clone() {
            memory.enable_compaction(compaction_config);
        }

        // Initialize stuck detector
        let mut stuck_detector = config.stuck_detection.clone().map(StuckDetector::new);

        // Initialize messages
        let system_prompt = self.build_system_prompt(&config).await;
        let system_msg = Message::system(&system_prompt);
        let user_msg = Message::user(&config.goal);

        // Add to both state (full history) and memory (LLM context window)
        state.add_message(system_msg.clone());
        state.add_message(user_msg.clone());
        memory.add(system_msg);
        memory.add(user_msg);

        // Core loop (Swarm-inspired simplicity)
        while state.iteration < state.max_iterations && !state.is_terminal() {
            let summarizer = self.summarizer.as_deref().unwrap_or(self.llm.as_ref());
            if let Some(result) = memory
                .auto_compact_if_needed(summarizer, config.context_window)
                .await?
            {
                // Compaction affects working memory only; full state history remains intact.
                compaction_results.push(result);
            }

            self.apply_steer_messages(&mut state, &mut memory).await;

            // Check wall-clock before LLM call
            if let Err(e) = tracker.check_wall_clock() {
                state.resource_exhaust(e.to_string());
                break;
            }

            // 1. LLM call - use working memory for context (handles overflow)
            let mut request =
                CompletionRequest::new(memory.get_messages()).with_tools(self.tools.schemas());

            // Only set temperature if explicitly configured (some models don't support it)
            if let Some(temp) = config.temperature {
                request = request.with_temperature(temp);
            }

            let response = self.llm.complete(request).await?;

            // Track token usage
            if let Some(usage) = &response.usage {
                total_tokens += usage.total_tokens;
                if let Some(cost) = usage.cost_usd {
                    total_cost_usd += cost;
                }
            }

            // 2. No tool calls → check finish reason and complete
            if response.tool_calls.is_empty() {
                let answer = response.content.unwrap_or_default();
                let assistant_msg = Message::assistant(&answer);
                state.add_message(assistant_msg.clone());
                memory.add(assistant_msg);

                match response.finish_reason {
                    FinishReason::MaxTokens => {
                        state.fail("Response truncated due to max token limit");
                        break;
                    }
                    FinishReason::Error => {
                        state.fail("LLM returned an error");
                        break;
                    }
                    _ => {
                        // If the response is empty and we haven't done any work yet,
                        // this is likely an anomalous API response — retry the loop
                        // instead of reporting an empty completion.
                        if answer.trim().is_empty() && state.iteration == 0 {
                            tracing::warn!("Empty LLM response on first iteration, retrying");
                            state.iteration += 1;
                            continue;
                        }
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
                break;
            }

            // 3. Execute tools in parallel with timeout (Rig-inspired)
            let tool_futures: Vec<_> = response
                .tool_calls
                .iter()
                .map(|tc| {
                    let name = tc.name.clone();
                    let args = tc.arguments.clone();
                    let tools = Arc::clone(&self.tools);
                    let timeout = config.tool_timeout;
                    async move {
                        // Tool timeout
                        let result =
                            tokio::time::timeout(timeout, tools.execute_safe(&name, args)).await;
                        let result = match result {
                            Ok(r) => r,
                            Err(_) => Err(AiError::Tool(format!("Tool {} timed out", name))),
                        };
                        (tc.id.clone(), name, result)
                    }
                })
                .collect();

            let results = futures::future::join_all(tool_futures).await;
            tracker.record_tool_calls(results.len());

            for (tool_call_id, tool_name, result) in results {
                let mut result_str = match result {
                    Ok(output) if output.success => {
                        serde_json::to_string(&output.result).unwrap_or_default()
                    }
                    Ok(output) => format!("Error: {}", output.error.unwrap_or_default()),
                    Err(e) => format!("Error: {}", e),
                };

                // Truncate long results to prevent context overflow
                if result_str.len() > config.max_tool_result_length {
                    result_str = format!(
                        "{}...[truncated, {} chars total]",
                        &result_str[..config.max_tool_result_length],
                        result_str.len()
                    );
                }

                // Record tool call for stuck detection
                if let Some(ref mut detector) = stuck_detector {
                    let args_json = response
                        .tool_calls
                        .iter()
                        .find(|tc| tc.id == tool_call_id)
                        .map(|tc| serde_json::to_string(&tc.arguments).unwrap_or_default())
                        .unwrap_or_default();
                    detector.record(&tool_name, &args_json);
                }

                // Add tool result to both state and working memory
                let tool_result_msg = Message::tool_result(tool_call_id.clone(), result_str);
                state.add_message(tool_result_msg.clone());
                memory.add(tool_result_msg);
            }

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
                        break;
                    }
                }
            }

            state.increment_iteration();
        }

        // Build result
        let resource_usage = tracker.usage_snapshot();
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

    pub async fn execute_streaming(
        &self,
        config: AgentConfig,
        emitter: &mut dyn StreamEmitter,
    ) -> Result<AgentResult> {
        let execution_id = uuid::Uuid::new_v4().to_string();
        let mut state = AgentState::new(execution_id, config.max_iterations);
        state.context = config.context.clone();
        let mut total_tokens: u32 = 0;
        let mut total_cost_usd: f64 = 0.0;
        let mut compaction_results = Vec::new();
        let tracker = ResourceTracker::new(config.resource_limits.clone());

        let mut memory = WorkingMemory::new(config.max_memory_messages);
        if let Some(compaction_config) = config.compaction_config.clone() {
            memory.enable_compaction(compaction_config);
        }

        let mut stuck_detector = config.stuck_detection.clone().map(StuckDetector::new);

        let system_prompt = self.build_system_prompt(&config).await;
        let system_msg = Message::system(&system_prompt);
        let user_msg = Message::user(&config.goal);

        state.add_message(system_msg.clone());
        state.add_message(user_msg.clone());
        memory.add(system_msg);
        memory.add(user_msg);

        while state.iteration < state.max_iterations && !state.is_terminal() {
            let summarizer = self.summarizer.as_deref().unwrap_or(self.llm.as_ref());
            if let Some(result) = memory
                .auto_compact_if_needed(summarizer, config.context_window)
                .await?
            {
                // Compaction affects working memory only; full state history remains intact.
                compaction_results.push(result);
            }

            self.apply_steer_messages(&mut state, &mut memory).await;

            // Check wall-clock before LLM call
            if let Err(e) = tracker.check_wall_clock() {
                state.resource_exhaust(e.to_string());
                break;
            }

            let mut request =
                CompletionRequest::new(memory.get_messages()).with_tools(self.tools.schemas());

            if let Some(temp) = config.temperature {
                request = request.with_temperature(temp);
            }

            let response = self.get_streaming_completion(request, emitter).await?;

            if let Some(usage) = &response.usage {
                total_tokens += usage.total_tokens;
                if let Some(cost) = usage.cost_usd {
                    total_cost_usd += cost;
                }
            }

            if response.tool_calls.is_empty() {
                let answer = response.content.unwrap_or_default();
                let assistant_msg = Message::assistant(&answer);
                state.add_message(assistant_msg.clone());
                memory.add(assistant_msg);

                match response.finish_reason {
                    FinishReason::MaxTokens => {
                        state.fail("Response truncated due to max token limit");
                        break;
                    }
                    FinishReason::Error => {
                        state.fail("LLM returned an error");
                        break;
                    }
                    _ => {
                        emitter.emit_complete().await;
                        state.complete(&answer);
                        break;
                    }
                }
            }

            let tool_call_msg = Message::assistant_with_tool_calls(
                response.content.clone(),
                response.tool_calls.clone(),
            );
            state.add_message(tool_call_msg.clone());
            memory.add(tool_call_msg);

            // Check all resource limits before tool execution
            if let Err(e) = tracker.check() {
                state.resource_exhaust(e.to_string());
                break;
            }

            let results = self
                .execute_tools_with_events(&response.tool_calls, emitter, config.tool_timeout)
                .await;
            tracker.record_tool_calls(results.len());

            for (tool_call_id, result) in results {
                let mut result_str = match result {
                    Ok(output) if output.success => {
                        serde_json::to_string(&output.result).unwrap_or_default()
                    }
                    Ok(output) => format!("Error: {}", output.error.unwrap_or_default()),
                    Err(e) => format!("Error: {}", e),
                };

                if result_str.len() > config.max_tool_result_length {
                    result_str = format!(
                        "{}...[truncated, {} chars total]",
                        &result_str[..config.max_tool_result_length],
                        result_str.len()
                    );
                }

                // Record tool call for stuck detection
                if let Some(ref mut detector) = stuck_detector {
                    let args_json = response
                        .tool_calls
                        .iter()
                        .find(|tc| tc.id == tool_call_id)
                        .map(|tc| serde_json::to_string(&tc.arguments).unwrap_or_default())
                        .unwrap_or_default();
                    // tool_call_id format is unique per call, find tool name from response
                    let tool_name = response
                        .tool_calls
                        .iter()
                        .find(|tc| tc.id == tool_call_id)
                        .map(|tc| tc.name.as_str())
                        .unwrap_or("unknown");
                    detector.record(tool_name, &args_json);
                }

                let tool_result_msg = Message::tool_result(tool_call_id.clone(), result_str);
                state.add_message(tool_result_msg.clone());
                memory.add(tool_result_msg);
            }

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
                        break;
                    }
                }
            }

            state.increment_iteration();
        }

        let resource_usage = tracker.usage_snapshot();
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

    /// Execute agent and return execution steps as an async stream.
    pub fn run_stream(
        self: Arc<Self>,
        config: AgentConfig,
    ) -> Pin<Box<dyn Stream<Item = ExecutionStep> + Send>> {
        let (tx, mut rx) = mpsc::channel::<ExecutionStep>(128);
        let executor = Arc::clone(&self);

        tokio::spawn(async move {
            let started_execution_id = uuid::Uuid::new_v4().to_string();
            let _ = tx
                .send(ExecutionStep::Started {
                    execution_id: started_execution_id,
                })
                .await;

            let mut emitter = ChannelEmitter::new(tx.clone());
            let result = executor.execute_streaming(config, &mut emitter).await;
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
    ) -> Result<crate::llm::CompletionResponse> {
        if !self.llm.supports_streaming() {
            let response = self.llm.complete(request).await?;
            if let Some(content) = &response.content {
                emitter.emit_text_delta(content).await;
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
                emitter.emit_text_delta(&chunk.text).await;
            }

            if let Some(thinking) = &chunk.thinking {
                emitter.emit_thinking_delta(thinking).await;
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

        Ok(crate::llm::CompletionResponse {
            content: if text.is_empty() { None } else { Some(text) },
            tool_calls: accumulator.finalize(),
            finish_reason: finish_reason.unwrap_or(FinishReason::Stop),
            usage,
        })
    }

    async fn execute_tools_with_events(
        &self,
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        tool_timeout: Duration,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        let all_parallel = tool_calls.iter().all(|call| {
            self.tools
                .get(&call.name)
                .map(|tool| tool.supports_parallel_for(&call.arguments))
                .unwrap_or(false)
        });

        if all_parallel && tool_calls.len() > 1 {
            self.execute_tools_parallel(tool_calls, emitter, tool_timeout)
                .await
        } else {
            self.execute_tools_sequential(tool_calls, emitter, tool_timeout)
                .await
        }
    }

    async fn execute_tools_sequential(
        &self,
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        tool_timeout: Duration,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        let mut results = Vec::new();

        for call in tool_calls {
            let arguments = serde_json::to_string(&call.arguments).unwrap_or_default();
            emitter
                .emit_tool_call_start(&call.id, &call.name, &arguments)
                .await;

            let result = tokio::time::timeout(
                tool_timeout,
                self.tools.execute_safe(&call.name, call.arguments.clone()),
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
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        tool_timeout: Duration,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        for call in tool_calls {
            let arguments = serde_json::to_string(&call.arguments).unwrap_or_default();
            emitter
                .emit_tool_call_start(&call.id, &call.name, &arguments)
                .await;
        }

        let tools = Arc::clone(&self.tools);
        let futures: Vec<_> = tool_calls
            .iter()
            .map(|call| {
                let name = call.name.clone();
                let args = call.arguments.clone();
                let id = call.id.clone();
                let tools = Arc::clone(&tools);
                let timeout_dur = tool_timeout;
                async move {
                    let result = tokio::time::timeout(timeout_dur, tools.execute_safe(&name, args))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CompletionResponse, FinishReason, Role, StreamChunk, StreamResult, TokenUsage, ToolCall,
    };
    use crate::tools::{Tool, ToolOutput};
    use async_trait::async_trait;
    use futures::stream;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
}
