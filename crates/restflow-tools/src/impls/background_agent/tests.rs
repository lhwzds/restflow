use super::*;
use async_trait::async_trait;
use restflow_traits::assessment::{
    AgentOperationAssessor, OperationAssessment, OperationAssessmentIntent,
};
use restflow_traits::store::{
    BackgroundAgentControlRequest, BackgroundAgentConvertSessionRequest,
    BackgroundAgentCreateRequest, BackgroundAgentDeliverableListRequest,
    BackgroundAgentMessageListRequest, BackgroundAgentMessageRequest,
    BackgroundAgentProgressRequest, BackgroundAgentStore, BackgroundAgentTraceListRequest,
    BackgroundAgentTraceReadRequest, BackgroundAgentUpdateRequest, KvStore,
    MANAGE_BACKGROUND_AGENT_OPERATIONS_CSV,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;

struct MockStore;
struct FailingListStore;
struct MockAssessor;
#[derive(Default)]
struct MockKvStore {
    entries: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl AgentOperationAssessor for MockAssessor {
    async fn assess_agent_create(
        &self,
        _request: restflow_traits::store::AgentCreateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "create_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_agent_update(
        &self,
        _request: restflow_traits::store::AgentUpdateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "update_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_background_agent_create(
        &self,
        _request: BackgroundAgentCreateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "create_background_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_background_agent_convert_session(
        &self,
        _request: BackgroundAgentConvertSessionRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "convert_session_to_background_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_background_agent_update(
        &self,
        _request: BackgroundAgentUpdateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "update_background_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_background_agent_control(
        &self,
        _request: BackgroundAgentControlRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "control_background_agent",
            OperationAssessmentIntent::Run,
        ))
    }

    async fn assess_background_agent_template(
        &self,
        operation: &str,
        intent: OperationAssessmentIntent,
        _agent_ids: Vec<String>,
        _template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(operation, intent))
    }

    async fn assess_subagent_spawn(
        &self,
        operation: &str,
        _request: restflow_contracts::request::SubagentSpawnRequest,
        _template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            operation,
            OperationAssessmentIntent::Run,
        ))
    }

    async fn assess_subagent_batch(
        &self,
        operation: &str,
        _requests: Vec<restflow_contracts::request::SubagentSpawnRequest>,
        _template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            operation,
            OperationAssessmentIntent::Run,
        ))
    }
}

fn writable_tool() -> BackgroundAgentTool {
    BackgroundAgentTool::new(Arc::new(MockStore))
        .with_write(true)
        .with_assessor(Arc::new(MockAssessor))
}

fn writable_team_tool(kv_store: Arc<dyn KvStore>) -> BackgroundAgentTool {
    BackgroundAgentTool::new(Arc::new(MockStore))
        .with_kv_store(kv_store)
        .with_write(true)
        .with_assessor(Arc::new(MockAssessor))
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
            Err(ToolError::Tool(format!("entry not found: {}", key)))
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
        Ok(json!({ "success": true, "key": key }))
    }

    fn delete_entry(&self, key: &str, _accessor_id: Option<&str>) -> Result<Value> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let deleted = entries.remove(key).is_some();
        Ok(json!({ "deleted": deleted, "key": key }))
    }

    fn list_entries(&self, namespace: Option<&str>) -> Result<Value> {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prefix = namespace.map(|value| format!("{value}:"));
        let list = entries
            .iter()
            .filter(|(key, _)| {
                prefix
                    .as_ref()
                    .map(|value| key.starts_with(value))
                    .unwrap_or(true)
            })
            .map(|(key, value)| json!({ "key": key, "value": value }))
            .collect::<Vec<_>>();
        Ok(json!({
            "count": list.len(),
            "entries": list
        }))
    }
}

impl BackgroundAgentStore for MockStore {
    fn create_background_agent(&self, request: BackgroundAgentCreateRequest) -> Result<Value> {
        if request.preview {
            return Ok(json!({
                "status": "preview",
                "assessment": {
                    "operation": "create_background_agent"
                }
            }));
        }
        Ok(json!({
            "status": "executed",
            "result": {
                "id": "task-1"
            }
        }))
    }

