use super::*;
use crate::Tool;
use restflow_ai::agent::{
    SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
    SubagentManagerImpl, SubagentTracker,
};
use restflow_ai::llm::{MockLlmClient, MockStep};
use restflow_ai::tools::ToolRegistry;
use restflow_traits::store::KvStore;
use restflow_traits::{
    AgentOperationAssessor, OperationAssessment, OperationAssessmentIntent,
    OperationAssessmentIssue, normalize_legacy_approval_replay,
};
use restflow_traits::{DEFAULT_SUBAGENT_TIMEOUT_SECS, SubagentManager};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::impls::spawn_subagent_batch::SpawnSubagentBatchOperation;

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
    fn get_entry(&self, key: &str) -> crate::Result<Value> {
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
    ) -> crate::Result<Value> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.insert(key.to_string(), content.to_string());
        Ok(json!({"success": true, "key": key}))
    }

    fn delete_entry(&self, key: &str, _accessor_id: Option<&str>) -> crate::Result<Value> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let deleted = entries.remove(key).is_some();
        Ok(json!({"deleted": deleted, "key": key}))
    }

    fn list_entries(&self, namespace: Option<&str>) -> crate::Result<Value> {
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

fn make_test_deps(
    agents: Vec<(&str, &str)>,
    mock_steps: Vec<MockStep>,
) -> Arc<dyn SubagentManager> {
    let (tx, rx) = mpsc::channel(16);
    let tracker = Arc::new(SubagentTracker::new(tx, rx));
    let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agents(agents));
    let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
    let tool_registry = Arc::new(ToolRegistry::new());
    let config = SubagentConfig {
        max_parallel_agents: 5,
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

struct WarningAssessor;

#[async_trait]
impl AgentOperationAssessor for WarningAssessor {
    async fn assess_agent_create(
        &self,
        _request: restflow_traits::store::AgentCreateRequest,
    ) -> crate::Result<OperationAssessment> {
        unreachable!("unused in this test")
    }

    async fn assess_agent_update(
        &self,
        _request: restflow_traits::store::AgentUpdateRequest,
    ) -> crate::Result<OperationAssessment> {
        unreachable!("unused in this test")
    }

    async fn assess_background_agent_create(
        &self,
        _request: restflow_traits::store::BackgroundAgentCreateRequest,
    ) -> crate::Result<OperationAssessment> {
        unreachable!("unused in this test")
    }

    async fn assess_background_agent_convert_session(
        &self,
        _request: restflow_traits::store::BackgroundAgentConvertSessionRequest,
    ) -> crate::Result<OperationAssessment> {
        unreachable!("unused in this test")
    }

    async fn assess_background_agent_update(
        &self,
        _request: restflow_traits::store::BackgroundAgentUpdateRequest,
    ) -> crate::Result<OperationAssessment> {
        unreachable!("unused in this test")
    }

    async fn assess_background_agent_delete(
        &self,
        _request: restflow_traits::store::BackgroundAgentDeleteRequest,
    ) -> crate::Result<OperationAssessment> {
        unreachable!("unused in this test")
    }

    async fn assess_background_agent_control(
        &self,
        _request: restflow_traits::store::BackgroundAgentControlRequest,
    ) -> crate::Result<OperationAssessment> {
        unreachable!("unused in this test")
    }

    async fn assess_background_agent_template(
        &self,
        _operation: &str,
        _intent: OperationAssessmentIntent,
        _agent_ids: Vec<String>,
        _template_mode: bool,
    ) -> crate::Result<OperationAssessment> {
        unreachable!("unused in this test")
    }

    async fn assess_subagent_spawn(
        &self,
        _operation: &str,
        _request: restflow_traits::subagent::ContractSubagentSpawnRequest,
        _template_mode: bool,
    ) -> crate::Result<OperationAssessment> {
        Ok(OperationAssessment::warning_with_confirmation(
            "spawn_subagent",
            OperationAssessmentIntent::Run,
            vec![OperationAssessmentIssue {
                code: "review".to_string(),
                message: "Review this batch before spawning.".to_string(),
                field: None,
                suggestion: None,
            }],
        ))
    }

    async fn assess_subagent_batch(
        &self,
        _operation: &str,
        _requests: Vec<restflow_traits::subagent::ContractSubagentSpawnRequest>,
        _template_mode: bool,
    ) -> crate::Result<OperationAssessment> {
        unreachable!("unused in this test")
    }
}

#[test]
fn test_params_deserialization() {
    let json = r#"{"agent": "researcher", "task": "Research topic X"}"#;
    let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.operation, SpawnSubagentBatchOperation::Spawn);
    assert_eq!(params.agent.as_deref(), Some("researcher"));
    assert_eq!(params.task.as_deref(), Some("Research topic X"));
    assert!(params.tasks.is_none());
    assert!(!params.wait);
}

#[test]
fn test_params_with_wait() {
    let json =
        r#"{"agent": "coder", "task": "Write function Y", "wait": true, "timeout_secs": 600}"#;
    let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.agent.as_deref(), Some("coder"));
    assert_eq!(params.task.as_deref(), Some("Write function Y"));
    assert!(params.wait);
    assert_eq!(params.timeout_secs, Some(600));
}

