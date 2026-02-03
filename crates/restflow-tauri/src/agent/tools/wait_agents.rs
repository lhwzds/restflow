//! wait_agents tool - Wait for sub-agents to finish and return results.

use super::{SubagentDeps, Tool, ToolResult};
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::{Duration, timeout};
use ts_rs::TS;

/// Parameters for wait_agents tool.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WaitAgentsParams {
    /// Task IDs to wait for. If empty, waits for all running sub-agents.
    #[serde(default)]
    pub task_ids: Vec<String>,

    /// Timeout in seconds (default: 300).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// wait_agents tool for the unified agent.
pub struct WaitAgentsTool {
    deps: Arc<SubagentDeps>,
}

impl WaitAgentsTool {
    /// Create a new wait_agents tool.
    pub fn new(deps: Arc<SubagentDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl Tool for WaitAgentsTool {
    fn name(&self) -> &str {
        "wait_agents"
    }

    fn description(&self) -> &str {
        "Wait for one or more sub-agents to finish and return their results."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of sub-agent task IDs to wait for. If empty, waits for all running sub-agents."
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": 300,
                    "description": "Timeout in seconds (default: 300)"
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolResult> {
        let params: WaitAgentsParams = serde_json::from_value(input)
            .map_err(|e| AiError::Tool(format!("Invalid parameters: {}", e)))?;

        let wait_timeout = params
            .timeout_secs
            .unwrap_or(self.deps.config.subagent_timeout_secs);

        let task_ids = if params.task_ids.is_empty() {
            self.deps
                .tracker
                .running()
                .into_iter()
                .map(|state| state.id)
                .collect::<Vec<_>>()
        } else {
            params.task_ids
        };

        if task_ids.is_empty() {
            return Ok(ToolResult::success(json!({ "results": [] })));
        }

        let mut results = Vec::new();
        for task_id in task_ids {
            let result = match timeout(
                Duration::from_secs(wait_timeout),
                self.deps.tracker.wait(&task_id),
            )
            .await
            {
                Ok(Some(result)) => result,
                Ok(None) => {
                    results.push(json!({
                        "task_id": task_id,
                        "status": "not_found"
                    }));
                    continue;
                }
                Err(_) => {
                    results.push(json!({
                        "task_id": task_id,
                        "status": "timeout"
                    }));
                    continue;
                }
            };

            let entry = if result.success {
                json!({
                    "task_id": task_id,
                    "status": "completed",
                    "output": result.output,
                    "duration_ms": result.duration_ms
                })
            } else {
                json!({
                    "task_id": task_id,
                    "status": "failed",
                    "error": result.error.unwrap_or_else(|| "Unknown error".to_string()),
                    "duration_ms": result.duration_ms
                })
            };

            results.push(entry);
        }

        Ok(ToolResult::success(json!({ "results": results })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_default() {
        let json = r#"{}"#;
        let params: WaitAgentsParams = serde_json::from_str(json).unwrap();
        assert!(params.task_ids.is_empty());
        assert_eq!(params.timeout_secs, None);
    }

    #[test]
    fn test_params_deserialization() {
        let json = r#"{
            "task_ids": ["task-1", "task-2"],
            "timeout_secs": 120
        }"#;

        let params: WaitAgentsParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.task_ids.len(), 2);
        assert_eq!(params.timeout_secs, Some(120));
    }
}
