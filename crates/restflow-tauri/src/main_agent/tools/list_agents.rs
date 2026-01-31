//! list_agents tool - List available agent types and running agents.

use crate::main_agent::MainAgent;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use ts_rs::TS;

/// Parameters for list_agents tool
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListAgentsParams {
    /// Include currently running agents in the response
    #[serde(default = "default_include_running")]
    pub include_running: bool,
}

fn default_include_running() -> bool {
    true
}

/// list_agents tool for the main agent
pub struct ListAgentsTool {
    main_agent: Arc<MainAgent>,
}

impl ListAgentsTool {
    /// Create a new list_agents tool
    pub fn new(main_agent: Arc<MainAgent>) -> Self {
        Self { main_agent }
    }

    /// Get tool name
    pub fn name(&self) -> &str {
        "list_agents"
    }

    /// Get tool description
    pub fn description(&self) -> &str {
        "List available agent types and currently running agents."
    }

    /// Get JSON schema for parameters
    pub fn parameters_schema(&self) -> Value {
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

    /// Execute the tool
    pub async fn execute(&self, input: Value) -> Result<Value> {
        let params: ListAgentsParams =
            serde_json::from_value(input).map_err(|e| anyhow!("Invalid parameters: {}", e))?;

        // Get available agent definitions
        let definitions = self.main_agent.agent_definitions();
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
            let tracker = self.main_agent.running_subagents();
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

        Ok(response)
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
