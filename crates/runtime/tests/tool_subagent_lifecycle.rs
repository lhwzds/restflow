//! Cross-tool integration tests for the subagent lifecycle.

use std::collections::HashMap;
use std::sync::Arc;

use ai::agent::{
    SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentDeps,
    SubagentManagerImpl, SubagentTracker,
};
use ai::llm::{MockLlmClient, MockStep};
use ai::tools::ToolRegistry;
use runtime::tools::{ListSubagentsTool, SpawnSubagentTool, Tool, WaitSubagentsTool};
use serde_json::json;
use tokio::sync::mpsc;
use types::SubagentManager;

const TEST_PARENT_RUN_ID: &str = "parent-1";

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
                    system_prompt: format!("You are a {name} agent."),
                    allowed_tools: vec![],
                    max_iterations: Some(1),
                    default_model: None,
                },
            );
            summaries.push(SubagentDefSummary {
                id: id.to_string(),
                name: name.to_string(),
                description: format!("{name} agent"),
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

fn make_shared_deps(
    agents: Vec<(&str, &str)>,
    mock_steps: Vec<MockStep>,
) -> Arc<dyn SubagentManager> {
    let (tx, rx) = mpsc::channel(32);
    let tracker = Arc::new(SubagentTracker::new(tx, rx));
    let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agents(agents));
    let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
    let tool_registry = Arc::new(ToolRegistry::new());
    let config = SubagentConfig {
        max_parallel_agents: 10,
        subagent_timeout_secs: 30,
        max_iterations: 5,
        max_depth: 1,
    };
    let deps = Arc::new(SubagentDeps {
        tracker,
        definitions,
        llm_client,
        tool_registry,
        config,
        llm_client_factory: None,
        orchestrator: None,
    });
    Arc::new(SubagentManagerImpl::from_deps(&deps))
}

#[tokio::test]
async fn test_spawn_then_wait_lifecycle() {
    let deps = make_shared_deps(
        vec![("researcher", "Researcher")],
        vec![MockStep::text("research complete")],
    );

    let spawn_tool = SpawnSubagentTool::new(deps.clone());
    let spawn_result = spawn_tool
        .execute(json!({
            "agent": "researcher",
            "task": "Find info",
            "wait": false,
            "parent_run_id": TEST_PARENT_RUN_ID
        }))
        .await
        .unwrap();
    assert!(spawn_result.success);
    assert_eq!(spawn_result.result["status"], "spawned");
    let task_id = spawn_result.result["task_id"].as_str().unwrap().to_string();

    let wait_tool = WaitSubagentsTool::new(deps);
    let wait_result = wait_tool
        .execute(json!({
            "task_ids": [task_id],
            "parent_run_id": TEST_PARENT_RUN_ID,
            "timeout_secs": 10
        }))
        .await
        .unwrap();
    assert!(wait_result.success);
    let results = wait_result.result["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["status"], "completed");
}

#[tokio::test]
async fn test_spawn_then_list_shows_running() {
    let deps = make_shared_deps(
        vec![("coder", "Coder")],
        vec![MockStep::text("slow result").with_delay(5000)],
    );

    let spawn_tool = SpawnSubagentTool::new(deps.clone());
    let spawn_result = spawn_tool
        .execute(json!({
            "agent": "coder",
            "task": "Write code",
            "wait": false,
            "parent_run_id": TEST_PARENT_RUN_ID
        }))
        .await
        .unwrap();
    assert!(spawn_result.success);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let list_tool = ListSubagentsTool::new(deps);
    let list_result = list_tool
        .execute(json!({"parent_run_id": TEST_PARENT_RUN_ID}))
        .await
        .unwrap();
    assert!(list_result.success);
    assert!(list_result.result["running_count"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn test_spawn_multiple_then_wait_all() {
    let deps = make_shared_deps(
        vec![("researcher", "Researcher"), ("coder", "Coder")],
        vec![
            MockStep::text("result 1"),
            MockStep::text("result 2"),
            MockStep::text("result 3"),
        ],
    );

    let spawn_tool = SpawnSubagentTool::new(deps.clone());
    let mut task_ids = Vec::new();
    for (agent, task_desc) in [
        ("researcher", "task 1"),
        ("coder", "task 2"),
        ("researcher", "task 3"),
    ] {
        let result = spawn_tool
            .execute(json!({
                "agent": agent,
                "task": task_desc,
                "wait": false,
                "parent_run_id": TEST_PARENT_RUN_ID
            }))
            .await
            .unwrap();
        assert!(result.success);
        task_ids.push(result.result["task_id"].as_str().unwrap().to_string());
    }

    let wait_tool = WaitSubagentsTool::new(deps);
    let wait_result = wait_tool
        .execute(json!({
            "task_ids": task_ids,
            "parent_run_id": TEST_PARENT_RUN_ID,
            "timeout_secs": 10
        }))
        .await
        .unwrap();
    assert!(wait_result.success);
    let results = wait_result.result["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    for result in results {
        assert_eq!(result["status"], "completed");
    }
}

#[tokio::test]
async fn test_spawn_unknown_agent_error() {
    let deps = make_shared_deps(vec![("coder", "Coder")], vec![]);

    let spawn_tool = SpawnSubagentTool::new(deps.clone());
    let result = spawn_tool
        .execute(json!({"agent": "nonexistent", "task": "impossible"}))
        .await;
    assert!(result.is_err());

    let list_tool = ListSubagentsTool::new(deps);
    let list_result = list_tool.execute(json!({})).await.unwrap();
    assert!(list_result.success);
    assert_eq!(list_result.result["running_count"], 0);
}

#[tokio::test]
async fn test_spawn_wait_timeout_then_list() {
    let deps = make_shared_deps(
        vec![("coder", "Coder")],
        vec![MockStep::text("never").with_delay(60_000)],
    );

    let spawn_tool = SpawnSubagentTool::new(deps.clone());
    let spawn_result = spawn_tool
        .execute(json!({
            "agent": "coder",
            "task": "infinite task",
            "wait": false,
            "parent_run_id": TEST_PARENT_RUN_ID
        }))
        .await
        .unwrap();
    let task_id = spawn_result.result["task_id"].as_str().unwrap().to_string();

    let wait_tool = WaitSubagentsTool::new(deps.clone());
    let wait_result = wait_tool
        .execute(json!({
            "task_ids": [task_id],
            "parent_run_id": TEST_PARENT_RUN_ID,
            "timeout_secs": 1
        }))
        .await
        .unwrap();
    assert!(wait_result.success);
    let results = wait_result.result["results"].as_array().unwrap();
    assert_eq!(results[0]["status"], "timeout");

    let list_tool = ListSubagentsTool::new(deps);
    let list_result = list_tool
        .execute(json!({"parent_run_id": TEST_PARENT_RUN_ID}))
        .await
        .unwrap();
    assert!(list_result.success);
    assert!(list_result.result["running_count"].as_u64().is_some());
}
