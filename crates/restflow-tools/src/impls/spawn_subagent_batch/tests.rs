use super::*;
use crate::Tool;
use crate::impls::spawn_subagent_batch::types::SpawnSubagentBatchParams;
use restflow_ai::agent::{
    SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
    SubagentManagerImpl, SubagentTracker,
};
use restflow_ai::llm::{MockLlmClient, MockStep};
use restflow_ai::tools::ToolRegistry;
use restflow_contracts::request::SubagentSpawnRequest as ContractSubagentSpawnRequest;
use restflow_traits::store::KvStore;
use restflow_traits::{SpawnHandle, SubagentCompletion, SubagentManager, SubagentState};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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
    fn get_entry(&self, key: &str) -> Result<Value> {
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
    ) -> Result<Value> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.insert(key.to_string(), content.to_string());
        Ok(json!({"success": true, "key": key}))
    }

    fn delete_entry(&self, key: &str, _accessor_id: Option<&str>) -> Result<Value> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let deleted = entries.remove(key).is_some();
        Ok(json!({"deleted": deleted, "key": key}))
    }

    fn list_entries(&self, namespace: Option<&str>) -> Result<Value> {
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
            .map(|key| json!({"key": key}))
            .collect::<Vec<_>>();
        Ok(json!({
            "count": list.len(),
            "entries": list
        }))
    }
}

fn make_test_manager(
    agents: Vec<(&str, &str)>,
    mock_steps: Vec<MockStep>,
) -> Arc<dyn SubagentManager> {
    let (tx, rx) = mpsc::channel(32);
    let tracker = Arc::new(SubagentTracker::new(tx, rx));
    let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agents(agents));
    let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
    let tool_registry = Arc::new(ToolRegistry::new());
    let config = SubagentConfig {
        max_parallel_agents: 20,
        subagent_timeout_secs: 10,
        max_iterations: 5,
        max_depth: 1,
    };
    Arc::new(SubagentManagerImpl::new(
        tracker,
        definitions,
        llm_client,
        tool_registry,
        config,
    ))
}

struct FailingSpawnManager {
    inner: Arc<dyn SubagentManager>,
    fail_on_attempt: usize,
    attempts: Mutex<usize>,
}

impl FailingSpawnManager {
    fn new(inner: Arc<dyn SubagentManager>, fail_on_attempt: usize) -> Self {
        Self {
            inner,
            fail_on_attempt,
            attempts: Mutex::new(0),
        }
    }
}

#[async_trait]
impl SubagentManager for FailingSpawnManager {
    fn spawn(
        &self,
        request: ContractSubagentSpawnRequest,
    ) -> std::result::Result<SpawnHandle, ToolError> {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *attempts += 1;
        if *attempts == self.fail_on_attempt {
            return Err(ToolError::Tool("Injected spawn failure".to_string()));
        }
        self.inner.spawn(request)
    }

    fn list_callable(&self) -> Vec<SubagentDefSummary> {
        self.inner.list_callable()
    }

    fn list_running(&self) -> Vec<SubagentState> {
        self.inner.list_running()
    }

    fn running_count(&self) -> usize {
        self.inner.running_count()
    }

    async fn wait(&self, task_id: &str) -> Option<SubagentCompletion> {
        self.inner.wait(task_id).await
    }

    async fn wait_for_parent_owned_task(
        &self,
        task_id: &str,
        parent_run_id: &str,
    ) -> Option<SubagentCompletion> {
        self.inner
            .wait_for_parent_owned_task(task_id, parent_run_id)
            .await
    }

    fn config(&self) -> &SubagentConfig {
        self.inner.config()
    }
}

