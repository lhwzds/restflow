use super::*;
use crate::Tool;
use async_trait::async_trait;
use restflow_traits::assessment::{
    AgentOperationAssessor, OperationAssessment, OperationAssessmentIntent,
};
use restflow_traits::store::{
    MANAGE_TASK_OPERATIONS_CSV, TaskArtifactListRequest, TaskControlRequest,
    TaskConvertSessionRequest, TaskCreateRequest, TaskDeleteRequest, TaskMessageListRequest,
    TaskMessageRequest, TaskProgressRequest, TaskStore, TaskTraceListRequest, TaskTraceReadRequest,
    TaskUpdateRequest,
};
use serde_json::json;
use std::sync::Mutex;

struct MockStore;
struct FailingListStore;
struct MockAssessor;
#[derive(Default)]
struct ConfirmationCreateStore {
    create_calls: Mutex<usize>,
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

    async fn assess_task_create(
        &self,
        _request: TaskCreateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "create_task",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_task_convert_session(
        &self,
        _request: TaskConvertSessionRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "convert_session_to_task",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_task_update(
        &self,
        _request: TaskUpdateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "update_task",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_task_delete(
        &self,
        _request: TaskDeleteRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::warning_with_confirmation(
            "delete_task",
            OperationAssessmentIntent::Save,
            vec![],
        ))
    }

    async fn assess_task_control(
        &self,
        _request: TaskControlRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            "control_task",
            OperationAssessmentIntent::Run,
        ))
    }

    async fn assess_task_template(
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
        _request: restflow_traits::request::RunSpawnRequest,
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
        _requests: Vec<restflow_traits::request::RunSpawnRequest>,
        _template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        Ok(OperationAssessment::ok(
            operation,
            OperationAssessmentIntent::Run,
        ))
    }
}

fn writable_tool() -> TaskTool {
    TaskTool::new(Arc::new(MockStore))
        .with_write(true)
        .with_assessor(Arc::new(MockAssessor))
}

#[test]
fn task_tool_from_task_store_keeps_canonical_store() {
    let task_store: Arc<dyn TaskStore> = Arc::new(MockStore);
    let tool = TaskTool::from_task_store(task_store);

    let _: Arc<dyn TaskStore> = tool.store.clone();
}

#[test]
fn task_tool_constructor_preserves_canonical_store() {
    let tool = TaskTool::new(Arc::new(MockStore));

    let _: Arc<dyn TaskStore> = tool.store.clone();
}

#[test]
fn task_tool_uses_manage_tasks_name() {
    let tool = TaskTool::new(Arc::new(MockStore));
    assert_eq!(Tool::name(&tool), "manage_tasks");
}

#[test]
fn task_tool_schema_is_openai_function_compatible() {
    let schema = tool_parameters_schema();

    assert_eq!(schema["type"], "object");
    assert_eq!(schema["required"], json!(["operation"]));
    assert!(schema.get("oneOf").is_none());
    assert!(schema.get("anyOf").is_none());
    assert!(schema.get("allOf").is_none());
    assert!(schema.get("enum").is_none());
}

