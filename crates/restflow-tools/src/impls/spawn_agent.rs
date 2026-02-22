//! spawn_agent tool - Spawn a sub-agent to work on a task in parallel.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::{Duration, timeout};

use crate::error::{Result, ToolError};
use crate::tool::{Tool, ToolOutput};
use restflow_ai::agent::{SubagentDeps, SpawnRequest, spawn_subagent};

/// Parameters for spawn_agent tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnAgentParams {
    /// Agent type to spawn (researcher, coder, reviewer, writer, analyst).
    pub agent: String,

    /// Task description for the agent.
    pub task: String,

    /// If true, wait for completion. If false (default), run in background.
    #[serde(default)]
    pub wait: bool,

    /// Timeout in seconds (default: 300).
    pub timeout_secs: Option<u64>,
}

/// spawn_agent tool for the shared agent execution engine.
pub struct SpawnAgentTool {
    deps: Arc<SubagentDeps>,
}

impl SpawnAgentTool {
    pub fn new(deps: Arc<SubagentDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl Tool for SpawnAgentTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }

    fn description(&self) -> &str {
        "Spawn a specialized agent to work on a task in parallel. The agent runs in the background; call wait_agents to check completion."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "enum": ["researcher", "coder", "reviewer", "writer", "analyst"],
                    "description": "The specialized agent to spawn"
                },
                "task": {
                    "type": "string",
                    "description": "Detailed task description for the agent"
                },
                "wait": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, wait for completion. If false, run in background."
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": 300,
                    "description": "Timeout in seconds (default: 300)"
                }
            },
            "required": ["agent", "task"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SpawnAgentParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

        let request = SpawnRequest {
            agent_id: params.agent.clone(),
            task: params.task.clone(),
            timeout_secs: params.timeout_secs,
            priority: None,
        };

        let handle = spawn_subagent(
            self.deps.tracker.clone(),
            self.deps.definitions.clone(),
            self.deps.llm_client.clone(),
            self.deps.tool_registry.clone(),
            self.deps.config.clone(),
            request,
        )
        .map_err(|e| ToolError::Tool(e.to_string()))?;

        if params.wait {
            let wait_timeout = params
                .timeout_secs
                .unwrap_or(self.deps.config.subagent_timeout_secs);

            let result = match timeout(
                Duration::from_secs(wait_timeout),
                self.deps.tracker.wait(&handle.id),
            )
            .await
            {
                Ok(Some(result)) => result,
                Ok(None) => return Ok(ToolOutput::error("Sub-agent not found")),
                Err(_) => {
                    return Ok(ToolOutput::success(json!({
                        "agent": handle.agent_name,
                        "status": "timeout",
                        "message": "Timeout waiting for sub-agent"
                    })));
                }
            };

            let output = if result.success {
                json!({
                    "agent": handle.agent_name,
                    "status": "completed",
                    "output": result.output,
                    "duration_ms": result.duration_ms
                })
            } else {
                json!({
                    "agent": handle.agent_name,
                    "status": "failed",
                    "error": result.error.unwrap_or_else(|| "Unknown error".to_string()),
                    "duration_ms": result.duration_ms
                })
            };
            Ok(ToolOutput::success(output))
        } else {
            Ok(ToolOutput::success(json!({
                "task_id": handle.id,
                "agent": handle.agent_name,
                "status": "spawned",
                "message": format!(
                    "Agent '{}' is now working on the task in background. Use wait_agents to check completion.",
                    handle.agent_name
                )
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;
    use restflow_ai::agent::{
        SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
        SubagentTracker,
    };
    use restflow_ai::llm::{MockLlmClient, MockStep};
    use restflow_ai::tools::ToolRegistry;
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    struct MockDefLookup {
        defs: HashMap<String, SubagentDefSnapshot>,
        summaries: Vec<SubagentDefSummary>,
    }

    impl MockDefLookup {
        fn with_agents(agents: Vec<(&str, &str)>) -> Self {
            let mut defs = HashMap::new();
            let mut summaries = Vec::new();
            for (id, name) in agents {
                defs.insert(
                    id.to_string(),
                    SubagentDefSnapshot {
                        name: name.to_string(),
                        system_prompt: format!("You are a {} agent.", name),
                        allowed_tools: vec![],
                        max_iterations: Some(1),
                    },
                );
                summaries.push(SubagentDefSummary {
                    id: id.to_string(),
                    name: name.to_string(),
                    description: format!("{} agent", name),
                    tags: vec![],
                });
            }
            Self { defs, summaries }
        }
    }

    impl SubagentDefLookup for MockDefLookup {
        fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
            self.defs.get(id).cloned()
        }
        fn list_callable(&self) -> Vec<SubagentDefSummary> {
            self.summaries.clone()
        }
    }

    fn make_test_deps(
        agents: Vec<(&str, &str)>,
        mock_steps: Vec<MockStep>,
    ) -> Arc<SubagentDeps> {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> =
            Arc::new(MockDefLookup::with_agents(agents));
        let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 5,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };
        Arc::new(SubagentDeps {
            tracker,
            definitions,
            llm_client,
            tool_registry,
            config,
        })
    }

    #[test]
    fn test_params_deserialization() {
        let json = r#"{"agent": "researcher", "task": "Research topic X"}"#;
        let params: SpawnAgentParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.agent, "researcher");
        assert!(!params.wait);
    }

    #[test]
    fn test_params_with_wait() {
        let json = r#"{"agent": "coder", "task": "Write function Y", "wait": true, "timeout_secs": 600}"#;
        let params: SpawnAgentParams = serde_json::from_str(json).unwrap();
        assert!(params.wait);
        assert_eq!(params.timeout_secs, Some(600));
    }

    #[tokio::test]
    async fn test_spawn_agent_background() {
        let deps = make_test_deps(
            vec![("researcher", "Researcher")],
            vec![MockStep::text("research done")],
        );
        let tool = SpawnAgentTool::new(deps);
        let result = tool
            .execute(json!({"agent": "researcher", "task": "Find info", "wait": false}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result["status"], "spawned");
        assert!(result.result["task_id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_spawn_agent_wait_success() {
        let deps = make_test_deps(
            vec![("coder", "Coder")],
            vec![MockStep::text("function written")],
        );
        let tool = SpawnAgentTool::new(deps);
        let result = tool
            .execute(json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 10}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result["status"], "completed");
        assert!(result.result["output"].as_str().unwrap().contains("function written"));
    }

    #[tokio::test]
    async fn test_spawn_agent_wait_failure() {
        let deps = make_test_deps(
            vec![("coder", "Coder")],
            vec![MockStep::error("LLM error")],
        );
        let tool = SpawnAgentTool::new(deps);
        let result = tool
            .execute(json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 10}))
            .await
            .unwrap();
        assert!(result.success); // ToolOutput is success, but status indicates failure
        assert_eq!(result.result["status"], "failed");
        assert!(result.result["error"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_spawn_agent_unknown_agent() {
        let deps = make_test_deps(vec![], vec![]);
        let tool = SpawnAgentTool::new(deps);
        let result = tool
            .execute(json!({"agent": "nonexistent", "task": "Do something"}))
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Unknown agent type"));
    }

    #[tokio::test]
    async fn test_spawn_agent_invalid_params() {
        let deps = make_test_deps(vec![], vec![]);
        let tool = SpawnAgentTool::new(deps);
        // Missing required "agent" and "task" fields
        let result = tool.execute(json!({"wait": true})).await;
        assert!(result.is_err());
    }
}
