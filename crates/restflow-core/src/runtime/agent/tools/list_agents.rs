//! list_agents tool - List available agent types and running agents.

use super::{SubagentDeps, Tool, ToolResult};
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use ts_rs::TS;

/// Parameters for list_agents tool.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListAgentsParams {
    /// Include currently running agents in the response.
    #[serde(default = "default_include_running")]
    pub include_running: bool,
}

fn default_include_running() -> bool {
    true
}

/// list_agents tool for the shared agent execution engine.
pub struct ListAgentsTool {
    deps: Arc<SubagentDeps>,
}

impl ListAgentsTool {
    /// Create a new list_agents tool.
    pub fn new(deps: Arc<SubagentDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl Tool for ListAgentsTool {
    fn name(&self) -> &str {
        "list_agents"
    }

    fn description(&self) -> &str {
        "List available agent types and currently running agents."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "include_running": {
                    "type": "boolean",
                    "default": true,
                    "description": "Include currently running agents"
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolResult> {
        let params: ListAgentsParams = serde_json::from_value(input)
            .map_err(|e| AiError::Tool(format!("Invalid parameters: {}", e)))?;

        let definitions = &self.deps.definitions;
        let available: Vec<Value> = definitions
            .callable()
            .iter()
            .map(|def| {
                json!({
                    "id": def.id,
                    "name": def.name,
                    "description": def.description,
                    "tags": def.tags
                })
            })
            .collect();

        let mut response = json!({
            "available_agents": available
        });

        if params.include_running {
            let tracker = &self.deps.tracker;
            let running: Vec<Value> = tracker
                .running()
                .iter()
                .map(|state| {
                    json!({
                        "task_id": state.id,
                        "agent": state.agent_name,
                        "task": state.task,
                        "status": format!("{:?}", state.status),
                        "started_at": state.started_at
                    })
                })
                .collect();

            response["running_agents"] = json!(running);
            response["running_count"] = json!(tracker.running_count());
        }

        Ok(ToolResult::success(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_default() {
        let json = r#"{}"#;
        let params: ListAgentsParams = serde_json::from_str(json).unwrap();
        assert!(params.include_running);
    }

    #[test]
    fn test_params_no_running() {
        let json = r#"{"include_running": false}"#;
        let params: ListAgentsParams = serde_json::from_str(json).unwrap();
        assert!(!params.include_running);
    }
}