    fn convert_session_to_background_agent(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> Result<Value> {
        if request.preview {
            return Ok(json!({
                "status": "preview",
                "assessment": {
                    "operation": "convert_session_to_background_agent"
                }
            }));
        }
        Ok(json!({
            "status": "executed",
            "result": {
                "task": {
                    "id": "task-1",
                    "chat_session_id": request.session_id.clone(),
                    "name": request.name.unwrap_or_else(|| "converted".to_string()),
                },
                "source_session_id": request.session_id,
                "source_session_agent_id": "agent-1",
                "run_now": request.run_now.unwrap_or(false)
            },
        }))
    }

    fn update_background_agent(&self, request: BackgroundAgentUpdateRequest) -> Result<Value> {
        if request.preview {
            return Ok(json!({
                "status": "preview",
                "assessment": {
                    "operation": "update_background_agent"
                }
            }));
        }
        Ok(json!({
            "status": "executed",
            "result": {
                "id": "task-1",
                "updated": true
            }
        }))
    }

    fn delete_background_agent(&self, _id: &str) -> Result<Value> {
        Ok(json!({ "deleted": true }))
    }

    fn list_background_agents(&self, _status: Option<String>) -> Result<Value> {
        Ok(json!([{"id": "task-1"}]))
    }

    fn control_background_agent(&self, request: BackgroundAgentControlRequest) -> Result<Value> {
        if request.preview {
            return Ok(json!({
                "status": "preview",
                "assessment": {
                    "operation": "control_background_agent"
                }
            }));
        }
        Ok(json!({
            "status": "executed",
            "result": {
                "id": request.id,
                "action": request.action
            }
        }))
    }

    fn get_background_agent_progress(
        &self,
        request: BackgroundAgentProgressRequest,
    ) -> Result<Value> {
        Ok(json!({
            "id": request.id,
            "event_limit": request.event_limit.unwrap_or(10),
            "status": "active"
        }))
    }

    fn send_background_agent_message(
        &self,
        request: BackgroundAgentMessageRequest,
    ) -> Result<Value> {
        Ok(json!({
            "id": request.id,
            "message": request.message,
            "source": request.source.unwrap_or_else(|| "user".to_string())
        }))
    }

    fn list_background_agent_messages(
        &self,
        request: BackgroundAgentMessageListRequest,
    ) -> Result<Value> {
        Ok(json!([{
            "id": "msg-1",
            "task_id": request.id,
            "limit": request.limit.unwrap_or(50)
        }]))
    }

    fn list_background_agent_deliverables(
        &self,
        request: BackgroundAgentDeliverableListRequest,
    ) -> Result<Value> {
        Ok(json!([{
            "id": "d-1",
            "task_id": request.id,
            "type": "report"
        }]))
    }

    fn list_background_agent_traces(
        &self,
        request: BackgroundAgentTraceListRequest,
    ) -> Result<Value> {
        Ok(json!([{
            "id": request.id,
            "trace_id": "trace-001",
            "event_type": "tool_call_completed",
        }]))
    }

    fn read_background_agent_trace(
        &self,
        request: BackgroundAgentTraceReadRequest,
    ) -> Result<Value> {
        Ok(json!({
            "trace_id": request.trace_id,
            "line_limit": request.line_limit.unwrap_or(200),
            "events": [
                {"event_type": "turn_started"},
                {"event_type": "turn_completed"}
            ]
        }))
    }
}

impl BackgroundAgentStore for FailingListStore {
    fn create_background_agent(&self, _request: BackgroundAgentCreateRequest) -> Result<Value> {
        Ok(json!({
            "status": "executed",
            "result": { "id": "task-1" }
        }))
    }

