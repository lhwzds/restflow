//! Batch tool — execute up to 25 tool calls in a single invocation.
//!
//! Allows the LLM to batch multiple independent tool calls into one round-trip,
//! avoiding the overhead of 25 separate LLM calls. Each sub-invocation runs in
//! parallel with bounded concurrency.

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::FuturesOrdered;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::ToolRegistry;
use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};

/// Maximum number of sub-invocations per batch call.
const MAX_BATCH_SIZE: usize = 25;

/// A single tool invocation within a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInvocation {
    /// Name of the tool to invoke.
    pub tool: String,
    /// Input arguments for the tool.
    pub input: Value,
}

/// Parameters for the batch tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchParams {
    /// Array of tool invocations (max 25).
    pub invocations: Vec<BatchInvocation>,
    /// Continue executing remaining invocations if one fails (default: true).
    #[serde(default = "default_continue_on_error")]
    pub continue_on_error: bool,
    /// Per-invocation timeout in seconds (default: 300).
    pub timeout_secs: Option<u64>,
}

fn default_continue_on_error() -> bool {
    true
}

/// Batch tool that executes multiple tool calls in parallel.
pub struct BatchTool {
    tools: Arc<ToolRegistry>,
}

impl BatchTool {
    /// Create a new batch tool backed by the given tool registry.
    pub fn new(tools: Arc<ToolRegistry>) -> Self {
        Self { tools }
    }
}

#[async_trait]
impl Tool for BatchTool {
    fn name(&self) -> &str {
        "batch"
    }

