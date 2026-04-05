use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::FuturesOrdered;
use restflow_telemetry::{
    ExecutionEvent, ExecutionEventEnvelope, TelemetryContext, TelemetrySink,
    ToolCallCompletedPayload, ToolCallStartedPayload,
};
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use restflow_traits::store::is_task_management_tool_name;

use crate::agent::stream::StreamEmitter;
use crate::error::{AiError, Result};
use crate::llm::ToolCall;
use crate::tools::{ToolErrorCategory, ToolRegistry};

use super::{AgentExecutor, MAX_TOOL_RETRIES};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ToolInvocationContext<'a> {
    /// Legacy field name kept for compatibility with existing runtime call sites.
    pub parent_execution_id: Option<&'a str>,
    pub chat_session_id: Option<&'a str>,
    pub trace_session_id: Option<&'a str>,
    pub trace_scope_id: Option<&'a str>,
}

impl<'a> ToolInvocationContext<'a> {
    fn parent_run_id(self) -> Option<&'a str> {
        self.parent_execution_id
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ToolExecutionOptions<'a> {
    pub tool_timeout: Duration,
    pub yolo_mode: bool,
    pub max_concurrency: usize,
    pub telemetry_sink: Option<&'a Arc<dyn TelemetrySink>>,
    pub telemetry_context: Option<&'a TelemetryContext>,
    pub invocation: ToolInvocationContext<'a>,
}

impl AgentExecutor {
    fn is_subagent_spawn_tool(tool_name: &str) -> bool {
        tool_name == "spawn_subagent" || tool_name == "spawn_subagent_batch"
    }

    fn inject_spawn_parent_run_id(tool_name: &str, args: &mut Value, parent_run_id: Option<&str>) {
        if !Self::is_subagent_spawn_tool(tool_name) {
            return;
        }
        let Some(parent_run_id) = parent_run_id else {
            return;
        };
        if let Some(map) = args.as_object_mut() {
            map.remove("parent_execution_id");
            map.insert(
                "parent_run_id".to_string(),
                Value::String(parent_run_id.to_string()),
            );
        }
    }

    fn inject_spawn_trace_context(
        tool_name: &str,
        args: &mut Value,
        trace_session_id: Option<&str>,
        trace_scope_id: Option<&str>,
    ) {
        if !Self::is_subagent_spawn_tool(tool_name) {
            return;
        }
        let Some(map) = args.as_object_mut() else {
            return;
        };
        if let Some(trace_session_id) = trace_session_id {
            map.insert(
                "trace_session_id".to_string(),
                Value::String(trace_session_id.to_string()),
            );
        }
        if let Some(trace_scope_id) = trace_scope_id {
            map.insert(
                "trace_scope_id".to_string(),
                Value::String(trace_scope_id.to_string()),
            );
        }
    }

    fn inject_promote_session_id(tool_name: &str, args: &mut Value, chat_session_id: Option<&str>) {
        if !is_task_management_tool_name(tool_name) {
            return;
        }
        let Some(chat_session_id) = chat_session_id else {
            return;
        };
        let Some(map) = args.as_object_mut() else {
            return;
        };
        let operation = map
            .get("operation")
            .and_then(Value::as_str)
            .map(|value| value.trim().to_ascii_lowercase());
        if operation.as_deref() != Some("promote_to_background") {
            return;
        }
        map.insert(
            "session_id".to_string(),
            Value::String(chat_session_id.to_string()),
        );
    }

    fn inject_subagent_parent_scope(
        tool_name: &str,
        args: &mut Value,
        parent_run_id: Option<&str>,
    ) {
        if tool_name != "list_subagents" && tool_name != "wait_subagents" {
            return;
        }
        let Some(parent_run_id) = parent_run_id else {
            return;
        };
        let Some(map) = args.as_object_mut() else {
            return;
        };
        map.insert(
            "parent_run_id".to_string(),
            Value::String(parent_run_id.to_string()),
        );
    }

    pub(crate) async fn execute_tools_with_events(
        &self,
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        options: ToolExecutionOptions<'_>,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        self.execute_tools_parallel(tool_calls, emitter, options)
            .await
    }