    fn convert_session_to_background_agent(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> Result<Value> {
        Ok(json!({
            "status": "executed",
            "result": {
                "task": {
                    "id": "task-1",
                    "chat_session_id": request.session_id,
                },
                "source_session_id": "session-1",
                "source_session_agent_id": "agent-1",
                "run_now": request.run_now.unwrap_or(false)
            },
        }))
    }

    fn update_background_agent(&self, _request: BackgroundAgentUpdateRequest) -> Result<Value> {
        Ok(json!({
            "status": "executed",
            "result": { "id": "task-1", "updated": true }
        }))
    }

    fn delete_background_agent(&self, _id: &str) -> Result<Value> {
        Ok(json!({ "deleted": true }))
    }

    fn list_background_agents(&self, _status: Option<String>) -> Result<Value> {
        Err(crate::ToolError::Tool("store offline".to_string()))
    }

    fn control_background_agent(&self, request: BackgroundAgentControlRequest) -> Result<Value> {
        Ok(json!({
            "status": "executed",
            "result": { "id": request.id, "action": request.action }
        }))
    }

    fn get_background_agent_progress(
        &self,
        request: BackgroundAgentProgressRequest,
    ) -> Result<Value> {
        Ok(json!({
            "id": request.id,
            "event_limit": request.event_limit.unwrap_or(10),
            "status": "active"
        }))
    }

    fn send_background_agent_message(
        &self,
        request: BackgroundAgentMessageRequest,
    ) -> Result<Value> {
        Ok(json!({
            "id": request.id,
            "message": request.message,
            "source": request.source.unwrap_or_else(|| "user".to_string())
        }))
    }

    fn list_background_agent_messages(
        &self,
        request: BackgroundAgentMessageListRequest,
    ) -> Result<Value> {
        Ok(json!([{
            "id": "msg-1",
            "task_id": request.id,
            "limit": request.limit.unwrap_or(50)
        }]))
    }

    fn list_background_agent_deliverables(
        &self,
        request: BackgroundAgentDeliverableListRequest,
    ) -> Result<Value> {
        Ok(json!([{
            "id": "d-1",
            "task_id": request.id,
            "type": "report"
        }]))
    }

    fn list_background_agent_traces(
        &self,
        _request: BackgroundAgentTraceListRequest,
    ) -> Result<Value> {
        Ok(json!([]))
    }

    fn read_background_agent_trace(
        &self,
        request: BackgroundAgentTraceReadRequest,
    ) -> Result<Value> {
        Ok(json!({
            "trace_id": request.trace_id,
            "line_limit": request.line_limit.unwrap_or(200),
            "events": []
        }))
    }
}

#[tokio::test]
async fn test_list_tasks() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore));
    let output = tool.execute(json!({ "operation": "list" })).await.unwrap();
    assert!(output.success);
}

#[tokio::test]
async fn test_write_guard() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore));
    let result = tool
        .execute(json!({
            "operation": "create",
            "name": "A",
            "agent_id": "agent-1"
        }))
        .await;
    let err = result.expect_err("expected write-guard error");
    assert!(
        err.to_string()
            .contains("Available read-only operations: list, progress")
    );
}

#[tokio::test]
async fn test_invalid_input_message() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore));
    let output = tool
        .execute(json!({
            "id": "task-1"
        }))
        .await
        .expect("tool should return error output");
    assert!(!output.success);
    assert!(
        output
            .error
            .expect("expected error")
            .contains(MANAGE_BACKGROUND_AGENT_OPERATIONS_CSV)
    );
}

#[tokio::test]
async fn test_create_accepts_typed_background_agent_payloads() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "create",
            "name": "Scheduled Task",
            "agent_id": "agent-1",
            "schedule": {
                "type": "interval",
                "interval_ms": 60000,
                "start_at": null
            },
            "durability_mode": "async",
            "memory": {},
            "resource_limits": {}
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["status"], "executed");
}

#[tokio::test]
async fn test_create_preview_returns_store_outcome() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "create",
            "name": "Preview Task",
            "agent_id": "agent-1",
            "schedule": {
                "type": "interval",
                "interval_ms": 60000,
                "start_at": null
            },
            "preview": true
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["status"], "preview");
    assert_eq!(
        output.result["assessment"]["operation"],
        "create_background_agent"
    );
}

#[tokio::test]
async fn test_create_rejects_invalid_durability_mode_payload() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore)).with_write(true);
    let output = tool
        .execute(json!({
            "operation": "create",
            "name": "Broken Task",
            "agent_id": "agent-1",
            "durability_mode": "broken"
        }))
        .await
        .expect("tool should return structured error output");
    assert!(!output.success);
    assert!(
        output
            .error
            .expect("expected error")
            .contains("Invalid input")
    );
}

#[tokio::test]
async fn test_convert_session_operation() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "convert_session",
            "session_id": "session-1",
            "name": "Converted Task",
            "run_now": true
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(
        output
            .result
            .get("result")
            .and_then(|result| result.get("task"))
            .and_then(|task| task.get("chat_session_id"))
            .and_then(|value| value.as_str()),
        Some("session-1")
    );
    assert_eq!(
        output
            .result
            .get("result")
            .and_then(|result| result.get("run_now"))
            .and_then(|value| value.as_bool()),
        Some(true)
    );
}

#[tokio::test]
async fn test_promote_to_background_operation() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "promote_to_background",
            "session_id": "session-1",
            "name": "Promoted Task",
            "run_now": false
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(
        output
            .result
            .get("result")
            .and_then(|result| result.get("task"))
            .and_then(|task| task.get("chat_session_id"))
            .and_then(|value| value.as_str()),
        Some("session-1")
    );
    assert_eq!(
        output
            .result
            .get("result")
            .and_then(|result| result.get("run_now"))
            .and_then(|value| value.as_bool()),
        Some(false)
    );
}