    fn description(&self) -> &str {
        "Execute up to 25 tool calls in a single invocation. Each sub-call runs in parallel. \
         Use this to batch multiple independent operations and avoid round-trip overhead."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "invocations": {
                    "type": "array",
                    "description": "Array of tool invocations to execute in parallel (max 25)",
                    "maxItems": MAX_BATCH_SIZE,
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": {
                                "type": "string",
                                "description": "Name of the tool to invoke"
                            },
                            "input": {
                                "type": "object",
                                "description": "Input arguments for the tool"
                            }
                        },
                        "required": ["tool", "input"]
                    }
                },
                "continue_on_error": {
                    "type": "boolean",
                    "default": true,
                    "description": "Continue executing remaining invocations if one fails (default: true)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional per-invocation timeout in seconds. If omitted, no timeout is applied (the executor's wrapper timeout still applies)."
                }
            },
            "required": ["invocations"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: BatchParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid batch parameters: {}", e)))?;

        if params.invocations.is_empty() {
            return Ok(ToolOutput::success(json!({
                "results": [],
                "summary": { "total": 0, "succeeded": 0, "failed": 0 }
            })));
        }

        if params.invocations.len() > MAX_BATCH_SIZE {
            return Err(ToolError::Tool(format!(
                "Batch size {} exceeds maximum of {}",
                params.invocations.len(),
                MAX_BATCH_SIZE
            )));
        }

        // Prevent recursion: reject if any invocation calls "batch"
        for inv in &params.invocations {
            if inv.tool == "batch" {
                return Err(ToolError::Tool(
                    "Recursive batch calls are not allowed".to_string(),
                ));
            }
        }

        let continue_on_error = params.continue_on_error;
        let timeout = params.timeout_secs.map(Duration::from_secs);
        let semaphore = Arc::new(Semaphore::new(MAX_BATCH_SIZE));
        let mut ordered = FuturesOrdered::new();

        for (idx, inv) in params.invocations.into_iter().enumerate() {
            let tools = Arc::clone(&self.tools);
            let sem = Arc::clone(&semaphore);
            let tool_name = inv.tool;
            let tool_input = inv.input;
            let tool_timeout = timeout;

            ordered.push_back(async move {
                let _permit = sem.acquire().await;
                let result = if let Some(t) = tool_timeout {
                    tokio::time::timeout(t, tools.execute_safe(&tool_name, tool_input))
                        .await
                        .unwrap_or_else(|_| {
                            Err(ToolError::Tool(format!("Tool '{}' timed out", tool_name)))
                        })
                } else {
                    tools.execute_safe(&tool_name, tool_input).await
                };
                (idx, tool_name, result)
            });
        }

        let mut results = Vec::new();
        let mut succeeded = 0usize;
        let mut failed = 0usize;

        while let Some((idx, tool_name, result)) = ordered.next().await {
            let entry = match result {
                Ok(output) if output.success => {
                    succeeded += 1;
                    json!({
                        "index": idx,
                        "tool": tool_name,
                        "success": true,
                        "output": output.result
                    })
                }
                Ok(output) => {
                    failed += 1;
                    json!({
                        "index": idx,
                        "tool": tool_name,
                        "success": false,
                        "error": output.error.unwrap_or_else(|| "unknown error".to_string())
                    })
                }
                Err(e) => {
                    failed += 1;
                    json!({
                        "index": idx,
                        "tool": tool_name,
                        "success": false,
                        "error": e.to_string()
                    })
                }
            };
            results.push(entry);

            if !continue_on_error && failed > 0 {
                // Mark remaining as skipped
                break;
            }
        }

        let total = succeeded + failed;
        Ok(ToolOutput::success(json!({
            "results": results,
            "summary": {
                "total": total,
                "succeeded": succeeded,
                "failed": failed
            }
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple echo tool for testing.
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echo input back"
        }
        fn parameters_schema(&self) -> Value {
            json!({"type": "object"})
        }
        async fn execute(&self, input: Value) -> Result<ToolOutput> {
            Ok(ToolOutput::success(input))
        }
    }

    /// A tool that always fails.
    struct FailTool;

    #[async_trait]
    impl Tool for FailTool {
        fn name(&self) -> &str {
            "fail"
        }
        fn description(&self) -> &str {
            "Always fails"
        }
        fn parameters_schema(&self) -> Value {
            json!({"type": "object"})
        }
        async fn execute(&self, _input: Value) -> Result<ToolOutput> {
            Ok(ToolOutput::error("intentional failure"))
        }
    }

    fn make_registry() -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);
        registry.register(FailTool);
        Arc::new(registry)
    }

    #[tokio::test]
    async fn test_batch_two_echo_tools() {
        let registry = make_registry();
        let batch = BatchTool::new(registry);
        let result = batch
            .execute(json!({
                "invocations": [
                    { "tool": "echo", "input": { "msg": "hello" } },
                    { "tool": "echo", "input": { "msg": "world" } }
                ]
            }))
            .await
            .unwrap();

        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0]["success"].as_bool().unwrap());
        assert!(results[1]["success"].as_bool().unwrap());
        assert_eq!(result.result["summary"]["succeeded"], 2);
        assert_eq!(result.result["summary"]["failed"], 0);
    }

    #[tokio::test]
    async fn test_batch_partial_failure_continue() {
        let registry = make_registry();
        let batch = BatchTool::new(registry);
        let result = batch
            .execute(json!({
                "invocations": [
                    { "tool": "echo", "input": { "msg": "ok" } },
                    { "tool": "fail", "input": {} },
                    { "tool": "echo", "input": { "msg": "also ok" } }
                ],
                "continue_on_error": true
            }))
            .await
            .unwrap();

        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0]["success"].as_bool().unwrap());
        assert!(!results[1]["success"].as_bool().unwrap());
        assert!(results[2]["success"].as_bool().unwrap());
        assert_eq!(result.result["summary"]["succeeded"], 2);
        assert_eq!(result.result["summary"]["failed"], 1);
    }

    #[tokio::test]
    async fn test_batch_exceeds_max_size() {
        let registry = make_registry();
        let batch = BatchTool::new(registry);
        let invocations: Vec<Value> = (0..26)
            .map(|i| json!({ "tool": "echo", "input": { "i": i } }))
            .collect();
        let result = batch.execute(json!({ "invocations": invocations })).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn test_batch_recursive_rejected() {
        let registry = make_registry();
        let batch = BatchTool::new(registry);
        let result = batch
            .execute(json!({
                "invocations": [
                    { "tool": "batch", "input": { "invocations": [] } }
                ]
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Recursive batch"));
    }

    #[tokio::test]
    async fn test_batch_empty() {
        let registry = make_registry();
        let batch = BatchTool::new(registry);
        let result = batch.execute(json!({ "invocations": [] })).await.unwrap();

        assert!(result.success);
        assert_eq!(result.result["summary"]["total"], 0);
    }
}