#[test]
fn test_params_with_model_and_provider() {
    let json = r#"{"agent":"coder","task":"Write function","model":"gpt-5.3-codex","provider":"openai-codex"}"#;
    let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.model.as_deref(), Some("gpt-5.3-codex"));
    assert_eq!(params.provider.as_deref(), Some("openai-codex"));
}

#[test]
fn test_params_with_team_operation() {
    let json = r#"{"operation":"save_team","team":"TeamOnly","workers":[{"count":2}]}"#;
    let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.operation, SpawnSubagentBatchOperation::SaveTeam);
    assert_eq!(params.team.as_deref(), Some("TeamOnly"));
    assert!(params.task.is_none());
    assert!(params.tasks.is_none());
}

#[test]
fn test_params_accept_legacy_confirmation_token_alias() {
    let mut value = json!({"task":"Review work","confirmation_token":"approval-1"});
    normalize_legacy_approval_replay(&mut value);
    let params: SpawnSubagentParams = serde_json::from_value(value).unwrap();
    assert_eq!(params.approval_id.as_deref(), Some("approval-1"));
}

#[tokio::test]
async fn test_spawn_subagent_background() {
    let deps = make_test_deps(
        vec![("researcher", "Researcher")],
        vec![MockStep::text("research done")],
    );
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({"agent": "researcher", "task": "Find info", "wait": false}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.result["status"], "spawned");
    assert!(result.result["task_id"].as_str().is_some());
}

#[tokio::test]
async fn test_spawn_subagent_wait_success() {
    let deps = make_test_deps(
        vec![("coder", "Coder")],
        vec![MockStep::text("function written")],
    );
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 10}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.result["status"], "completed");
    assert!(
        result.result["output"]
            .as_str()
            .unwrap()
            .contains("function written")
    );
}

#[tokio::test]
async fn test_spawn_subagent_wait_failure() {
    let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::error("LLM error")]);
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 10}))
        .await
        .unwrap();
    assert!(result.success); // ToolOutput is success, but status indicates failure
    assert_eq!(result.result["status"], "failed");
    assert!(result.result["error"].as_str().is_some());
}

#[tokio::test]
async fn test_spawn_subagent_wait_timeout_returns_task_id() {
    let deps = make_test_deps(
        vec![("coder", "Coder")],
        vec![MockStep::text("slow").with_delay(2_000)],
    );
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 1}))
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.result["status"], "timeout");
    assert!(result.result["task_id"].as_str().is_some());
}

#[tokio::test]
async fn test_spawn_subagent_returns_pending_approval_when_assessment_requires_confirmation() {
    let deps = make_test_deps(
        vec![("coder", "Coder")],
        vec![MockStep::text("function written")],
    );
    let tool = SpawnSubagentTool::new(deps).with_assessor(Arc::new(WarningAssessor));

    let result = tool
        .execute(json!({"agent": "coder", "task": "Review code"}))
        .await
        .expect("tool should return structured pending approval");

    assert!(!result.success);
    assert_eq!(result.result["pending_approval"], true);
    assert!(result.result["approval_id"].as_str().is_some());
}

#[tokio::test]
async fn test_spawn_subagent_unknown_agent() {
    let deps = make_test_deps(vec![], vec![]);
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({"agent": "nonexistent", "task": "Do something"}))
        .await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("No callable sub-agents available"));
}

#[tokio::test]
async fn test_spawn_subagent_invalid_params() {
    let deps = make_test_deps(vec![], vec![]);
    let tool = SpawnSubagentTool::new(deps);
    let result = tool.execute(json!({"wait": true})).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Single spawn requires non-empty 'task'")
    );
}

#[tokio::test]
async fn test_spawn_subagent_rejects_model_without_provider() {
    let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({"agent": "coder", "task": "Write code", "model": "gpt-5.3-codex"}))
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
async fn test_spawn_subagent_rejects_provider_without_model() {
    let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({"agent": "coder", "task": "Write code", "provider": "openai-codex"}))
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
async fn test_spawn_subagent_resolves_by_name() {
    let deps = make_test_deps(
        vec![("agent-123", "Code Planner")],
        vec![MockStep::text("planned")],
    );
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({"agent": "code planner", "task": "plan task", "wait": true}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.result["status"], "completed");
}