#[tokio::test]
async fn test_convert_session_defaults_run_now_to_false() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "convert_session",
            "session_id": "session-1",
            "name": "Converted Task"
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(
        output
            .result
            .get("result")
            .and_then(|result| result.get("run_now"))
            .and_then(|value| value.as_bool()),
        Some(false)
    );
}

#[tokio::test]
async fn test_promote_to_background_requires_session_id() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore)).with_write(true);
    let err = tool
        .execute(json!({
            "operation": "promote_to_background"
        }))
        .await
        .expect_err("expected missing session_id error");
    assert!(
        err.to_string()
            .contains("promote_to_background requires session_id")
    );
}

#[tokio::test]
async fn test_list_store_error_is_wrapped() {
    let tool = BackgroundAgentTool::new(Arc::new(FailingListStore));
    let result = tool.execute(json!({ "operation": "list" })).await;
    let err = result.expect_err("expected wrapped store error");
    let err_text = err.to_string();
    assert!(err_text.contains("Failed to list background agent"));
    assert!(err_text.contains("store offline"));
}

#[tokio::test]
async fn test_progress_operation() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore));
    let output = tool
        .execute(json!({
            "operation": "progress",
            "id": "task-1",
            "event_limit": 5
        }))
        .await
        .unwrap();
    assert!(output.success);
}

#[tokio::test]
async fn test_list_deliverables_operation() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore));
    let output = tool
        .execute(json!({
            "operation": "list_deliverables",
            "id": "task-1"
        }))
        .await
        .unwrap();
    assert!(output.success);
}

#[tokio::test]
async fn test_list_traces_operation() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore));
    let output = tool
        .execute(json!({
            "operation": "list_traces",
            "id": "task-1",
            "limit": 5
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result.as_array().map(|items| items.len()), Some(1));
}

#[tokio::test]
async fn test_read_trace_operation() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore));
    let output = tool
        .execute(json!({
            "operation": "read_trace",
            "trace_id": "trace-task-1-20260214-000000",
            "line_limit": 2
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(
        output
            .result
            .get("trace_id")
            .and_then(|value| value.as_str()),
        Some("trace-task-1-20260214-000000")
    );
}

#[tokio::test]
async fn test_stop_uses_control_not_delete() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore)).with_write(true);
    let output = tool
        .execute(json!({
            "operation": "stop",
            "id": "task-1"
        }))
        .await
        .unwrap();
    assert!(output.success);
    // Stop should call control_background_agent with action "stop", not delete
    // MockStore returns { id, action } for control operations
    assert_eq!(
        output
            .result
            .get("result")
            .and_then(|result| result.get("action"))
            .and_then(|v| v.as_str()),
        Some("stop")
    );
}

#[tokio::test]
async fn test_start_uses_control_with_start_action() {
    let tool = BackgroundAgentTool::new(Arc::new(MockStore)).with_write(true);
    let output = tool
        .execute(json!({
            "operation": "start",
            "id": "task-1"
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(
        output
            .result
            .get("result")
            .and_then(|result| result.get("action"))
            .and_then(|v| v.as_str()),
        Some("start")
    );
}

#[tokio::test]
async fn test_run_batch_with_mixed_input_modes() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "run_batch",
            "agent_id": "agent-1",
            "input": "fallback input",
            "workers": [
                { "count": 2 },
                { "inputs": ["task-a", "task-b"] }
            ]
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["operation"], "run_batch");
    assert_eq!(output.result["total"], 4);
    assert_eq!(output.result["run_now"], false);
    assert_eq!(
        output.result["tasks"].as_array().map(|items| items.len()),
        Some(4)
    );
}

#[tokio::test]
async fn test_run_batch_defaults_run_now_to_false() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "run_batch",
            "agent_id": "agent-1",
            "input": "save-only input",
            "workers": [
                { "count": 1 }
            ]
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["run_now"], false);
    assert_eq!(output.result["tasks"][0]["run_now"], false);
}