impl TaskStore for MockStore {
    fn create_task(&self, request: TaskCreateRequest) -> Result<Value> {
        if request.preview {
            return Ok(json!({
                "status": "preview",
                "assessment": {
                    "operation": "create_task"
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

    fn convert_session_to_task(&self, request: TaskConvertSessionRequest) -> Result<Value> {
        if request.preview {
            return Ok(json!({
                "status": "preview",
                "assessment": {
                    "operation": "convert_session_to_task"
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

    fn update_task(&self, request: TaskUpdateRequest) -> Result<Value> {
        if request.preview {
            return Ok(json!({
                "status": "preview",
                "assessment": {
                    "operation": "update_task"
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

    fn delete_task(&self, request: TaskDeleteRequest) -> Result<Value> {
        if request.preview {
            return Ok(json!({
                "status": "preview",
                "assessment": {
                    "operation": "delete_task",
                    "approval_id": "confirm-delete"
                }
            }));
        }
        if request.approval_id.as_deref() != Some("confirm-delete") {
            return Ok(json!({
                "status": "confirmation_required",
                "assessment": {
                    "operation": "delete_task",
                    "approval_id": "confirm-delete"
                }
            }));
        }
        Ok(json!({
            "status": "executed",
            "result": {
                "id": request.id,
                "deleted": true
            }
        }))
    }

    fn list_tasks(&self, _status: Option<String>) -> Result<Value> {
        Ok(json!([{"id": "task-1"}]))
    }

    fn control_task(&self, request: TaskControlRequest) -> Result<Value> {
        if request.preview {
            return Ok(json!({
                "status": "preview",
                "assessment": {
                    "operation": "control_task"
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

    fn get_task_progress(&self, request: TaskProgressRequest) -> Result<Value> {
        Ok(json!({
            "id": request.id,
            "event_limit": request.event_limit.unwrap_or(10),
            "status": "active"
        }))
    }

    fn send_task_message(&self, request: TaskMessageRequest) -> Result<Value> {
        Ok(json!({
            "id": request.id,
            "message": request.message,
            "source": request.source.unwrap_or_else(|| "user".to_string())
        }))
    }

    fn list_task_messages(&self, request: TaskMessageListRequest) -> Result<Value> {
        Ok(json!([{
            "id": "msg-1",
            "task_id": request.id,
            "limit": request.limit.unwrap_or(50)
        }]))
    }

    fn list_task_artifacts(&self, request: TaskArtifactListRequest) -> Result<Value> {
        Ok(json!([{
            "id": "d-1",
            "task_id": request.id,
            "type": "report"
        }]))
    }

    fn list_task_traces(&self, request: TaskTraceListRequest) -> Result<Value> {
        Ok(json!([{
            "id": request.id,
            "trace_id": "trace-001",
            "event_type": "tool_call_completed",
        }]))
    }

    fn read_task_trace(&self, request: TaskTraceReadRequest) -> Result<Value> {
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

impl TaskStore for ConfirmationCreateStore {
    fn create_task(&self, request: TaskCreateRequest) -> Result<Value> {
        let mut create_calls = self
            .create_calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *create_calls += 1;
        let sequence = *create_calls;
        if request.approval_id.as_deref() != Some("confirm-create") {
            return Ok(json!({
                "status": "confirmation_required",
                "assessment": {
                    "operation": "create_task",
                    "approval_id": "confirm-create"
                }
            }));
        }
        Ok(json!({
            "status": "executed",
            "result": {
                "id": format!("task-{sequence}")
            }
        }))
    }

    fn convert_session_to_task(&self, _request: TaskConvertSessionRequest) -> Result<Value> {
        panic!("not expected")
    }

    fn update_task(&self, _request: TaskUpdateRequest) -> Result<Value> {
        panic!("not expected")
    }

    fn delete_task(&self, _request: TaskDeleteRequest) -> Result<Value> {
        panic!("not expected")
    }

    fn list_tasks(&self, _status: Option<String>) -> Result<Value> {
        panic!("not expected")
    }

    fn control_task(&self, request: TaskControlRequest) -> Result<Value> {
        Ok(json!({
            "status": "executed",
            "result": {
                "id": request.id,
                "action": request.action
            }
        }))
    }

    fn get_task_progress(&self, _request: TaskProgressRequest) -> Result<Value> {
        panic!("not expected")
    }

    fn send_task_message(&self, _request: TaskMessageRequest) -> Result<Value> {
        panic!("not expected")
    }

    fn list_task_messages(&self, _request: TaskMessageListRequest) -> Result<Value> {
        panic!("not expected")
    }

    fn list_task_artifacts(&self, _request: TaskArtifactListRequest) -> Result<Value> {
        panic!("not expected")
    }

    fn list_task_traces(&self, _request: TaskTraceListRequest) -> Result<Value> {
        panic!("not expected")
    }

    fn read_task_trace(&self, _request: TaskTraceReadRequest) -> Result<Value> {
        panic!("not expected")
    }
}

impl TaskStore for FailingListStore {
    fn create_task(&self, _request: TaskCreateRequest) -> Result<Value> {
        Ok(json!({
            "status": "executed",
            "result": { "id": "task-1" }
        }))
    }

    fn convert_session_to_task(&self, request: TaskConvertSessionRequest) -> Result<Value> {
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

    fn update_task(&self, _request: TaskUpdateRequest) -> Result<Value> {
        Ok(json!({
            "status": "executed",
            "result": { "id": "task-1", "updated": true }
        }))
    }

    fn delete_task(&self, request: TaskDeleteRequest) -> Result<Value> {
        Ok(json!({
            "status": "executed",
            "result": { "id": request.id, "deleted": true }
        }))
    }

    fn list_tasks(&self, _status: Option<String>) -> Result<Value> {
        Err(crate::ToolError::Tool("store offline".to_string()))
    }

    fn control_task(&self, request: TaskControlRequest) -> Result<Value> {
        Ok(json!({
            "status": "executed",
            "result": { "id": request.id, "action": request.action }
        }))
    }

    fn get_task_progress(&self, request: TaskProgressRequest) -> Result<Value> {
        Ok(json!({
            "id": request.id,
            "event_limit": request.event_limit.unwrap_or(10),
            "status": "active"
        }))
    }

    fn send_task_message(&self, request: TaskMessageRequest) -> Result<Value> {
        Ok(json!({
            "id": request.id,
            "message": request.message,
            "source": request.source.unwrap_or_else(|| "user".to_string())
        }))
    }

    fn list_task_messages(&self, request: TaskMessageListRequest) -> Result<Value> {
        Ok(json!([{
            "id": "msg-1",
            "task_id": request.id,
            "limit": request.limit.unwrap_or(50)
        }]))
    }

    fn list_task_artifacts(&self, request: TaskArtifactListRequest) -> Result<Value> {
        Ok(json!([{
            "id": "d-1",
            "task_id": request.id,
            "type": "report"
        }]))
    }

    fn list_task_traces(&self, _request: TaskTraceListRequest) -> Result<Value> {
        Ok(json!([]))
    }

    fn read_task_trace(&self, request: TaskTraceReadRequest) -> Result<Value> {
        Ok(json!({
            "trace_id": request.trace_id,
            "line_limit": request.line_limit.unwrap_or(200),
            "events": []
        }))
    }
}

#[tokio::test]
async fn test_list_tasks() {
    let tool = TaskTool::new(Arc::new(MockStore));
    let output = tool.execute(json!({ "operation": "list" })).await.unwrap();
    assert!(output.success);
}

#[tokio::test]
async fn test_write_guard() {
    let tool = TaskTool::new(Arc::new(MockStore));
    let result = tool
        .execute(json!({
            "operation": "create",
            "name": "A",
            "agent_id": "agent-1",
            "schedule": {
                "type": "interval",
                "interval_ms": 60000,
                "start_at": null
            }
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
    let tool = TaskTool::new(Arc::new(MockStore));
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
            .contains(MANAGE_TASK_OPERATIONS_CSV)
    );
}

#[tokio::test]
async fn test_create_accepts_typed_task_payloads() {
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
    assert_eq!(output.result["assessment"]["operation"], "create_task");
}

#[tokio::test]
async fn test_delete_preview_returns_store_outcome() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "delete",
            "id": "task-1",
            "preview": true
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["status"], "preview");
    assert_eq!(output.result["assessment"]["operation"], "delete_task");
}

#[tokio::test]
async fn test_delete_requires_approval_id() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "delete",
            "id": "task-1"
        }))
        .await
        .unwrap();
    assert!(!output.success);
    assert_eq!(output.result["pending_approval"], true);
    assert_eq!(output.result["approval_id"], "confirm-delete");
    assert_eq!(output.result["assessment"]["approval_id"], "confirm-delete");
}

#[tokio::test]
async fn test_delete_accepts_approval_id_for_replay() {
    let tool = writable_tool();
    let output = tool
        .execute(json!({
            "operation": "delete",
            "id": "task-1",
            "approval_id": "confirm-delete"
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["result"]["id"], "task-1");
    assert_eq!(output.result["result"]["deleted"], true);
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
    let tool = TaskTool::new(Arc::new(MockStore)).with_write(true);
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
    let tool = TaskTool::new(Arc::new(FailingListStore));
    let result = tool.execute(json!({ "operation": "list" })).await;
    let err = result.expect_err("expected wrapped store error");
    let err_text = err.to_string();
    assert!(err_text.contains("Failed to list tasks"));
    assert!(err_text.contains("store offline"));
}

#[tokio::test]
async fn test_progress_operation() {
    let tool = TaskTool::new(Arc::new(MockStore));
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
async fn test_list_artifacts_operation() {
    let tool = TaskTool::new(Arc::new(MockStore));
    let output = tool
        .execute(json!({
            "operation": "list_artifacts",
            "id": "task-1"
        }))
        .await
        .unwrap();
    assert!(output.success);
}

#[tokio::test]
async fn test_list_traces_operation() {
    let tool = TaskTool::new(Arc::new(MockStore));
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
    let tool = TaskTool::new(Arc::new(MockStore));
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
    let tool = TaskTool::new(Arc::new(MockStore)).with_write(true);
    let output = tool
        .execute(json!({
            "operation": "stop",
            "id": "task-1"
        }))
        .await
        .unwrap();
    assert!(output.success);
    // Stop should call control_task with action "stop", not delete
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
    let tool = TaskTool::new(Arc::new(MockStore)).with_write(true);
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
async fn test_run_batch_replays_per_task_confirmation() {
    let tool = TaskTool::new(Arc::new(ConfirmationCreateStore::default()))
        .with_write(true)
        .with_assessor(Arc::new(MockAssessor));
    let output = tool
        .execute(json!({
            "operation": "run_batch",
            "agent_id": "agent-1",
            "input": "confirmed input",
            "workers": [
                { "count": 2 }
            ]
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["operation"], "run_batch");
    assert_eq!(output.result["total"], 2);
    assert_eq!(output.result["tasks"][0]["task_id"], "task-2");
    assert_eq!(output.result["tasks"][1]["task_id"], "task-4");
}
