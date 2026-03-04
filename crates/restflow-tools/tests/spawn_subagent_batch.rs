//! Integration tests for spawn_subagent_batch tool.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use restflow_ai::agent::{
    SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentDeps,
    SubagentManagerImpl, SubagentTracker,
};
use restflow_ai::llm::{MockLlmClient, MockStep};
use restflow_ai::tools::ToolRegistry;
use restflow_tools::{SpawnSubagentBatchTool, SpawnSubagentTool, Tool};
use restflow_traits::SubagentManager;
use restflow_traits::store::KvStore;
use serde_json::{Value, json};
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
                    default_model: None,
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

#[derive(Default)]
struct MockKvStore {
    entries: Mutex<HashMap<String, String>>,
}

impl KvStore for MockKvStore {
    fn get_entry(&self, key: &str) -> restflow_tools::Result<Value> {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(value) = entries.get(key) {
            Ok(json!({
                "found": true,
                "key": key,
                "value": value
            }))
        } else {
            Ok(json!({
                "found": false,
                "key": key
            }))
        }
    }

    fn set_entry(
        &self,
        key: &str,
        content: &str,
        _visibility: Option<&str>,
        _content_type: Option<&str>,
        _type_hint: Option<&str>,
        _tags: Option<Vec<String>>,
        _accessor_id: Option<&str>,
    ) -> restflow_tools::Result<Value> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.insert(key.to_string(), content.to_string());
        Ok(json!({"success": true, "key": key}))
    }

    fn delete_entry(&self, key: &str, _accessor_id: Option<&str>) -> restflow_tools::Result<Value> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let deleted = entries.remove(key).is_some();
        Ok(json!({"deleted": deleted, "key": key}))
    }

    fn list_entries(&self, namespace: Option<&str>) -> restflow_tools::Result<Value> {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prefix = namespace.map(|value| format!("{value}:"));
        let list = entries
            .keys()
            .filter(|key| {
                prefix
                    .as_ref()
                    .map(|value| key.starts_with(value))
                    .unwrap_or(true)
            })
            .map(|key| json!({ "key": key }))
            .collect::<Vec<_>>();
        Ok(json!({
            "count": list.len(),
            "entries": list
        }))
    }
}

fn make_manager_with_parallel(
    agents: Vec<(&str, &str)>,
    steps: Vec<MockStep>,
    max_parallel_agents: usize,
) -> Arc<dyn SubagentManager> {
    let (tx, rx) = mpsc::channel(32);
    let tracker = Arc::new(SubagentTracker::new(tx, rx));
    let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agents(agents));
    let llm_client = Arc::new(MockLlmClient::from_steps("mock", steps));
    let tool_registry = Arc::new(ToolRegistry::new());
    let config = SubagentConfig {
        max_parallel_agents,
        subagent_timeout_secs: 10,
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
    });
    Arc::new(SubagentManagerImpl::from_deps(&deps))
}

fn make_manager(agents: Vec<(&str, &str)>, steps: Vec<MockStep>) -> Arc<dyn SubagentManager> {
    make_manager_with_parallel(agents, steps, 20)
}

