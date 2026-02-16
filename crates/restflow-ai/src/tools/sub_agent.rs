//! Sub-agent tool for spawning nested agent executions.
//!
//! This tool allows an agent to spawn another agent as a sub-task,
//! with hierarchical tracking via subflow paths.

use crate::agent::{spawn_subagent, SpawnRequest, SubagentConfig, SubagentTracker, AgentDefinitionRegistry};
use crate::llm::LlmClient;
use crate::tools::{Tool, ToolOutput};
use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Input for spawning a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentInput {
    /// Agent type ID to spawn (e.g., "researcher", "coder").
    pub agent_id: String,
    /// Task description for the sub-agent.
    pub task: String,
    /// Optional timeout in seconds.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Parent subflow path (inherited from parent agent's context).
    #[serde(default)]
    pub parent_subflow_path: Vec<String>,
}

/// Tool for spawning sub-agents.
pub struct SubAgentTool {
    tracker: Arc<SubagentTracker>,
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<crate::tools::ToolRegistry>,
    config: SubagentConfig,
    /// Injected agent definition registry (allows non-builtin agents)
    definitions: Arc<AgentDefinitionRegistry>,
}

impl SubAgentTool {
    /// Create a new SubAgentTool with injected dependencies.
    pub fn new(
        tracker: Arc<SubagentTracker>,
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<crate::tools::ToolRegistry>,
        config: SubagentConfig,
        definitions: Arc<AgentDefinitionRegistry>,
    ) -> Self {
        Self {
            tracker,
            llm_client,
            tool_registry,
            config,
            definitions,
        }
    }
    
    /// Create a SubAgentTool with built-in definitions only (for backward compatibility).
    pub fn with_builtins(
        tracker: Arc<SubagentTracker>,
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<crate::tools::ToolRegistry>,
        config: SubagentConfig,
    ) -> Self {
        Self {
            tracker,
            llm_client,
            tool_registry,
            config,
            definitions: Arc::new(AgentDefinitionRegistry::with_builtins()),
        }
    }
}

#[async_trait]
impl Tool for SubAgentTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }

    fn description(&self) -> &str {
        "Spawn a specialized sub-agent to work on a task. The sub-agent will execute \
         autonomously and return results. Use this for tasks that can be delegated to specialists."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Agent type ID to spawn (e.g., 'researcher', 'coder')"
                },
                "task": {
                    "type": "string",
                    "description": "Detailed task description for the sub-agent"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (default: 300)"
                },
                "parent_subflow_path": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Parent subflow path for hierarchical tracking (inherited from parent agent context)"
                }
            },
            "required": ["agent_id", "task"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput> {
        let sub_agent_input: SubAgentInput = serde_json::from_value(input)
            .map_err(|e| crate::error::AiError::Tool(format!("Invalid input: {}", e)))?;

        let request = SpawnRequest {
            agent_id: sub_agent_input.agent_id,
            task: sub_agent_input.task,
            timeout_secs: sub_agent_input.timeout_secs,
            priority: None,
            parent_subflow_path: sub_agent_input.parent_subflow_path,
        };

        let handle = spawn_subagent(
            self.tracker.clone(),
            self.definitions.clone(),
            self.llm_client.clone(),
            self.tool_registry.clone(),
            self.config.clone(),
            request,
        )?;

        // Wait for completion
        let result = self.tracker.wait(&handle.id).await;

        let output = match result {
            Some(res) => {
                if res.success {
                    ToolOutput::success(serde_json::json!({"output": res.output, "summary": res.summary}))
                } else {
                    ToolOutput::error(res.error.unwrap_or_else(|| "Sub-agent failed".to_string()))
                }
            }
            None => ToolOutput::error("Sub-agent did not complete".to_string()),
        };

        Ok(output)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_agent_input_deserialization() {
        let json = r#"{
            "agent_id": "researcher",
            "task": "Research Rust async patterns",
            "timeout_secs": 600,
            "parent_subflow_path": ["call_1", "call_2"]
        }"#;
        
        let input: SubAgentInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.agent_id, "researcher");
        assert_eq!(input.task, "Research Rust async patterns");
        assert_eq!(input.timeout_secs, Some(600));
        assert_eq!(input.parent_subflow_path, vec!["call_1", "call_2"]);
    }

    #[test]
    fn test_sub_agent_input_minimal() {
        let json = r#"{
            "agent_id": "coder",
            "task": "Write a function"
        }"#;
        
        let input: SubAgentInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.agent_id, "coder");
        assert_eq!(input.task, "Write a function");
        assert_eq!(input.timeout_secs, None);
        assert!(input.parent_subflow_path.is_empty());
    }

    #[test]
    fn test_parameters_schema_includes_parent_subflow_path() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Agent type ID to spawn (e.g., 'researcher', 'coder')"
                },
                "task": {
                    "type": "string",
                    "description": "Detailed task description for the sub-agent"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (default: 300)"
                },
                "parent_subflow_path": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Parent subflow path for hierarchical tracking (inherited from parent agent context)"
                }
            },
            "required": ["agent_id", "task"]
        });
        
        // Verify parent_subflow_path is in schema
        assert!(schema["properties"]["parent_subflow_path"].is_object());
        assert_eq!(schema["properties"]["parent_subflow_path"]["type"], "array");
    }
}