#[tokio::test]
async fn test_spawn_batch_waits_for_all_instances() {
    let manager = make_test_manager(
        vec![("coder", "Coder")],
        vec![
            MockStep::text("done-1"),
            MockStep::text("done-2"),
            MockStep::text("done-3"),
        ],
    );
    let tool = SpawnSubagentBatchTool::new(manager);
    let output = tool
        .execute(json!({
            "operation": "spawn",
            "task": "Implement fixes",
            "wait": true,
            "specs": [
                { "agent": "coder", "count": 3 }
            ]
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert_eq!(output.result["status"], "completed");
    assert_eq!(output.result["spawned_count"], 3);
    assert_eq!(output.result["results"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_spawn_batch_rejects_provider_without_model() {
    let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);
    let result = tool
        .execute(json!({
            "operation": "spawn",
            "task": "Implement fixes",
            "specs": [
                { "agent": "coder", "count": 1, "provider": "openai-codex" }
            ]
        }))
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("requires both 'model' and 'provider'")
    );
}

#[tokio::test]
async fn test_spawn_batch_supports_distinct_tasks_list() {
    let manager = make_test_manager(
        vec![("coder", "Coder")],
        vec![
            MockStep::text("done-1"),
            MockStep::text("done-2"),
            MockStep::text("done-3"),
        ],
    );
    let tool = SpawnSubagentBatchTool::new(manager);
    let output = tool
        .execute(json!({
            "operation": "spawn",
            "wait": true,
            "specs": [
                { "agent": "coder", "tasks": ["task-1", "task-2", "task-3"] }
            ]
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert_eq!(output.result["status"], "completed");
    assert_eq!(output.result["spawned_count"], 3);
    let results = output.result["results"]
        .as_array()
        .expect("results should be array");
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|entry| entry["status"] == "completed"));
}

#[tokio::test]
async fn test_spawn_batch_rejects_task_and_tasks_together() {
    let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "specs": [
                { "agent": "coder", "task": "single", "tasks": ["task-1"] }
            ]
        }))
        .await;

    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("either 'task' or 'tasks'") || message.contains("both 'task' and 'tasks'")
    );
}

#[tokio::test]
async fn test_spawn_batch_rejects_tasks_count_mismatch() {
    let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "specs": [
                { "agent": "coder", "count": 2, "tasks": ["task-1", "task-2", "task-3"] }
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

#[tokio::test]
async fn test_spawn_batch_rejects_team_and_specs_combined() {
    let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "team": "Team1",
            "specs": [
                { "agent": "coder", "count": 1 }
            ],
            "task": "Implement fixes"
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
async fn test_spawn_batch_requires_task_when_spec_has_no_override() {
    let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
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
async fn test_spawn_batch_rejects_when_requested_instances_exceed_slots() {
    let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "task": "Implement fixes",
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
async fn test_spawn_batch_save_as_team_requires_store() {
    let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentBatchTool::new(manager);

    let result = tool
        .execute(json!({
            "operation": "spawn",
            "task": "Implement fixes",
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
async fn test_spawn_batch_save_as_team_persists_team() {
    let manager = make_test_manager(
        vec![("coder", "Coder")],
        vec![MockStep::text("done-1"), MockStep::text("done-2")],
    );
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

    let spawn_output = tool
        .execute(json!({
            "operation": "spawn",
            "wait": true,
            "save_as_team": "SavedTeam",
            "specs": [
                { "agent": "coder", "tasks": ["task-1", "task-2"] }
            ]
        }))
        .await
        .unwrap();
    assert!(spawn_output.success);
    assert_eq!(spawn_output.result["saved_team"], "SavedTeam");

    let get_output = tool
        .execute(json!({
            "operation": "get_team",
            "team": "SavedTeam"
        }))
        .await
        .unwrap();
    assert!(get_output.success);
    assert_eq!(get_output.result["team"], "SavedTeam");
    assert_eq!(get_output.result["total_instances"], 2);
    let spec = get_output.result["members"][0].clone();
    assert_eq!(spec["count"], 2);
    assert!(spec.get("task").is_none() || spec["task"].is_null());
    assert!(spec.get("tasks").is_none() || spec["tasks"].is_null());
}

#[tokio::test]
async fn test_spawn_batch_returns_spawned_tasks_on_partial_failure() {
    let inner = make_test_manager(
        vec![("coder", "Coder")],
        vec![MockStep::text("done-1"), MockStep::text("done-2")],
    );
    let manager: Arc<dyn SubagentManager> = Arc::new(FailingSpawnManager::new(inner, 2));
    let tool = SpawnSubagentBatchTool::new(manager);

    let output = tool
        .execute(json!({
            "operation": "spawn",
            "task": "Implement fixes",
            "specs": [
                { "agent": "coder", "count": 2 }
            ]
        }))
        .await
        .expect("partial failure should still produce output");

    assert!(output.success);
    assert_eq!(output.result["status"], "partial_failure");
    assert_eq!(output.result["spawned_count"], 1);
    assert_eq!(output.result["failed_spec_index"], 0);
    assert_eq!(output.result["failed_instance_index"], 1);
    assert!(
        output.result["error"]
            .as_str()
            .expect("error message")
            .contains("Injected spawn failure")
    );
    let task_ids = output.result["task_ids"]
        .as_array()
        .expect("task_ids should be array");
    assert_eq!(task_ids.len(), 1);
    let tasks = output.result["tasks"]
        .as_array()
        .expect("tasks should be array");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["instance_index"], 0);
}

#[tokio::test]
async fn test_team_lifecycle_and_spawn_from_team() {
    let manager = make_test_manager(
        vec![("coder", "Coder")],
        vec![MockStep::text("done-1"), MockStep::text("done-2")],
    );
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

    let save_output = tool
        .execute(json!({
            "operation": "save_team",
            "team": "Team1",
            "specs": [
                { "agent": "coder", "count": 2 }
            ]
        }))
        .await
        .unwrap();
    assert!(save_output.success);
    assert_eq!(save_output.result["operation"], "save_team");

    let list_output = tool
        .execute(json!({"operation": "list_teams"}))
        .await
        .unwrap();
    assert!(list_output.success);
    let teams = list_output.result["teams"].as_array().unwrap();
    assert!(teams.iter().any(|entry| entry["team"] == "Team1"));

    let get_output = tool
        .execute(json!({"operation": "get_team", "team": "Team1"}))
        .await
        .unwrap();
    assert!(get_output.success);
    assert_eq!(get_output.result["team"], "Team1");
    assert_eq!(get_output.result["member_groups"], 1);
    assert_eq!(get_output.result["total_instances"], 2);

    let spawn_output = tool
        .execute(json!({
            "operation": "spawn",
            "team": "Team1",
            "task": "Run review",
            "wait": true
        }))
        .await
        .unwrap();
    assert!(spawn_output.success);
    assert_eq!(spawn_output.result["spawned_count"], 2);

    let delete_output = tool
        .execute(json!({"operation": "delete_team", "team": "Team1"}))
        .await
        .unwrap();
    assert!(delete_output.success);
    assert_eq!(delete_output.result["operation"], "delete_team");
}

#[tokio::test]
async fn test_save_team_rejects_unknown_agent_reference() {
    let manager = make_test_manager(vec![("coder", "Coder")], vec![]);
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

    let result = tool
        .execute(json!({
            "operation": "save_team",
            "team": "BrokenTeam",
            "specs": [
                { "agent": "missing-agent", "count": 1 }
            ]
        }))
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown agent"));
}

#[tokio::test]
async fn test_get_team_rejects_legacy_specs_payload() {
    let manager = make_test_manager(vec![("coder", "Coder")], vec![]);
    let kv_store = Arc::new(MockKvStore::default());
    kv_store
        .set_entry(
            "subagent_team:LegacyTeam",
            &json!({
                "version": 1,
                "name": "LegacyTeam",
                "specs": [
                    {
                        "agent": "coder",
                        "tasks": ["task-1", "task-2"]
                    }
                ],
                "created_at": 1,
                "updated_at": 2
            })
            .to_string(),
            None,
            None,
            None,
            None,
            None,
        )
        .expect("store legacy team");
    let kv_store: Arc<dyn KvStore> = kv_store;
    let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

    let error = tool
        .execute(json!({"operation": "get_team", "team": "LegacyTeam"}))
        .await
        .expect_err("legacy payload should fail to decode");

    assert!(
        error
            .to_string()
            .contains("Failed to decode team 'LegacyTeam'")
    );
}

#[test]
fn test_batch_params_accept_legacy_confirmation_token_alias() {
    let params: SpawnSubagentBatchParams =
        serde_json::from_str(r#"{"team":"reviewers","confirmation_token":"approval-1"}"#)
            .expect("params should deserialize");
    assert_eq!(params.approval_id.as_deref(), Some("approval-1"));
}
