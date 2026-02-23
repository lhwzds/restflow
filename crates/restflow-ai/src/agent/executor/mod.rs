//! Agent executor with ReAct loop
//!
//! # Timeout Architecture
//!
//! The executor applies a **wrapper timeout** around all tool executions. When a tool
//! has its own internal timeout, there are **two layers of timeout**:
//!
//! 1. **Executor wrapper timeout** (`tool_timeout`): Controls how long the entire
//!    tool execution can take, including any overhead. Default: 300s.
//!    Configurable via `AgentConfig::with_tool_timeout()`.
//!
//! 2. **Tool-internal timeout**: Some tools (like `bash`, `python`) have their own
//!    timeout for the actual operation:
//!    - `bash`: `timeout_secs` (default 300s)
//!    - `python`: `timeout_seconds` (default varies)
//!
//! **Important**: To avoid confusing timeout errors, ensure the executor wrapper
//! timeout is **greater than or equal to** the tool-internal timeout plus a small
//! buffer. If the wrapper timeout fires first, you'll get a generic "Tool X timed out"
//! error instead of the tool's more specific timeout message.
//!
//! **Recommended configuration**:
//! - `agent.tool_timeout_secs` >= max(`bash_timeout_secs`, `python_timeout_secs`) + 10s
//! - Example: If bash needs 300s, set `tool_timeout_secs` to 310-320s

mod config;
mod prompt;
mod streaming;
pub use config::*;

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::agent::ExecutionStep;
use crate::agent::context::{ContextDiscoveryConfig, WorkspaceContextCache};
use crate::agent::context_manager::{self, ContextManagerConfig, TokenEstimator};
use crate::agent::deferred::{DeferredExecutionManager, DeferredStatus};
use crate::agent::model_router::{classify_task, select_model};
use crate::agent::resource::ResourceTracker;
use crate::agent::scratchpad::Scratchpad;
use crate::agent::state::{AgentState, AgentStatus};
use crate::agent::stream::{ChannelEmitter, NullEmitter, StreamEmitter, ToolCallAccumulator};
use crate::agent::streaming_buffer::{BufferMode, StreamingBuffer};
use crate::agent::stuck::{StuckAction, StuckDetector};
use crate::error::{AiError, Result};
use crate::llm::{CompletionRequest, FinishReason, LlmClient, Message, Role, ToolCall};
use crate::agent::sub_agent::SubagentTracker;
use crate::steer::SteerMessage;
use crate::tools::{ToolErrorCategory, ToolRegistry};
use dashmap::DashMap;
use futures::stream::FuturesOrdered;
use futures::{Stream, StreamExt};
use tokio::sync::{Mutex, Semaphore, mpsc};
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::sleep;
use tracing::debug;

/// Truncate tool output with middle-truncation and optional disk persistence.
/// Returns the (possibly truncated) string with a retrieval hint if the full output was saved.
fn truncate_tool_output(
    content: &str,
    max_len: usize,
    scratchpad: Option<&Scratchpad>,
    call_id: &str,
    tool_name: &str,
) -> String {
    if content.len() <= max_len {
        return content.to_string();
    }

    let total_len = content.len();

    // Save full output to disk before truncating
    let saved_path = scratchpad.and_then(|sp| sp.save_full_output(call_id, tool_name, content));

    // Build the retrieval hint
    let hint = if let Some(ref path) = saved_path {
        format!(
            "\n\n[Full output ({total_len} chars) saved to: {}. \
             Use file read tool with offset/limit to view specific sections, \
             or use search to find specific content.]",
            path.display()
        )
    } else {
        String::new()
    };

    // Middle-truncate the content, leaving room for the hint
    let truncate_target = max_len.saturating_sub(hint.len());
    let mut result = context_manager::middle_truncate(content, truncate_target);
    result.push_str(&hint);
    result
}

