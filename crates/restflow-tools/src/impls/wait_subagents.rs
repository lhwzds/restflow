//! wait_subagents tool - Wait for sub-agents to finish and return results.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::{Duration, timeout};

use crate::impls::subagent_read_capability::SubagentReadCapabilityService;
use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};
use restflow_traits::{DEFAULT_SUBAGENT_TIMEOUT_SECS, SubagentManager, SubagentStatus};

#[cfg(feature = "ts")]
const TS_EXPORT_TO_WEB_TYPES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/types/generated/"
);

/// Parameters for wait_subagents tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = TS_EXPORT_TO_WEB_TYPES))]
pub struct WaitSubagentsParams {
    /// Task IDs to wait for.
    pub task_ids: Vec<String>,

    /// Parent run scope that owns the requested tasks.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub parent_run_id: Option<String>,

    /// Timeout in seconds.
    /// - `Some(0)` means wait without timeout.
    /// - `None` uses subagent manager default timeout.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub timeout_secs: Option<u64>,
}

/// wait_subagents tool for the shared agent execution engine.
pub struct WaitSubagentsTool {
    manager: Arc<dyn SubagentManager>,
    capability: SubagentReadCapabilityService,
}

impl WaitSubagentsTool {
    pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
        let capability = SubagentReadCapabilityService::new(manager.clone());
        Self {
            manager,
            capability,
        }
    }

    fn completion_entry(task_id: &str, completion: restflow_traits::SubagentCompletion) -> Value {
        let status = match completion.status {
            SubagentStatus::Completed => "completed",
            SubagentStatus::Failed => "failed",
            SubagentStatus::Interrupted => "interrupted",
            SubagentStatus::TimedOut => "timed_out",
            SubagentStatus::Pending => "pending",
            SubagentStatus::Running => "running",
        };

        let mut entry = json!({
            "task_id": task_id,
            "status": status,
        });

        if let Some(result) = completion.result {
            entry["duration_ms"] = json!(result.duration_ms);
            if result.success {
                entry["output"] = json!(result.output);
            } else {
                entry["error"] = json!(result.error.unwrap_or_else(|| "Unknown error".to_string()));
                if !result.output.is_empty() {
                    entry["output"] = json!(result.output);
                }
            }
        }

        entry
    }
}

