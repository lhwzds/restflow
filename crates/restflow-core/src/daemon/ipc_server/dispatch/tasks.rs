use super::super::runtime::parse_task_status;
use super::super::*;
use crate::boundary::task::{core_patch_to_update_request, core_spec_to_create_request};
use crate::daemon::request_mapper::to_contract;
use crate::services::operation_assessment::OperationAssessorAdapter;
use crate::services::operation_assessment::assessment_summary;
use crate::services::task_command::{TaskCommandError, TaskCommandService, TaskExecutionMode};
use crate::storage::task_runtime::ResolveTaskIdError;
use restflow_traits::ApprovalHandledResponse;
use restflow_traits::TaskCommandOutcome;
use restflow_traits::store::{TaskControlRequest, TaskConvertSessionRequest, TaskDeleteRequest};
use serde::Serialize;
use serde_json::json;

fn resolve_task_id(core: &Arc<AppCore>, id: &str) -> std::result::Result<String, IpcResponse> {
    match core.storage.tasks.resolve_existing_task_id_typed(id) {
        Ok(id) => Ok(id),
        Err(ResolveTaskIdError::NotFound(_)) => Err(IpcResponse::not_found("Task")),
        Err(ResolveTaskIdError::Ambiguous { prefix, preview }) => Err(IpcResponse::error(
            400,
            format!("Task ID prefix '{prefix}' is ambiguous. Candidates: {preview}"),
        )),
        Err(ResolveTaskIdError::Internal(err)) => Err(IpcResponse::error(500, err.to_string())),
    }
}

fn command_service(core: &Arc<AppCore>) -> TaskCommandService {
    TaskCommandService::from_storage(
        core.storage.as_ref(),
        Some(Arc::new(OperationAssessorAdapter::new(core.clone()))),
    )
}

fn command_error_response(error: TaskCommandError) -> IpcResponse {
    IpcResponse::error_payload(error.payload())
}

fn guarded_mutation_response<T: Serialize>(outcome: TaskCommandOutcome<T>) -> IpcResponse {
    match outcome {
        TaskCommandOutcome::Executed { result } => IpcResponse::success(result),
        TaskCommandOutcome::Blocked { assessment } => IpcResponse::error_with_details(
            409,
            assessment_summary(&assessment),
            Some(json!({
                "status": "blocked",
                "assessment": assessment,
            })),
        ),
        TaskCommandOutcome::ConfirmationRequired { assessment } => IpcResponse::error_with_details(
            409,
            assessment_summary(&assessment),
            Some(json!({
                "status": "confirmation_required",
                "pending_approval": true,
                "approval_id": assessment.approval_id,
                "assessment": assessment,
            })),
        ),
        TaskCommandOutcome::Preview { assessment } => IpcResponse::error_with_details(
            400,
            "Typed IPC task mutations do not support preview. Use manage_tasks for preview flows.",
            Some(json!({
                "status": "preview",
                "assessment": assessment,
            })),
        ),
    }
}

impl IpcServer {
    pub(super) async fn handle_list_tasks(
        core: &Arc<AppCore>,
        status: Option<String>,
    ) -> IpcResponse {
        let result = match status {
            Some(status) => match parse_task_status(&status) {
                Ok(status) => core.storage.tasks.list_tasks_by_status(status),
                Err(err) => return IpcResponse::error(400, err.to_string()),
            },
            None => core.storage.tasks.list_tasks(),
        };

        match result {
            Ok(tasks) => IpcResponse::success(tasks),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_runnable_tasks(
        core: &Arc<AppCore>,
        current_time: Option<i64>,
    ) -> IpcResponse {
        let now = current_time.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        match core.storage.tasks.list_runnable_tasks(now) {
            Ok(tasks) => IpcResponse::success(tasks),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_task(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let resolved_id = match resolve_task_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match core.storage.tasks.get_task(&resolved_id) {
            Ok(Some(task)) => IpcResponse::success(task),
            Ok(None) => IpcResponse::not_found("Task"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_task_history(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let resolved_id = match resolve_task_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match core.storage.tasks.list_events_for_task(&resolved_id) {
            Ok(events) => IpcResponse::success(events),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_task(
        core: &Arc<AppCore>,
        spec: crate::models::TaskSpec,
    ) -> IpcResponse {
        let request = match core_spec_to_create_request(&spec) {
            Ok(request) => request,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match command_service(core)
            .create_from_request(request, TaskExecutionMode::Guarded)
            .await
        {
            Ok(outcome) => guarded_mutation_response(outcome),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_create_task_from_session(
        core: &Arc<AppCore>,
        request: TaskConvertSessionRequest,
    ) -> IpcResponse {
        match command_service(core)
            .convert_session(request, TaskExecutionMode::Guarded)
            .await
        {
            Ok(outcome) => guarded_mutation_response(outcome),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_update_task(
        core: &Arc<AppCore>,
        id: String,
        patch: crate::models::TaskPatch,
    ) -> IpcResponse {
        let request = match core_patch_to_update_request(id, &patch) {
            Ok(request) => request,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match command_service(core)
            .update_from_request(request, TaskExecutionMode::Guarded)
            .await
        {
            Ok(outcome) => guarded_mutation_response(outcome),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_delete_task(
        core: &Arc<AppCore>,
        id: String,
        approval_id: Option<String>,
    ) -> IpcResponse {
        let request = TaskDeleteRequest {
            id,
            preview: false,
            approval_id,
        };
        match command_service(core)
            .delete_from_request(request, TaskExecutionMode::Guarded)
            .await
        {
            Ok(outcome) => guarded_mutation_response(outcome),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_control_task(
        core: &Arc<AppCore>,
        id: String,
        action: crate::models::TaskControlAction,
        approval_id: Option<String>,
    ) -> IpcResponse {
        let action = match to_contract(action) {
            Ok(value) => value,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let request = TaskControlRequest {
            id,
            action,
            preview: false,
            approval_id,
        };
        match command_service(core)
            .control_from_request(request, TaskExecutionMode::Guarded)
            .await
        {
            Ok(outcome) => guarded_mutation_response(outcome),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_get_task_progress(
        core: &Arc<AppCore>,
        id: String,
        event_limit: Option<usize>,
    ) -> IpcResponse {
        let resolved_id = match resolve_task_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match command_service(core).progress(&resolved_id, event_limit.unwrap_or(10)) {
            Ok(progress) => IpcResponse::success(progress),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_send_task_message(
        core: &Arc<AppCore>,
        id: String,
        message: String,
        source: Option<crate::models::TaskMessageSource>,
    ) -> IpcResponse {
        let resolved_id = match resolve_task_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match command_service(core).send_message(
            &resolved_id,
            message,
            source.unwrap_or(crate::models::TaskMessageSource::User),
        ) {
            Ok(msg) => IpcResponse::success(msg),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_task_approval(
        core: &Arc<AppCore>,
        id: String,
        approved: bool,
    ) -> IpcResponse {
        let resolved_id = match resolve_task_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        let message = if approved {
            "User approved the pending action."
        } else {
            "User rejected the pending action."
        };
        match core.storage.tasks.send_task_message(
            &resolved_id,
            message.to_string(),
            crate::models::TaskMessageSource::System,
        ) {
            Ok(_) => IpcResponse::success(ApprovalHandledResponse { handled: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_task_messages(
        core: &Arc<AppCore>,
        id: String,
        limit: Option<usize>,
    ) -> IpcResponse {
        let resolved_id = match resolve_task_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match core
            .storage
            .tasks
            .list_task_messages(&resolved_id, limit.unwrap_or(50).max(1))
        {
            Ok(messages) => IpcResponse::success(messages),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