#[tokio::test]
async fn test_spawn_subagent_without_agent_uses_temporary_mode() {
    let deps = make_test_deps(
        vec![("agent-123", "Code Planner")],
        vec![MockStep::text("planned")],
    );
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({"task": "plan task", "wait": true}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.result["status"], "completed");
}

#[tokio::test]
async fn test_spawn_subagent_rejects_inline_fields_with_agent() {
    let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({
            "agent": "coder",
            "task": "Write code",
            "inline_system_prompt": "You are temporary"
        }))
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("cannot be combined")
    );
}

#[tokio::test]
async fn test_spawn_subagent_supports_workers_list_mode() {
    let deps = make_test_deps(
        vec![("coder", "Coder")],
        vec![MockStep::text("done-1"), MockStep::text("done-2")],
    );
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({
            "task": "batch task",
            "wait": true,
            "workers": [
                { "agent": "coder", "count": 2 }
            ]
        }))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.result["status"], "completed");
    assert_eq!(result.result["spawned_count"], 2);
}

#[tokio::test]
async fn test_spawn_subagent_rejects_mixed_single_and_workers_mode_fields() {
    let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({
            "task": "batch task",
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
async fn test_spawn_subagent_workers_support_distinct_tasks_list() {
    let deps = make_test_deps(
        vec![("coder", "Coder")],
        vec![MockStep::text("done-a"), MockStep::text("done-b")],
    );
    let tool = SpawnSubagentTool::new(deps);
    let result = tool
        .execute(json!({
            "task": "",
            "wait": true,
            "workers": [
                { "agent": "coder", "tasks": ["task-A", "task-B"] }
            ]
        }))
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.result["status"], "completed");
    assert_eq!(result.result["spawned_count"], 2);
    let results = result.result["results"]
        .as_array()
        .expect("results should be array");
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|entry| entry["status"] == "completed"));
}

#[tokio::test]
async fn test_spawn_subagent_team_supports_runtime_tasks_list() {
    let deps = make_test_deps(
        vec![("coder", "Coder")],
        vec![MockStep::text("done-a"), MockStep::text("done-b")],
    );
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentTool::new(deps).with_kv_store(kv_store);

    let saved = tool
        .execute(json!({
            "operation": "save_team",
            "team": "RuntimeTasksTeam",
            "workers": [
                { "agent": "coder", "count": 2 }
            ]
        }))
        .await
        .unwrap();
    assert!(saved.success);

    let result = tool
        .execute(json!({
            "team": "RuntimeTasksTeam",
            "tasks": ["task-a", "task-b"],
            "wait": true
        }))
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.result["status"], "completed");
    assert_eq!(result.result["spawned_count"], 2);
}

#[tokio::test]
async fn test_spawn_subagent_save_team_operation_persists_without_spawning() {
    let deps = make_test_deps(
        vec![("coder", "Coder")],
        vec![MockStep::text("should-not-run")],
    );
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentTool::new(deps.clone()).with_kv_store(kv_store);

    let output = tool
        .execute(json!({
            "operation": "save_team",
            "team": "TeamOnly",
            "workers": [
                { "agent": "coder", "count": 2 }
            ]
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert_eq!(output.result["operation"], "save_team");
    assert_eq!(deps.running_count(), 0);

    let reuse = tool
        .execute(json!({
            "task": "Use saved team",
            "wait": true,
            "team": "TeamOnly"
        }))
        .await
        .unwrap();

    assert!(reuse.success);
    assert_eq!(reuse.result["spawned_count"], 2);
}

#[tokio::test]
async fn test_spawn_subagent_save_team_rejects_prompt_fields() {
    let deps = make_test_deps(vec![("coder", "Coder")], vec![]);
    let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
    let tool = SpawnSubagentTool::new(deps).with_kv_store(kv_store);

    let result = tool
        .execute(json!({
            "operation": "save_team",
            "team": "PromptfulTeam",
            "workers": [
                { "agent": "coder", "task": "Should not persist" }
            ]
        }))
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("stores worker structure only")
    );
}

#[test]
fn test_parameters_schema_uses_dynamic_agent_ids() {
    let deps = make_test_deps(
        vec![("agent-1", "Researcher"), ("agent-2", "Coder")],
        vec![],
    );
    let tool = SpawnSubagentTool::new(deps);
    let schema = tool.parameters_schema();
    let values = schema["properties"]["agent"]["enum"]
        .as_array()
        .expect("agent enum should exist");
    let ids = values
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert!(ids.contains(&"agent-1"));
    assert!(ids.contains(&"agent-2"));
    assert_eq!(
        schema["properties"]["timeout_secs"]["default"],
        json!(DEFAULT_SUBAGENT_TIMEOUT_SECS)
    );
}
