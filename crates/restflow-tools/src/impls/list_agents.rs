//! list_agents tool - List available agent types and running agents.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::error::{Result, ToolError};
use crate::tool::{Tool, ToolOutput};
use restflow_ai::agent::SubagentDeps;

/// Parameters for list_agents tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: ListAgentsParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

        let available: Vec<Value> = self
            .deps
            .definitions
            .list_callable()
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

        let mut response = json!({ "available_agents": available });

        if params.include_running {
            let running: Vec<Value> = self
                .deps
                .tracker
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
            response["running_count"] = json!(self.deps.tracker.running_count());
        }

        Ok(ToolOutput::success(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_default() {
        let params: ListAgentsParams = serde_json::from_str("{}").unwrap();
        assert!(params.include_running);
    }

    #[test]
    fn test_params_no_running() {
        let params: ListAgentsParams =
            serde_json::from_str(r#"{"include_running": false}"#).unwrap();
        assert!(!params.include_running);
    }
}