    pub(crate) async fn execute_tool_call(
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
        self.tools
            .execute_safe(name, args)
            .await
            .map_err(Into::into)
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
            let output =
                tokio::time::timeout(tool_timeout, tools.execute_safe(&name, args.clone()))
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

    pub(crate) async fn execute_tools_parallel(
        &self,
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
        options: ToolExecutionOptions<'_>,
    ) -> Vec<(String, Result<crate::tools::ToolOutput>)> {
        // TODO(ToolSearch): Currently all tool calls run in parallel with a semaphore.
        // Should partition into batches using Tool::is_concurrency_safe() / is_read_only():
        //   1. Batch consecutive read-only tools → run concurrently (current behavior)
        //   2. Batch non-read-only tools → run serially (preserves ordering, avoids conflicts)
        // See Claude Code's partitionToolCalls() in src/services/tools/toolOrchestration.ts:91
        let ToolExecutionOptions {
            tool_timeout,
            yolo_mode,
            max_concurrency,
            telemetry_sink,
            telemetry_context,
            invocation: context,
        } = options;

        // 1. Emit start events for all tool calls upfront
        for call in tool_calls {
            let mut args = call.arguments.clone();
            Self::inject_spawn_parent_run_id(&call.name, &mut args, context.parent_run_id());
            Self::inject_spawn_trace_context(
                &call.name,
                &mut args,
                context.trace_session_id,
                context.trace_scope_id,
            );
            Self::inject_promote_session_id(&call.name, &mut args, context.chat_session_id);
            Self::inject_subagent_parent_scope(&call.name, &mut args, context.parent_run_id());
            let arguments = serde_json::to_string(&args).unwrap_or_default();
            emitter
                .emit_tool_call_start(&call.id, &call.name, &arguments)
                .await;
            if let (Some(telemetry_sink), Some(telemetry_context)) =
                (telemetry_sink, telemetry_context)
            {
                telemetry_sink
                    .emit(ExecutionEventEnvelope::from_telemetry_context(
                        telemetry_context,
                        ExecutionEvent::ToolCallStarted(ToolCallStartedPayload {
                            tool_call_id: call.id.clone(),
                            tool_name: call.name.clone(),
                            input: Some(arguments.clone()),
                        }),
                    ))
                    .await;
            }
        }

        // 2. Spawn each tool as an independent Tokio task with semaphore-bounded concurrency
        let semaphore = Arc::new(Semaphore::new(max_concurrency));
        let mut ordered = FuturesOrdered::new();

        for call in tool_calls {
            let tools = Arc::clone(&self.tools);
            let sem = Arc::clone(&semaphore);
            let name = call.name.clone();
            let mut args = call.arguments.clone();
            Self::inject_spawn_parent_run_id(&call.name, &mut args, context.parent_run_id());
            Self::inject_spawn_trace_context(
                &call.name,
                &mut args,
                context.trace_session_id,
                context.trace_scope_id,
            );
            Self::inject_promote_session_id(&call.name, &mut args, context.chat_session_id);
            Self::inject_subagent_parent_scope(&call.name, &mut args, context.parent_run_id());
            let tool_call_id = call.id.clone();
            let tool_name = call.name.clone();

            let handle: JoinHandle<Result<crate::tools::ToolOutput>> = tokio::spawn(async move {
                let _permit = sem
                    .acquire()
                    .await
                    .map_err(|_| AiError::Tool("Tool concurrency semaphore closed".to_string()))?;
                Self::execute_tool_with_retry(tools, name, args, tool_timeout, yolo_mode).await
            });

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
                Ok(o) if o.success => (serde_json::to_string(&o.result).unwrap_or_default(), true),
                Ok(o) => (
                    format!("Error: {}", o.error.clone().unwrap_or_default()),
                    false,
                ),
                Err(error) => (format!("Error: {}", error), false),
            };
            emitter
                .emit_tool_call_result(&id, &name, &result_str, success)
                .await;
            if let (Some(telemetry_sink), Some(telemetry_context)) =
                (telemetry_sink, telemetry_context)
            {
                let error = if success {
                    None
                } else {
                    Some(result_str.clone())
                };
                telemetry_sink
                    .emit(ExecutionEventEnvelope::from_telemetry_context(
                        telemetry_context,
                        ExecutionEvent::ToolCallCompleted(ToolCallCompletedPayload {
                            tool_call_id: id.clone(),
                            tool_name: name.clone(),
                            input_summary: None,
                            output: Some(result_str.clone()),
                            output_ref: None,
                            success,
                            duration_ms: None,
                            error,
                        }),
                    ))
                    .await;
            }
            output.push((id, result));

            // Process any pending cancellation steer commands between tool completions
            self.process_cancel_steers().await;
        }

        // Clear any remaining entries (shouldn't happen, but defensive)
        self.active_tool_calls.clear();

        output
    }
}