#[tokio::test]
async fn test_spawn_subagent_batch_fanout_wait() {
    let manager = make_manager(
        vec![("coder", "Coder")],
        vec![
            MockStep::text("result-1"),
            MockStep::text("result-2"),
            MockStep::text("result-3"),
        ],
    );
    let tool = SpawnSubagentBatchTool::new(manager);

    let output = tool
        .execute(json!({
            "operation": "spawn",
            "task": "Review code",
            "wait": true,
            "specs": [
                { "agent": "coder", "count": 3, "model": "gpt-5.3-codex", "provider": "openai-codex" }
            ]
        }))
        .await
        .expect("spawn should succeed");

    assert!(output.success);
    assert_eq!(output.result["status"], "completed");
    assert_eq!(output.result["spawned_count"], 3);
    assert_eq!(output.result["results"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_spawn_subagent_batch_team_persistence() {
    let manager = make_manager(
        vec![("coder", "Coder")],
        vec![
            MockStep::text("team-result-1"),
            MockStep::text("team-result-2"),
        ],
    );
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

    let save_output = tool
        .execute(json!({
            "operation": "save_team",
            "team": "TeamAlpha",
            "specs": [
                { "agent": "coder", "count": 2 }
            ]
        }))
        .await
        .expect("save_team should succeed");
    assert!(save_output.success);

    let spawn_output = tool
        .execute(json!({
            "operation": "spawn",
            "team": "TeamAlpha",
            "task": "Team run",
            "wait": true
        }))
        .await
        .expect("spawn from team should succeed");
    assert!(spawn_output.success);
    assert_eq!(spawn_output.result["spawned_count"], 2);

    let list_output = tool
        .execute(json!({
            "operation": "list_teams"
        }))
        .await
        .expect("list_teams should succeed");
    assert!(list_output.success);
    let teams = list_output.result["teams"].as_array().unwrap();
    assert!(teams.iter().any(|entry| entry["team"] == "TeamAlpha"));
}

#[tokio::test]
async fn test_spawn_subagent_batch_rejects_team_and_specs_combined() {
    let manager = make_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "team": "TeamAlpha",
            "specs": [
                { "agent": "coder", "count": 1 }
            ],
            "task": "Review code"
        }))
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("either 'team' or 'specs'")
    );
}

#[tokio::test]
async fn test_spawn_subagent_batch_requires_task_when_unspecified() {
    let manager = make_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "specs": [
                { "agent": "coder", "count": 1 }
            ]
        }))
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Missing task for spec index 0")
    );
}

#[tokio::test]
async fn test_spawn_subagent_batch_rejects_over_parallel_limit() {
    let manager = make_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "task": "Review code",
            "specs": [
                { "agent": "coder", "count": 21 }
            ]
        }))
        .await;

    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(message.contains("Requested 21 sub-agents"));
    assert!(message.contains("max_parallel: 20"));
}

#[tokio::test]
async fn test_spawn_subagent_batch_save_as_team_requires_store() {
    let manager = make_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "task": "Review code",
            "save_as_team": "NoStore",
            "specs": [
                { "agent": "coder", "count": 1 }
            ]
        }))
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Team storage is unavailable")
    );
}

#[tokio::test]
async fn test_spawn_subagent_batch_save_as_team_persists_and_reloads() {
    let manager = make_manager(
        vec![("coder", "Coder")],
        vec![MockStep::text("saved-1"), MockStep::text("saved-2")],
    );
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

    let spawn_output = tool
        .execute(json!({
            "operation": "spawn",
            "task": "Team review",
            "wait": true,
            "save_as_team": "SavedTeam",
            "specs": [
                { "agent": "coder", "count": 2 }
            ]
        }))
        .await
        .expect("spawn with save_as_team should succeed");
    assert!(spawn_output.success);
    assert_eq!(spawn_output.result["saved_team"], "SavedTeam");
    assert_eq!(spawn_output.result["spawned_count"], 2);

    let get_output = tool
        .execute(json!({
            "operation": "get_team",
            "team": "SavedTeam"
        }))
        .await
        .expect("get_team should succeed");
    assert!(get_output.success);
    assert_eq!(get_output.result["team"], "SavedTeam");
    assert_eq!(get_output.result["total_instances"], 2);
}

#[tokio::test]
async fn test_spawn_subagent_wrapper_workers_save_as_team_and_reuse() {
    let manager = make_manager(
        vec![("coder", "Coder")],
        vec![MockStep::text("wrapped-1"), MockStep::text("wrapped-2")],
    );
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentTool::new(manager).with_kv_store(kv_store);

    let first_output = tool
        .execute(json!({
            "task": "Wrapper run",
            "wait": true,
            "save_as_team": "WrapperTeam",
            "workers": [
                { "agent": "coder", "count": 1 }
            ]
        }))
        .await
        .expect("wrapper workers mode should succeed");
    assert!(first_output.success);
    assert_eq!(first_output.result["saved_team"], "WrapperTeam");
    assert_eq!(first_output.result["spawned_count"], 1);

    let second_output = tool
        .execute(json!({
            "task": "Reuse team",
            "wait": true,
            "team": "WrapperTeam"
        }))
        .await
        .expect("wrapper team mode should succeed");
    assert!(second_output.success);
    assert_eq!(second_output.result["spawned_count"], 1);
}