/// Agent executor implementing Swarm-style ReAct loop
pub struct AgentExecutor {
    pub(crate) llm: Arc<dyn LlmClient>,
    pub(crate) tools: Arc<ToolRegistry>,
    pub(crate) context_cache: Option<WorkspaceContextCache>,
    pub(crate) steer_rx: Option<Mutex<mpsc::Receiver<SteerMessage>>>,
    /// Optional sub-agent tracker for completion notification injection.
    pub(crate) subagent_tracker: Option<Arc<SubagentTracker>>,
    /// Active tool calls that can be individually cancelled.
    pub(crate) active_tool_calls: Arc<DashMap<String, AbortHandle>>,
    /// Buffer for steer messages that were read during tool drain but need
    /// to be processed by `apply_steer_messages` at the next iteration.
    pub(crate) steer_buffer: Mutex<Vec<SteerMessage>>,
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
            context_cache,
            steer_rx: None,
            subagent_tracker: None,
            active_tool_calls: Arc::new(DashMap::new()),
            steer_buffer: Mutex::new(Vec::new()),
        }
    }

    /// Attach a steer channel for live instruction updates.
    pub fn with_steer_channel(mut self, rx: mpsc::Receiver<SteerMessage>) -> Self {
        self.steer_rx = Some(Mutex::new(rx));
        self
    }

    /// Attach a sub-agent tracker for automatic completion notification injection.
    pub fn with_subagent_tracker(mut self, tracker: Arc<SubagentTracker>) -> Self {
        self.subagent_tracker = Some(tracker);
        self
    }

    /// Poll the sub-agent tracker for completions and inject notification messages.
    async fn poll_subagent_completions(
        &self,
        state: &mut AgentState,
        max_result_length: usize,
    ) {
        let Some(tracker) = &self.subagent_tracker else {
            return;
        };

        let completions = tracker.poll_completions().await;
        if completions.is_empty() {
            return;
        }

        for completion in completions {
            let agent_name = tracker
                .get(&completion.id)
                .map(|s| s.agent_name.clone())
                .unwrap_or_else(|| "unknown".to_string());

            let status_str = if completion.result.success {
                "completed"
            } else {
                "failed"
            };

            let mut output = completion.result.output.clone();
            if output.len() > max_result_length {
                output = context_manager::middle_truncate(&output, max_result_length);
            }

            let error_tag = match &completion.result.error {
                Some(err) => format!("\n  <error>{}</error>", err),
                None => String::new(),
            };

            let notification = format!(
                "<subagent_notification>\n  \
                 <task_id>{}</task_id>\n  \
                 <agent>{}</agent>\n  \
                 <status>{}</status>\n  \
                 <duration_ms>{}</duration_ms>\n  \
                 <output>{}</output>{}\n\
                 </subagent_notification>",
                completion.id,
                agent_name,
                status_str,
                completion.result.duration_ms,
                output,
                error_tag,
            );

            tracing::info!(
                task_id = %completion.id,
                agent = %agent_name,
                status = %status_str,
                "Injecting sub-agent completion notification"
            );

            state.add_message(Message::system(notification));
        }
    }

    async fn drain_steer_messages(&self) -> Vec<SteerMessage> {
        // First, drain any buffered messages from the tool-drain phase
        let mut messages = {
            let mut buffer = self.steer_buffer.lock().await;
            std::mem::take(&mut *buffer)
        };

        let Some(rx) = &self.steer_rx else {
            return messages;
        };

        let mut rx = rx.lock().await;
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
                        state.add_message(msg);
                        continue;
                    }
                    tracing::info!(
                        instruction = %instruction,
                        source = ?steer.source,
                        "Received steer message, injecting into conversation"
                    );
                    let msg = Message::user(format!("[User Update]: {}", instruction));
                    state.add_message(msg);
                }
                crate::steer::SteerCommand::Interrupt { reason, .. } => {
                    tracing::info!(
                        reason = %reason,
                        source = ?steer.source,
                        "Received interrupt command"
                    );
                    state.interrupt(reason);
                }
                crate::steer::SteerCommand::CancelToolCall { tool_call_id } => {
                    if let Some((_, abort_handle)) =
                        self.active_tool_calls.remove(tool_call_id)
                    {
                        abort_handle.abort();
                        tracing::info!(
                            tool_call_id = %tool_call_id,
                            source = ?steer.source,
                            "Tool call cancelled via steer"
                        );
                    }
                }
            }
        }
    }

    async fn process_resolved_deferred_calls(
        &self,
        deferred_manager: &DeferredExecutionManager,
        state: &mut AgentState,
        tool_timeout: Duration,
        max_tool_result_length: usize,
        scratchpad: Option<&Scratchpad>,
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
                        self.execute_tool_call(&deferred.tool_name, deferred.args.clone(), false),
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
                    text = truncate_tool_output(
                        &text,
                        max_tool_result_length,
                        scratchpad,
                        &deferred.call_id,
                        &deferred.tool_name,
                    );
                    let msg = Message::system(text);
                    state.add_message(msg);
                }
                DeferredStatus::Denied { reason } => {
                    let msg = Message::system(format!(
                        "Deferred tool call '{}' was denied: {}",
                        deferred.tool_name, reason
                    ));
                    state.add_message(msg);
                }
                DeferredStatus::TimedOut => {
                    let msg = Message::system(format!(
                        "Approval timed out for deferred tool call '{}'.",
                        deferred.tool_name
                    ));
                    state.add_message(msg);
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

    /// Resume execution from an existing state snapshot in non-stream mode.
    pub async fn run_from_state(
        &self,
        config: AgentConfig,
        mut state: AgentState,
    ) -> Result<AgentResult> {
        state.status = AgentStatus::Running;
        state.ended_at = None;
        let execution_id = state.execution_id.clone();
        let mut emitter = NullEmitter;
        self.execute_with_mode(config, &mut emitter, false, Some(execution_id), Some(state))
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
        let tracker = ResourceTracker::new(config.resource_limits.clone());
        let context_config = ContextManagerConfig::default()
            .with_context_window(config.context_window);
        let mut token_estimator = TokenEstimator::default();

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

            state.add_message(system_msg);
            state.add_message(user_msg);
        }

        // Core loop (Swarm-inspired simplicity)
        while state.iteration < state.max_iterations && !state.is_terminal() {
            if let Some(scratchpad) = &config.scratchpad {
                scratchpad.log_iteration_begin(state.iteration + 1);
            }
            self.apply_steer_messages(&mut state, &deferred_manager)
                .await;
            self.poll_subagent_completions(&mut state, config.max_tool_result_length)
                .await;
            self.process_resolved_deferred_calls(
                &deferred_manager,
                &mut state,
                config.tool_timeout,
                config.max_tool_result_length,
                config.scratchpad.as_deref(),
            )
            .await;

            // Check wall-clock before LLM call
            if let Err(e) = tracker.check_wall_clock() {
                state.resource_exhaust(e.to_string());
                break;
            }

            // 1. LLM call
            if let Some(routing) = config
                .model_routing
                .as_ref()
                .filter(|routing| routing.enabled)
                && let Some(switcher) = config.model_switcher.as_ref()
            {
                let tool_names: Vec<&str> = last_tool_names.iter().map(String::as_str).collect();
                let messages = state.messages.clone();
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

            // Context management: compact if approaching context window limit
            token_estimator.tick_cooldown();
            let estimated = token_estimator.estimate(&state.messages);
            if token_estimator.compact_allowed()
                && context_manager::should_compact(estimated, &context_config)
            {
                match context_manager::compact(
                    &mut state.messages,
                    &context_config,
                    self.llm.as_ref(),
                )
                .await
                {
                    Ok(stats) => {
                        tracing::info!(
                            messages_replaced = stats.messages_replaced,
                            tokens_before = stats.tokens_before,
                            tokens_after = stats.tokens_after,
                            "Context compacted"
                        );
                        if !context_manager::compact_was_effective(&stats) {
                            tracing::warn!("Compact was ineffective, entering cooldown");
                            token_estimator.start_compact_cooldown(5);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Context compaction failed, entering cooldown"
                        );
                        token_estimator.start_compact_cooldown(3);
                    }
                }
            }

            let request_messages = sanitize_tool_call_history(state.messages.clone());
            let mut request =
                CompletionRequest::new(request_messages).with_tools(self.tools.schemas());

            // Only set temperature if explicitly configured (some models don't support it)
            if let Some(temp) = config.temperature {
                request = request.with_temperature(temp);
            }
            if let Some(max_tokens) = config.max_output_tokens {
                request = request.with_max_tokens(max_tokens);
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
                // Calibrate token estimator with actual prompt tokens
                if usage.prompt_tokens > 0 {
                    let est = context_manager::estimate_tokens(&state.messages);
                    token_estimator.calibrate(est, usage.prompt_tokens);
                }
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
                state.add_message(assistant_msg);
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
            state.add_message(tool_call_msg);

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
                    &response.tool_calls,
                    emitter,
                    config.tool_timeout,
                    config.yolo_mode,
                    config.max_tool_concurrency,
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

                // Truncate long results with middle-truncation and disk persistence
                let tool_name_for_truncate = tool_call
                    .map(|tc| tc.name.as_str())
                    .unwrap_or("unknown");
                result_str = truncate_tool_output(
                    &result_str,
                    config.max_tool_result_length,
                    config.scratchpad.as_deref(),
                    &tool_call_id,
                    tool_name_for_truncate,
                );

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

                // Add tool result to state
                let tool_result_msg = Message::tool_result(tool_call_id.clone(), result_str);
                state.add_message(tool_result_msg);
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
                        state.add_message(nudge_msg);
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

        // Context management: prune old tool results for checkpoint/resume
        let prune_stats = context_manager::prune(&mut state.messages, &context_config);
        if prune_stats.applied {
            tracing::info!(
                messages_truncated = prune_stats.messages_truncated,
                tokens_saved = prune_stats.tokens_saved,
                "Post-loop context pruned"
            );
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
            resource_usage,
        })
    }

    async fn execute_tools_with_events(
        &self,
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        tool_timeout: Duration,
        yolo_mode: bool,
        max_tool_concurrency: usize,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        self.execute_tools_parallel(tool_calls, emitter, tool_timeout, yolo_mode, max_tool_concurrency)
            .await
    }

    async fn execute_tool_call(
        &self,
        name: &str,
        args: Value,
        yolo_mode: bool,
    ) -> Result<crate::tools::ToolOutput> {
        let mut retry_count = 0usize;

        loop {
            let output = self
                .execute_tool_call_once(name, args.clone(), yolo_mode)
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
        self.tools.execute_safe(name, args).await.map_err(Into::into)
    }

    /// Execute a tool with retry logic and timeout.
    /// Static version that accepts `Arc<ToolRegistry>` for use inside `tokio::spawn`.
    async fn execute_tool_with_retry(
        tools: Arc<ToolRegistry>,
        name: String,
        mut args: Value,
        tool_timeout: Duration,
        yolo_mode: bool,
    ) -> Result<crate::tools::ToolOutput> {
        if yolo_mode
            && name == "bash"
            && let Some(map) = args.as_object_mut()
        {
            map.insert("yolo_mode".to_string(), Value::Bool(true));
        }

        let mut retry_count = 0usize;
        loop {
            let output = tokio::time::timeout(
                tool_timeout,
                tools.execute_safe(&name, args.clone()),
            )
            .await
            .map_err(|_| AiError::Tool(format!("Tool {} timed out", name)))
            .and_then(|r| r.map_err(Into::into))?;

            if output.success {
                return Ok(output);
            }

            if output
                .result
                .get("pending_approval")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
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

    async fn execute_tools_parallel(
        &self,
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        tool_timeout: Duration,
        yolo_mode: bool,
        max_concurrency: usize,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        // 1. Emit start events for all tool calls upfront
        for call in tool_calls {
            let arguments = serde_json::to_string(&call.arguments).unwrap_or_default();
            emitter
                .emit_tool_call_start(&call.id, &call.name, &arguments)
                .await;
        }

        // 2. Spawn each tool as an independent Tokio task with semaphore-bounded concurrency
        let semaphore = Arc::new(Semaphore::new(max_concurrency));
        let mut ordered = FuturesOrdered::new();

        for call in tool_calls {
            let tools = Arc::clone(&self.tools);
            let sem = Arc::clone(&semaphore);
            let name = call.name.clone();
            let args = call.arguments.clone();
            let tool_call_id = call.id.clone();
            let tool_name = call.name.clone();

            let handle: JoinHandle<Result<crate::tools::ToolOutput>> = tokio::spawn(
                async move {
                    let _permit = sem.acquire().await.map_err(|_| {
                        AiError::Tool("Tool concurrency semaphore closed".to_string())
                    })?;
                    Self::execute_tool_with_retry(tools, name, args, tool_timeout, yolo_mode).await
                },
            );

            // Capture abort handle for cancellation support
            self.active_tool_calls
                .insert(tool_call_id.clone(), handle.abort_handle());

            ordered.push_back(async move {
                let result = match handle.await {
                    Ok(r) => r,
                    Err(e) if e.is_cancelled() => {
                        Err(AiError::Tool("Tool call cancelled".to_string()))
                    }
                    Err(e) => Err(AiError::Tool(format!("Tool task panicked: {}", e))),
                };
                (tool_call_id, tool_name, result)
            });
        }

        // 3. Drain results in submission order, emitting events as each completes.
        //    Between each result, check for cancellation steer commands.
        let mut output = Vec::with_capacity(tool_calls.len());
        while let Some((id, name, result)) = ordered.next().await {
            // Remove from active set now that it has completed
            self.active_tool_calls.remove(&id);

            let (result_str, success) = match &result {
                Ok(o) if o.success => (
                    serde_json::to_string(&o.result).unwrap_or_default(),
                    true,
                ),
                Ok(o) => (
                    format!("Error: {}", o.error.clone().unwrap_or_default()),
                    false,
                ),
                Err(error) => (format!("Error: {}", error), false),
            };
            emitter
                .emit_tool_call_result(&id, &name, &result_str, success)
                .await;
            output.push((id, result));

            // Process any pending cancellation steer commands between tool completions
            self.process_cancel_steers().await;
        }

        // Clear any remaining entries (shouldn't happen, but defensive)
        self.active_tool_calls.clear();

        output
    }

    /// Process only CancelToolCall steer commands (non-blocking).
    /// Message and Interrupt variants are buffered for `apply_steer_messages()`.
    async fn process_cancel_steers(&self) {
        let Some(rx) = &self.steer_rx else {
            return;
        };

        let mut rx = rx.lock().await;
        let mut deferred = Vec::new();
        while let Ok(steer) = rx.try_recv() {
            match &steer.command {
                crate::steer::SteerCommand::CancelToolCall { tool_call_id } => {
                    if let Some((_, abort_handle)) =
                        self.active_tool_calls.remove(tool_call_id)
                    {
                        abort_handle.abort();
                        tracing::info!(
                            tool_call_id = %tool_call_id,
                            "Tool call cancelled via steer (during tool drain)"
                        );
                    }
                }
                _ => deferred.push(steer),
            }
        }
        drop(rx);

        // Buffer non-cancel messages for the next apply_steer_messages() call
        if !deferred.is_empty() {
            let mut buffer = self.steer_buffer.lock().await;
            buffer.extend(deferred);
        }
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

fn sanitize_tool_call_history(messages: Vec<Message>) -> Vec<Message> {
    use std::collections::HashSet;

    let mut assistant_ids: HashSet<String> = HashSet::new();
    let mut tool_result_ids: HashSet<String> = HashSet::new();

    for msg in &messages {
        if let Some(tool_calls) = &msg.tool_calls {
            for call in tool_calls {
                assistant_ids.insert(call.id.clone());
            }
        }
        if matches!(msg.role, Role::Tool)
            && let Some(id) = &msg.tool_call_id
        {
            tool_result_ids.insert(id.clone());
        }
    }

    let valid_ids: HashSet<String> = assistant_ids
        .intersection(&tool_result_ids)
        .cloned()
        .collect();

    let mut sanitized = Vec::with_capacity(messages.len());
    for mut msg in messages {
        if let Some(tool_calls) = msg.tool_calls.take() {
            let filtered: Vec<ToolCall> = tool_calls
                .into_iter()
                .filter(|call| valid_ids.contains(&call.id))
                .collect();
            if !filtered.is_empty() {
                msg.tool_calls = Some(filtered);
                sanitized.push(msg);
            } else if !msg.content.trim().is_empty() {
                msg.tool_calls = None;
                sanitized.push(msg);
            }
            continue;
        }

        if matches!(msg.role, Role::Tool) {
            match msg.tool_call_id.as_ref() {
                Some(id) if valid_ids.contains(id) => sanitized.push(msg),
                Some(_) => {}
                None => sanitized.push(msg),
            }
            continue;
        }

        sanitized.push(msg);
    }

    sanitized
}

#[cfg(test)]
mod tests;