#[tokio::test]
async fn test_team_management_round_trip() {
    let kv_store = Arc::new(MockKvStore::default());
    let tool = writable_team_tool(kv_store);

    let save = tool
        .execute(json!({
            "operation": "save_team",
            "team": "TeamA",
            "workers": [
                { "agent_id": "agent-1", "count": 2 }
            ]
        }))
        .await
        .unwrap();
    assert!(save.success);
    assert_eq!(save.result["operation"], "save_team");

    let list = tool
        .execute(json!({
            "operation": "list_teams"
        }))
        .await
        .unwrap();
    assert!(list.success);
    assert_eq!(list.result["operation"], "list_teams");
    assert_eq!(
        list.result["teams"].as_array().map(|items| items.len()),
        Some(1)
    );

    let get = tool
        .execute(json!({
            "operation": "get_team",
            "team": "TeamA"
        }))
        .await
        .unwrap();
    assert!(get.success);
    assert_eq!(get.result["operation"], "get_team");
    assert_eq!(get.result["team"], "TeamA");
    assert_eq!(get.result["member_groups"], 1);
    assert_eq!(get.result["total_instances"], 2);
    assert_eq!(
        get.result["members"].as_array().map(|items| items.len()),
        Some(1)
    );
    assert!(
        get.result["members"][0].get("input").is_none()
            || get.result["members"][0]["input"].is_null()
    );

    let delete = tool
        .execute(json!({
            "operation": "delete_team",
            "team": "TeamA"
        }))
        .await
        .unwrap();
    assert!(delete.success);
    assert_eq!(delete.result["operation"], "delete_team");
}

#[tokio::test]
async fn test_run_batch_from_saved_team() {
    let kv_store = Arc::new(MockKvStore::default());
    let tool = writable_team_tool(kv_store);

    tool.execute(json!({
        "operation": "save_team",
        "team": "TeamB",
        "workers": [
            { "agent_id": "agent-1", "count": 2 }
        ]
    }))
    .await
    .unwrap();

    let output = tool
        .execute(json!({
            "operation": "run_batch",
            "team": "TeamB",
            "inputs": ["alpha", "beta"]
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["operation"], "run_batch");
    assert_eq!(output.result["total"], 2);
}

#[tokio::test]
async fn test_run_batch_save_as_team_strips_runtime_inputs() {
    let kv_store = Arc::new(MockKvStore::default());
    let tool = writable_team_tool(kv_store);

    let saved = tool
        .execute(json!({
            "operation": "run_batch",
            "save_as_team": "TeamC",
            "agent_id": "agent-1",
            "workers": [
                { "count": 2, "inputs": ["alpha", "beta"] }
            ]
        }))
        .await
        .unwrap();
    assert!(saved.success);

    let get = tool
        .execute(json!({
            "operation": "get_team",
            "team": "TeamC"
        }))
        .await
        .unwrap();
    assert!(get.success);
    assert!(
        get.result["members"][0].get("inputs").is_none()
            || get.result["members"][0]["inputs"].is_null()
    );
    assert_eq!(get.result["members"][0]["count"], 2);
}

#[tokio::test]
async fn test_run_batch_rejects_workers_and_team_combined() {
    let kv_store = Arc::new(MockKvStore::default());
    let tool = writable_team_tool(kv_store);

    let result = tool
        .execute(json!({
            "operation": "run_batch",
            "team": "TeamD",
            "workers": [
                { "agent_id": "agent-1", "count": 1 }
            ],
            "input": "task"
        }))
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("either 'workers' or 'team'")
    );
}

#[tokio::test]
async fn test_save_team_rejects_runtime_input_fields() {
    let kv_store = Arc::new(MockKvStore::default());
    let tool = writable_team_tool(kv_store);

    let result = tool
        .execute(json!({
            "operation": "save_team",
            "team": "PromptfulTeam",
            "workers": [
                { "agent_id": "agent-1", "input": "do work" }
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

#[tokio::test]
async fn test_get_team_rejects_legacy_worker_payload() {
    let kv_store = Arc::new(MockKvStore::default());
    kv_store
        .set_entry(
            "background_agent_team:LegacyTeam",
            &json!({
                "version": 1,
                "name": "LegacyTeam",
                "workers": [
                    {
                        "agent_id": "agent-1",
                        "inputs": ["alpha", "beta"]
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
    let tool = BackgroundAgentTool::new(Arc::new(MockStore))
        .with_kv_store(kv_store)
        .with_write(true)
        .with_assessor(Arc::new(MockAssessor));

    let error = tool
        .execute(json!({
            "operation": "get_team",
            "team": "LegacyTeam"
        }))
        .await
        .expect_err("legacy payload should fail to decode");

    assert!(
        error
            .to_string()
            .contains("Failed to decode team 'LegacyTeam'")
    );
}
