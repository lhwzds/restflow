//! wait_agents tool - Wait for sub-agents to complete.

use crate::main_agent::MainAgent;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::{Duration, timeout};
use ts_rs::TS;

/// Parameters for wait_agents tool
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WaitAgentsParams {
    /// List of task IDs to wait for. If empty, waits for all.
    #[serde(default)]
    pub task_ids: Vec<String>,

    /// Maximum time to wait in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    300
}

/// wait_agents tool for the main agent
pub struct WaitAgentsTool {
    main_agent: Arc<MainAgent>,
}

impl WaitAgentsTool {
    /// Create a new wait_agents tool
    pub fn new(main_agent: Arc<MainAgent>) -> Self {
        Self { main_agent }
    }

    /// Get tool name
    pub fn name(&self) -> &str {
        "wait_agents"
    }

    /// Get tool description
    pub fn description(&self) -> &str {
        "Wait for one or more background agents to complete. \
         Use this when you need results from previously spawned agents."
    }

    /// Get JSON schema for parameters
    pub fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of task IDs to wait for. If empty, waits for all."
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": 300,
                    "description": "Maximum time to wait"
                }
            }
        })
    }

    /// Execute the tool
    pub async fn execute(&self, input: Value) -> Result<Value> {
        let params: WaitAgentsParams =
            serde_json::from_value(input).map_err(|e| anyhow!("Invalid parameters: {}", e))?;

        let tracker = self.main_agent.running_subagents();

        let results = if params.task_ids.is_empty() {
            // Wait for all running sub-agents
            match timeout(Duration::from_secs(params.timeout_secs), tracker.wait_all()).await {
                Ok(results) => results,
                Err(_) => {
                    return Ok(json!({
                        "error": "Timeout waiting for sub-agents",
                        "completed": 0,
                        "results": []
                    }));
                }
            }
        } else {
            // Wait for specific sub-agents
            let mut results = Vec::new();
            for id in &params.task_ids {
                match timeout(Duration::from_secs(params.timeout_secs), tracker.wait(id)).await {
                    Ok(Some(result)) => results.push(result),
                    Ok(None) => {
                        // Sub-agent not found
                    }
                    Err(_) => {
                        // Timeout for this specific agent
                    }
                }
            }
            results
        };

        let output: Vec<Value> = results
            .iter()
            .map(|r| {
                json!({
                    "success": r.success,
                    "output": r.output,
                    "summary": r.summary,
                    "duration_ms": r.duration_ms,
                    "tokens_used": r.tokens_used,
                    "error": r.error
                })
            })
            .collect();

        Ok(json!({
            "completed": results.len(),
            "results": output
        }))
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
        assert_eq!(params.timeout_secs, 300);
    }

    #[test]
    fn test_params_with_ids() {
        let json = r#"{
            "task_ids": ["task-1", "task-2"],
            "timeout_secs": 600
        }"#;

        let params: WaitAgentsParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.task_ids.len(), 2);
        assert_eq!(params.timeout_secs, 600);
    }
}
