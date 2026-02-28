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
mod steer;
mod streaming;
mod tool_exec;
pub use config::*;

use std::sync::Arc;
use std::time::Duration;
use std::{fs, path::Path};

use serde_json::Value;

use crate::agent::context::{ContextDiscoveryConfig, WorkspaceContextCache};
use crate::agent::context_manager::{self, ContextManagerConfig, TokenEstimator};
use crate::agent::deferred::DeferredExecutionManager;
use crate::agent::model_router::{classify_task, select_model};
use crate::agent::resource::ResourceTracker;
use crate::agent::state::{AgentState, AgentStatus};
use crate::agent::stream::{NullEmitter, StreamEmitter};
use crate::agent::streaming_buffer::StreamingBuffer;
use crate::agent::stuck::{StuckAction, StuckDetector};
use crate::agent::sub_agent::SubagentTracker;
use crate::error::Result;
use crate::llm::{CompletionRequest, FinishReason, LlmClient, Message, Role, ToolCall};
use crate::steer::SteerMessage;
use crate::tools::ToolRegistry;
use dashmap::DashMap;
use tokio::sync::{Mutex, mpsc};
use tokio::task::AbortHandle;
use tracing::debug;

const USER_INSTRUCTIONS_PREFIX: &str = "# AGENTS.md instructions for ";

/// Truncate tool output with middle-truncation and optional disk persistence.
/// Returns the (possibly truncated) string with a retrieval hint if the full output was saved.
fn save_tool_output(
    output_dir: &Path,
    call_id: &str,
    tool_name: &str,
    content: &str,
) -> Option<std::path::PathBuf> {
    if fs::create_dir_all(output_dir).is_err() {
        return None;
    }

    let safe_name: String = tool_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let filename = format!("{safe_name}-{call_id}.txt");
    let path = output_dir.join(filename);
    match fs::write(&path, content) {
        Ok(()) => Some(path),
        Err(_) => None,
    }
}

fn truncate_tool_output(
    content: &str,
    max_len: usize,
    tool_output_dir: Option<&Path>,
    call_id: &str,
    tool_name: &str,
) -> String {
    if content.len() <= max_len {
        return content.to_string();
    }

    let total_len = content.len();

    // Save full output to disk before truncating
    let saved_path =
        tool_output_dir.and_then(|dir| save_tool_output(dir, call_id, tool_name, content));

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

    async fn build_workspace_instruction_user_message(&self) -> Option<String> {
        let cache = self.context_cache.as_ref()?;
        let context = cache.get().await;
        let instructions = context.content.trim();
        if instructions.is_empty() {
            return None;
        }

        debug!(
            files = ?context.loaded_files,
            bytes = context.total_bytes,
            "Loaded workspace instructions for user-role injection"
        );

        let directory = std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string());

        Some(format!(
            "{USER_INSTRUCTIONS_PREFIX}{directory}\n\n<INSTRUCTIONS>\n{instructions}\n</INSTRUCTIONS>"
        ))
    }

    fn has_workspace_instruction_message(state: &AgentState) -> bool {
        state.messages.iter().any(|message| {
            message.role == Role::User
                && message.content.starts_with(USER_INSTRUCTIONS_PREFIX)
                && message.content.contains("<INSTRUCTIONS>")
        })
    }

    fn inject_workspace_instruction_message(state: &mut AgentState, message: String) {
        if Self::has_workspace_instruction_message(state) {
            return;
        }

        let insert_index = if matches!(state.messages.first().map(|m| &m.role), Some(Role::System))
        {
            1
        } else {
            0
        };
        state.messages.insert(insert_index, Message::user(message));
        state.version += 1;
    }

    /// Execute agent - simplified Swarm-style loop
    pub async fn run(&self, config: AgentConfig) -> Result<AgentResult> {
        let mut emitter = NullEmitter;
        self.execute_with_mode(config, &mut emitter, false, None, None)
            .await
    }

    /// Execute agent in non-stream mode while still emitting execution events.
    ///
    /// This preserves non-streaming LLM behavior and is intended for runtimes that
    /// require tool call traces even when token streaming is disabled.
    pub async fn run_with_emitter(
        &self,
        config: AgentConfig,
        emitter: &mut dyn StreamEmitter,
    ) -> Result<AgentResult> {
        self.execute_with_mode(config, emitter, false, None, None)
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

    /// Resume execution from an existing state snapshot in non-stream mode while
    /// emitting execution events.
    pub async fn run_from_state_with_emitter(
        &self,
        config: AgentConfig,
        mut state: AgentState,
        emitter: &mut dyn StreamEmitter,
    ) -> Result<AgentResult> {
        state.status = AgentStatus::Running;
        state.ended_at = None;
        let execution_id = state.execution_id.clone();
        self.execute_with_mode(config, emitter, false, Some(execution_id), Some(state))
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
        state.max_iterations = config.max_iterations;
        state.context.extend(config.context.clone());
        let mut total_tokens: u32 = 0;
        let mut total_cost_usd: f64 = 0.0;
        let tracker = ResourceTracker::new(config.resource_limits.clone());
        let context_config =
            ContextManagerConfig::default().with_context_window(config.context_window);
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
        if config.prompt_flags.include_workspace_context
            && let Some(message) = self.build_workspace_instruction_user_message().await
        {
            Self::inject_workspace_instruction_message(&mut state, message);
        }

        // Core loop (Swarm-inspired simplicity)
        while state.iteration < state.max_iterations && !state.is_terminal() {
            self.apply_steer_messages(&mut state, &deferred_manager)
                .await;
            self.poll_subagent_completions(&mut state, config.max_tool_result_length)
                .await;
            self.process_resolved_deferred_calls(
                &deferred_manager,
                &mut state,
                config.tool_timeout,
                config.max_tool_result_length,
                config.tool_output_dir.as_deref(),
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
                let assistant_msg = Message::assistant(&answer);
                state.add_message(assistant_msg);
                last_tool_names.clear();

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
                break;
            }

            // 3. Execute tools with timeout and optional stream events.
            let results = self
                .execute_tools_with_events(
                    &response.tool_calls,
                    emitter,
                    config.tool_timeout,
                    config.yolo_mode,
                    config.max_tool_concurrency,
                    Some(state.execution_id.as_str()),
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
                let tool_name_for_truncate =
                    tool_call.map(|tc| tc.name.as_str()).unwrap_or("unknown");
                result_str = truncate_tool_output(
                    &result_str,
                    config.max_tool_result_length,
                    config.tool_output_dir.as_deref(),
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
