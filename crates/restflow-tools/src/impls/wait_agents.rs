//! wait_agents tool - Wait for sub-agents to finish and return results.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::{Duration, timeout};

use crate::error::{Result, ToolError};
use crate::tool::{Tool, ToolOutput};
use restflow_ai::agent::SubagentDeps;

/// Parameters for wait_agents tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitAgentsParams {
    /// Task IDs to wait for.
    pub task_ids: Vec<String>,

    /// Timeout in seconds (default: 300).
    pub timeout_secs: Option<u64>,
}

/// wait_agents tool for the shared agent execution engine.
pub struct WaitAgentsTool {
    deps: Arc<SubagentDeps>,
}

impl WaitAgentsTool {
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
                    "description": "List of sub-agent task IDs to wait for"
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": 300,
                    "description": "Timeout in seconds (default: 300)"
                }
            },
            "required": ["task_ids"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: WaitAgentsParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

        let wait_timeout = params
            .timeout_secs
            .unwrap_or(self.deps.config.subagent_timeout_secs);

        let mut results = Vec::new();
        for task_id in params.task_ids {
            let result = match timeout(
                Duration::from_secs(wait_timeout),
                self.deps.tracker.wait(&task_id),
            )
            .await
            {
                Ok(Some(result)) => result,
                Ok(None) => {
                    results.push(json!({"task_id": task_id, "status": "not_found"}));
                    continue;
                }
                Err(_) => {
                    results.push(json!({"task_id": task_id, "status": "timeout"}));
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

        Ok(ToolOutput::success(json!({ "results": results })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;
    use restflow_ai::agent::{
        SpawnRequest, SubagentConfig, SubagentDefLookup, SubagentDefSnapshot,
        SubagentDefSummary, SubagentTracker, spawn_subagent,
    };
    use restflow_ai::llm::{MockLlmClient, MockStep};
    use restflow_ai::tools::ToolRegistry;
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    struct MockDefLookup {
        defs: HashMap<String, SubagentDefSnapshot>,
    }

    impl MockDefLookup {
        fn with_agent(id: &str) -> Self {
            let mut defs = HashMap::new();
            defs.insert(
                id.to_string(),
                SubagentDefSnapshot {
                    name: id.to_string(),
                    system_prompt: "You are a test agent.".to_string(),
                    allowed_tools: vec![],
                    max_iterations: Some(1),
                },
            );
            Self { defs }
        }
    }

    impl SubagentDefLookup for MockDefLookup {
        fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
            self.defs.get(id).cloned()
        }
        fn list_callable(&self) -> Vec<SubagentDefSummary> {
            vec![]
        }
    }

    fn make_deps(
        mock_steps: Vec<MockStep>,
    ) -> (Arc<SubagentDeps>, Arc<SubagentTracker>) {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> =
            Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 5,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };
        let deps = Arc::new(SubagentDeps {
            tracker: tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
        });
        (deps, tracker)
    }

    /// Spawn a subagent that immediately completes (via MockLlmClient) and return its task_id.
    fn spawn_test_agent(deps: &SubagentDeps) -> String {
        let handle = spawn_subagent(
            deps.tracker.clone(),
            deps.definitions.clone(),
            deps.llm_client.clone(),
            deps.tool_registry.clone(),
            deps.config.clone(),
            SpawnRequest {
                agent_id: "tester".to_string(),
                task: "test task".to_string(),
                timeout_secs: Some(10),
                priority: None,
            },
        )
        .unwrap();
        handle.id
    }

    #[test]
    fn test_params_deserialization() {
        let json = r#"{"task_ids": ["task-1", "task-2"], "timeout_secs": 120}"#;
        let params: WaitAgentsParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.task_ids.len(), 2);
        assert_eq!(params.timeout_secs, Some(120));
    }

    #[tokio::test]
    async fn test_wait_completed_task() {
        let (deps, _tracker) = make_deps(vec![MockStep::text("done")]);
        let task_id = spawn_test_agent(&deps);

        let tool = WaitAgentsTool::new(deps);
        let result = tool
            .execute(json!({"task_ids": [task_id], "timeout_secs": 5}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["status"], "completed");
    }

    #[tokio::test]
    async fn test_wait_nonexistent_task() {
        let (deps, _tracker) = make_deps(vec![]);
        let tool = WaitAgentsTool::new(deps);
        let result = tool
            .execute(json!({"task_ids": ["no-such-task"], "timeout_secs": 1}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results[0]["status"], "not_found");
    }

    #[tokio::test]
    async fn test_wait_timeout() {
        // Use a delayed step that exceeds the wait timeout
        let (deps, _tracker) =
            make_deps(vec![MockStep::text("slow").with_delay(5000)]);
        let task_id = spawn_test_agent(&deps);

        let tool = WaitAgentsTool::new(deps);
        let result = tool
            .execute(json!({"task_ids": [task_id], "timeout_secs": 1}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results[0]["status"], "timeout");
    }

    #[tokio::test]
    async fn test_wait_multiple_tasks() {
        let (deps, _tracker) = make_deps(vec![
            MockStep::text("result-1"),
            MockStep::text("result-2"),
        ]);
        let id1 = spawn_test_agent(&deps);
        let id2 = spawn_test_agent(&deps);

        let tool = WaitAgentsTool::new(deps);
        let result = tool
            .execute(json!({"task_ids": [id1, id2, "missing"], "timeout_secs": 5}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        // First two should be completed, third not_found
        assert_eq!(results[2]["status"], "not_found");
    }

    #[tokio::test]
    async fn test_wait_failed_task() {
        let (deps, _tracker) = make_deps(vec![MockStep::error("LLM error")]);
        let task_id = spawn_test_agent(&deps);

        let tool = WaitAgentsTool::new(deps);
        let result = tool
            .execute(json!({"task_ids": [task_id], "timeout_secs": 5}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results[0]["status"], "failed");
        assert!(results[0]["error"].as_str().is_some());
    }
}
