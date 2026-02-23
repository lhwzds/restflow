use std::sync::Arc;
use std::time::Duration;

use futures::stream::FuturesOrdered;
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::agent::stream::StreamEmitter;
use crate::error::{AiError, Result};
use crate::llm::ToolCall;
use crate::tools::{ToolErrorCategory, ToolRegistry};

use super::{AgentExecutor, MAX_TOOL_RETRIES};

impl AgentExecutor {
    pub(crate) async fn execute_tools_with_events(
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

    pub(crate) async fn execute_tools_parallel(
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
}
