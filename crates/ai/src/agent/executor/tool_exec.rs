use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::FuturesOrdered;
use serde_json::Value;
use serde_json::json;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use types::ToolOutput;

use crate::agent::reviewer::{ToolCallReviewer, ToolReviewRequest};
use crate::agent::stream::StreamEmitter;
use crate::error::{AiError, Result};
use crate::llm::{Message, ToolCall};
use crate::tools::{ToolErrorCategory, ToolRegistry};

use super::{AgentExecutor, MAX_TOOL_RETRIES};

fn non_dynamic_text(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() || matches!(value, "dynamic" | "swappable") {
        None
    } else {
        Some(value)
    }
}

fn spec_needs_default_model(value: &Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    let has_agent = map
        .get("agent")
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_model = map
        .get("model")
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_provider = map
        .get("provider")
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    !has_agent && !has_model && !has_provider
}

fn serialize_tool_output_for_emitter(output: &ToolOutput) -> String {
    let mut value = output.result.clone();
    if !output.success
        && let Some(error) = output
            .error
            .as_deref()
            .filter(|error| !error.trim().is_empty())
    {
        match &mut value {
            Value::Object(map) => {
                map.entry("error".to_string())
                    .or_insert_with(|| Value::String(error.to_string()));
            }
            _ => {
                value = json!({
                    "error": error,
                    "result": value,
                });
            }
        }
    }
    serde_json::to_string(&value).unwrap_or_else(|_| output.error.clone().unwrap_or_default())
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ToolInvocationContext<'a> {
    pub parent_run_id: Option<&'a str>,
    pub model: Option<&'a str>,
    pub provider: Option<&'a str>,
}

impl<'a> ToolInvocationContext<'a> {
    fn parent_run_id(self) -> Option<&'a str> {
        self.parent_run_id
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ToolExecutionOptions<'a> {
    pub tool_timeout: Duration,
    pub yolo_mode: bool,
    pub max_concurrency: usize,
    pub invocation: ToolInvocationContext<'a>,
    pub reviewer: Option<&'a Arc<dyn ToolCallReviewer>>,
    pub review_messages: &'a [Message],
}

impl AgentExecutor {
    fn is_subagent_spawn_tool(tool_name: &str) -> bool {
        tool_name == "spawn_subagent" || tool_name == "spawn_subagent_batch"
    }

    fn uses_runtime_policy(tool_name: &str) -> bool {
        Self::is_subagent_spawn_tool(tool_name)
            || matches!(tool_name, "wait_subagents" | "list_subagents")
    }

    fn inject_spawn_parent_run_id(tool_name: &str, args: &mut Value, parent_run_id: Option<&str>) {
        if !Self::is_subagent_spawn_tool(tool_name) {
            return;
        }
        let Some(parent_run_id) = parent_run_id else {
            return;
        };
        if let Some(map) = args.as_object_mut() {
            map.remove("parent_run_id");
            map.insert(
                "parent_run_id".to_string(),
                Value::String(parent_run_id.to_string()),
            );
        }
    }

    fn inject_spawn_model_provider(
        tool_name: &str,
        args: &mut Value,
        model: Option<&str>,
        provider: Option<&str>,
    ) {
        if !Self::is_subagent_spawn_tool(tool_name) {
            return;
        }
        let (Some(model), Some(provider)) = (non_dynamic_text(model), non_dynamic_text(provider))
        else {
            return;
        };
        let Some(map) = args.as_object_mut() else {
            return;
        };

        if tool_name == "spawn_subagent_batch" {
            let Some(specs) = map.get_mut("specs").and_then(Value::as_array_mut) else {
                return;
            };
            for spec in specs {
                if spec_needs_default_model(spec)
                    && let Some(spec_map) = spec.as_object_mut()
                {
                    spec_map.insert("model".to_string(), Value::String(model.to_string()));
                    spec_map.insert("provider".to_string(), Value::String(provider.to_string()));
                }
            }
            return;
        }

        let value = Value::Object(map.clone());
        if spec_needs_default_model(&value) {
            map.insert("model".to_string(), Value::String(model.to_string()));
            map.insert("provider".to_string(), Value::String(provider.to_string()));
        }
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

    fn reviewer_denied_output(reason: Option<String>) -> crate::tools::ToolOutput {
        let message = reason
            .filter(|reason| !reason.trim().is_empty())
            .unwrap_or_else(|| "Operation denied by reviewer.".to_string());
        crate::tools::ToolOutput {
            success: false,
            result: json!({
                "review_denied": true,
                "reason": message,
            }),
            error: Some(format!("Operation denied by reviewer: {message}")),
            error_category: Some(ToolErrorCategory::Auth),
            retryable: Some(false),
            retry_after_ms: None,
        }
    }

    fn reviewer_failed_output(error: impl ToString) -> crate::tools::ToolOutput {
        let message = error.to_string();
        crate::tools::ToolOutput {
            success: false,
            result: json!({
                "review_failed": true,
                "reason": message,
            }),
            error: Some(format!("Operation review failed closed: {message}")),
            error_category: Some(ToolErrorCategory::Auth),
            retryable: Some(false),
            retry_after_ms: None,
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
            invocation: context,
            reviewer,
            review_messages,
        } = options;
        let reviewer = reviewer.cloned();

        // 1. Emit start events for all tool calls upfront
        for call in tool_calls {
            let mut args = call.arguments.clone();
            Self::inject_spawn_parent_run_id(&call.name, &mut args, context.parent_run_id());
            Self::inject_spawn_model_provider(
                &call.name,
                &mut args,
                context.model,
                context.provider,
            );
            Self::inject_subagent_parent_scope(&call.name, &mut args, context.parent_run_id());
            let arguments = serde_json::to_string(&args).unwrap_or_default();
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
            let mut args = call.arguments.clone();
            Self::inject_spawn_parent_run_id(&call.name, &mut args, context.parent_run_id());
            Self::inject_spawn_model_provider(
                &call.name,
                &mut args,
                context.model,
                context.provider,
            );
            Self::inject_subagent_parent_scope(&call.name, &mut args, context.parent_run_id());
            let tool_call_id = call.id.clone();
            let tool_name = call.name.clone();
            let reviewer = reviewer.clone();
            let review_messages = review_messages.to_vec();
            let review_call = ToolCall {
                id: tool_call_id.clone(),
                name: name.clone(),
                arguments: args.clone(),
            };

            let handle: JoinHandle<Result<crate::tools::ToolOutput>> = tokio::spawn(async move {
                let _permit = sem
                    .acquire()
                    .await
                    .map_err(|_| AiError::Tool("Tool concurrency semaphore closed".to_string()))?;
                if let Some(reviewer) = reviewer
                    && !Self::uses_runtime_policy(&name)
                {
                    match reviewer
                        .review_tool_call(ToolReviewRequest {
                            messages: review_messages,
                            tool_call: review_call,
                        })
                        .await
                    {
                        Ok(outcome) if outcome.is_allowed() => {}
                        Ok(outcome) => return Ok(Self::reviewer_denied_output(outcome.reason)),
                        Err(error) => return Ok(Self::reviewer_failed_output(error)),
                    }
                }
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
                Ok(output) => (serialize_tool_output_for_emitter(output), output.success),
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