#[async_trait]
impl Tool for WaitSubagentsTool {
    fn name(&self) -> &str {
        "wait_subagents"
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
                "parent_run_id": {
                    "type": "string",
                    "description": "Required parent run scope that owns the requested task IDs"
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": DEFAULT_SUBAGENT_TIMEOUT_SECS,
                    "minimum": 0,
                    "description": "Timeout in seconds. Use 0 to wait without timeout. If omitted, uses subagent manager default timeout."
                }
            },
            "required": ["task_ids"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: WaitSubagentsParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;
        let parent_run_id = params
            .parent_run_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ToolError::Tool("parent_run_id is required for wait_subagents.".to_string())
            })?;

        let wait_timeout = params
            .timeout_secs
            .unwrap_or(self.manager.config().subagent_timeout_secs);

        let mut results = Vec::new();
        for task_id in params.task_ids {
            let wait_result = if wait_timeout == 0 {
                self.capability
                    .wait_for_parent_owned_task(&task_id, Some(parent_run_id))
                    .await?
            } else {
                match timeout(
                    Duration::from_secs(wait_timeout),
                    self.capability
                        .wait_for_parent_owned_task(&task_id, Some(parent_run_id)),
                )
                .await
                {
                    Ok(result) => result?,
                    Err(_) => {
                        results.push(json!({"task_id": task_id, "status": "timeout"}));
                        continue;
                    }
                }
            };

            let completion = match wait_result {
                Some(result) => result,
                None => {
                    results.push(json!({"task_id": task_id, "status": "not_found"}));
                    continue;
                }
            };
            results.push(Self::completion_entry(&task_id, completion));
        }

        Ok(ToolOutput::success(json!({ "results": results })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use restflow_ai::agent::{
        SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentDeps,
        SubagentManagerImpl, SubagentTracker,
    };
    use restflow_ai::llm::{MockLlmClient, MockStep};
    use restflow_ai::tools::ToolRegistry;
    use restflow_contracts::request::SubagentSpawnRequest as ContractSubagentSpawnRequest;
    use restflow_traits::SubagentManager;
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
                    default_model: None,
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
            vec![SubagentDefSummary {
                id: "tester".to_string(),
                name: "tester".to_string(),
                description: "test agent".to_string(),
                tags: vec![],
            }]
        }
    }

    fn make_deps(mock_steps: Vec<MockStep>) -> (Arc<SubagentDeps>, Arc<dyn SubagentManager>) {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
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
            llm_client_factory: None,
            orchestrator: None,
        });
        let manager: Arc<dyn SubagentManager> = Arc::new(SubagentManagerImpl::from_deps(&deps));
        (deps, manager)
    }

    /// Spawn a subagent that immediately completes (via MockLlmClient) and return its task_id.
    fn spawn_test_agent(manager: &Arc<dyn SubagentManager>) -> String {
        let handle = manager
            .spawn(ContractSubagentSpawnRequest {
                agent_id: Some("tester".to_string()),
                task: "test task".to_string(),
                timeout_secs: Some(10),
                parent_execution_id: Some("parent-1".to_string()),
                ..ContractSubagentSpawnRequest::default()
            })
            .expect("spawn should succeed");
        handle.id
    }

    #[test]
    fn test_params_deserialization() {
        let json = r#"{"task_ids": ["task-1", "task-2"], "parent_run_id": "parent-1", "timeout_secs": 120}"#;
        let params: WaitSubagentsParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.task_ids.len(), 2);
        assert_eq!(params.parent_run_id.as_deref(), Some("parent-1"));
        assert_eq!(params.timeout_secs, Some(120));
    }

    #[test]
    fn test_parameters_schema_uses_shared_timeout_default() {
        let (_deps, manager) = make_deps(vec![]);
        let tool = WaitSubagentsTool::new(manager);
        let schema = tool.parameters_schema();
        assert_eq!(
            schema["properties"]["timeout_secs"]["default"],
            json!(DEFAULT_SUBAGENT_TIMEOUT_SECS)
        );
    }

    #[tokio::test]
    async fn test_wait_completed_task() {
        let (_deps, manager) = make_deps(vec![MockStep::text("done")]);
        let task_id = spawn_test_agent(&manager);

        let tool = WaitSubagentsTool::new(manager);
        let result = tool
            .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 1}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["status"], "completed");
    }

    #[tokio::test]
    async fn test_wait_nonexistent_task() {
        let (_deps, manager) = make_deps(vec![]);
        let tool = WaitSubagentsTool::new(manager);
        let result = tool
            .execute(json!({"task_ids": ["no-such-task"], "parent_run_id": "parent-1", "timeout_secs": 1}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results[0]["status"], "not_found");
    }

    #[tokio::test]
    async fn test_wait_timeout() {
        // Use a delayed step that exceeds the wait timeout.
        let (_deps, manager) = make_deps(vec![MockStep::text("slow").with_delay(2_000)]);
        let task_id = spawn_test_agent(&manager);

        let tool = WaitSubagentsTool::new(manager);
        let result = tool
            .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 1}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results[0]["status"], "timeout");
    }

    #[tokio::test]
    async fn test_wait_interrupted_task() {
        let (deps, manager) = make_deps(vec![MockStep::text("slow").with_delay(2_000)]);
        let task_id = spawn_test_agent(&manager);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(deps.tracker.cancel(&task_id));

        let tool = WaitSubagentsTool::new(manager);
        let result = tool
            .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 1}))
            .await
            .unwrap();

        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results[0]["status"], "interrupted");
        assert_eq!(results[0]["error"], json!("Sub-agent interrupted"));
    }

    #[tokio::test]
    async fn test_wait_multiple_tasks() {
        let (_deps, manager) =
            make_deps(vec![MockStep::text("result-1"), MockStep::text("result-2")]);
        let id1 = spawn_test_agent(&manager);
        let id2 = spawn_test_agent(&manager);

        let tool = WaitSubagentsTool::new(manager);
        let result = tool
            .execute(json!({"task_ids": [id1, id2, "missing"], "parent_run_id": "parent-1", "timeout_secs": 1}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        // First two should be completed, third not_found
        assert_eq!(results[2]["status"], "not_found");
    }

    #[tokio::test]
    async fn test_wait_with_zero_timeout_waits_for_completion() {
        let (_deps, manager) = make_deps(vec![MockStep::text("slow-done").with_delay(200)]);
        let task_id = spawn_test_agent(&manager);

        let tool = WaitSubagentsTool::new(manager);
        let result = tool
            .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 0}))
            .await
            .unwrap();

        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["status"], "completed");
    }

    #[tokio::test]
    async fn test_wait_failed_task() {
        let (_deps, manager) = make_deps(vec![MockStep::error("LLM error")]);
        let task_id = spawn_test_agent(&manager);

        let tool = WaitSubagentsTool::new(manager);
        let result = tool
            .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 1}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results[0]["status"], "failed");
        assert!(results[0]["error"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_wait_requires_parent_scope() {
        let (_deps, manager) = make_deps(vec![]);
        let tool = WaitSubagentsTool::new(manager);
        let err = tool
            .execute(json!({"task_ids": ["task-1"], "timeout_secs": 1}))
            .await
            .expect_err("missing parent scope should fail");
        assert!(err.to_string().contains("parent_run_id is required"));
    }

    #[tokio::test]
    async fn test_wait_rejects_foreign_parent_scope() {
        let (_deps, manager) = make_deps(vec![MockStep::text("done")]);
        let task_id = manager
            .spawn(ContractSubagentSpawnRequest {
                agent_id: Some("tester".to_string()),
                task: "test task".to_string(),
                timeout_secs: Some(10),
                parent_execution_id: Some("parent-1".to_string()),
                ..ContractSubagentSpawnRequest::default()
            })
            .expect("spawn should succeed")
            .id;

        let tool = WaitSubagentsTool::new(manager);
        let result = tool
            .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-2", "timeout_secs": 1}))
            .await
            .unwrap();
        assert!(result.success);
        let results = result.result["results"].as_array().unwrap();
        assert_eq!(results[0]["status"], "not_found");
    }
}