#[tokio::test]
async fn test_spawn_subagent_wrapper_rejects_mixed_single_and_batch_fields() {
    let manager = make_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentTool::new(manager);

    let result = tool
        .execute(json!({
            "task": "Mixed mode",
            "agent": "coder",
            "workers": [
                { "agent": "coder", "count": 1 }
            ]
        }))
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Batch mode uses 'workers'/'team'")
    );
}

#[tokio::test]
async fn test_spawn_subagent_wrapper_supports_mixed_minimax_and_glm5_team() {
    let steps = (0..64)
        .map(|index| MockStep::text(format!("result-{index}")))
        .collect::<Vec<_>>();
    let manager = make_manager_with_parallel(vec![("planner", "Planner")], steps, 64);
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentTool::new(manager).with_kv_store(kv_store);

    let first_output = tool
        .execute(json!({
            "task": "Create implementation plans",
            "wait": true,
            "save_as_team": "PlanTeam",
            "workers": [
                {
                    "agent": "planner",
                    "count": 20,
                    "model": "minimax/coding-plan",
                    "provider": "minimax"
                },
                {
                    "agent": "planner",
                    "count": 3,
                    "model": "glm5/coding-plan",
                    "provider": "glm5"
                }
            ]
        }))
        .await
        .expect("mixed provider team spawn should succeed");

    assert!(first_output.success);
    assert_eq!(first_output.result["status"], "completed");
    assert_eq!(first_output.result["saved_team"], "PlanTeam");
    assert_eq!(first_output.result["spawned_count"], 23);

    let second_output = tool
        .execute(json!({
            "task": "Re-run planning wave",
            "wait": true,
            "team": "PlanTeam"
        }))
        .await
        .expect("spawn from saved mixed provider team should succeed");

    assert!(second_output.success);
    assert_eq!(second_output.result["status"], "completed");
    assert_eq!(second_output.result["spawned_count"], 23);
}

#[tokio::test]
async fn test_spawn_subagent_batch_supports_distinct_tasks_list() {
    let manager = make_manager(
        vec![("coder", "Coder")],
        vec![
            MockStep::text("distinct-1"),
            MockStep::text("distinct-2"),
            MockStep::text("distinct-3"),
        ],
    );
    let tool = SpawnSubagentBatchTool::new(manager);

    let output = tool
        .execute(json!({
            "operation": "spawn",
            "wait": true,
            "specs": [
                { "agent": "coder", "tasks": ["prompt-1", "prompt-2", "prompt-3"] }
            ]
        }))
        .await
        .expect("spawn with tasks list should succeed");

    assert!(output.success);
    assert_eq!(output.result["status"], "completed");
    assert_eq!(output.result["spawned_count"], 3);
    assert_eq!(output.result["results"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_spawn_subagent_wrapper_supports_distinct_tasks_list() {
    let manager = make_manager(
        vec![("coder", "Coder")],
        vec![MockStep::text("a"), MockStep::text("b")],
    );
    let tool = SpawnSubagentTool::new(manager);

    let output = tool
        .execute(json!({
            "task": "",
            "wait": true,
            "workers": [
                { "agent": "coder", "tasks": ["task-a", "task-b"] }
            ]
        }))
        .await
        .expect("wrapper workers tasks should succeed");

    assert!(output.success);
    assert_eq!(output.result["status"], "completed");
    assert_eq!(output.result["spawned_count"], 2);
}

#[tokio::test]
async fn test_spawn_subagent_batch_rejects_task_and_tasks_combined() {
    let manager = make_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "specs": [
                { "agent": "coder", "task": "single", "tasks": ["a"] }
            ]
        }))
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("either 'task' or 'tasks'")
    );
}

#[tokio::test]
async fn test_spawn_subagent_batch_rejects_tasks_count_mismatch() {
    let manager = make_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "specs": [
                { "agent": "coder", "count": 2, "tasks": ["a", "b", "c"] }
            ]
        }))
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Set count to 1 (default) or match tasks length")
    );
}
