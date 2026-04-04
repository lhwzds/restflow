use super::super::runtime::parse_background_agent_status;
use super::super::*;
use crate::boundary::background_agent::{
    core_patch_to_update_request, core_spec_to_create_request,
};
use crate::daemon::request_mapper::to_contract;
use crate::services::background_agent_command::{
    TaskCommandError, TaskCommandService, TaskExecutionMode,
};
use crate::services::operation_assessment::OperationAssessorAdapter;
use crate::storage::background_agent::ResolveTaskIdError;
use restflow_contracts::ApprovalHandledResponse;
use restflow_traits::store::{BackgroundAgentControlRequest, BackgroundAgentDeleteRequest};

fn resolve_background_agent_id(
    core: &Arc<AppCore>,
    id: &str,
) -> std::result::Result<String, IpcResponse> {
    match core
        .storage
        .background_agents
        .resolve_existing_task_id_typed(id)
    {
        Ok(id) => Ok(id),
        Err(ResolveTaskIdError::NotFound(_)) => Err(IpcResponse::not_found("Background agent")),
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

impl IpcServer {
    pub(super) async fn handle_list_background_agents(
        core: &Arc<AppCore>,
        status: Option<String>,
    ) -> IpcResponse {
        let result = match status {
            Some(status) => match parse_background_agent_status(&status) {
                Ok(status) => core.storage.background_agents.list_tasks_by_status(status),
                Err(err) => return IpcResponse::error(400, err.to_string()),
            },
            None => core.storage.background_agents.list_tasks(),
        };

        match result {
            Ok(background_agents) => IpcResponse::success(background_agents),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_runnable_background_agents(
        core: &Arc<AppCore>,
        current_time: Option<i64>,
    ) -> IpcResponse {
        let now = current_time.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        match core.storage.background_agents.list_runnable_tasks(now) {
            Ok(background_agents) => IpcResponse::success(background_agents),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_background_agent(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match core.storage.background_agents.get_task(&resolved_id) {
            Ok(Some(background_agent)) => IpcResponse::success(background_agent),
            Ok(None) => IpcResponse::not_found("Background agent"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_background_agent_history(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match core
            .storage
            .background_agents
            .list_events_for_task(&resolved_id)
        {
            Ok(events) => IpcResponse::success(events),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_background_agent(
        core: &Arc<AppCore>,
        spec: crate::models::BackgroundAgentSpec,
    ) -> IpcResponse {
        let request = match core_spec_to_create_request(&spec) {
            Ok(request) => request,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match command_service(core)
            .create_from_request(request, TaskExecutionMode::Direct)
            .await
            .and_then(TaskCommandService::into_direct_result)
        {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_convert_session_to_background_agent(
        core: &Arc<AppCore>,
        request: restflow_traits::store::BackgroundAgentConvertSessionRequest,
    ) -> IpcResponse {
        match command_service(core)
            .convert_session(request, TaskExecutionMode::Direct)
            .await
            .and_then(TaskCommandService::into_direct_result)
        {
            Ok(result) => IpcResponse::success(result),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_update_background_agent(
        core: &Arc<AppCore>,
        id: String,
        patch: crate::models::BackgroundAgentPatch,
    ) -> IpcResponse {
        let request = match core_patch_to_update_request(id, &patch) {
            Ok(request) => request,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match command_service(core)
            .update_from_request(request, TaskExecutionMode::Direct)
            .await
            .and_then(TaskCommandService::into_direct_result)
        {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_delete_background_agent(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        let request = BackgroundAgentDeleteRequest {
            id,
            preview: false,
            approval_id: None,
        };
        match command_service(core)
            .delete_from_request(request, TaskExecutionMode::Direct)
            .await
            .and_then(TaskCommandService::into_direct_result)
        {
            Ok(result) => IpcResponse::success(result),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_control_background_agent(
        core: &Arc<AppCore>,
        id: String,
        action: crate::models::BackgroundAgentControlAction,
    ) -> IpcResponse {
        let action = match to_contract(action) {
            Ok(value) => value,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let request = BackgroundAgentControlRequest {
            id,
            action,
            preview: false,
            approval_id: None,
        };
        match command_service(core)
            .control_from_request(request, TaskExecutionMode::Direct)
            .await
            .and_then(TaskCommandService::into_direct_result)
        {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_get_background_agent_progress(
        core: &Arc<AppCore>,
        id: String,
        event_limit: Option<usize>,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match command_service(core).progress(&resolved_id, event_limit.unwrap_or(10)) {
            Ok(progress) => IpcResponse::success(progress),
            Err(err) => command_error_response(err),
        }
    }

    pub(super) async fn handle_send_background_agent_message(
        core: &Arc<AppCore>,
        id: String,
        message: String,
        source: Option<crate::models::TaskMessageSource>,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
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

    pub(super) async fn handle_background_agent_approval(
        core: &Arc<AppCore>,
        id: String,
        approved: bool,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        let message = if approved {
            "User approved the pending action."
        } else {
            "User rejected the pending action."
        };
        match core
            .storage
            .background_agents
            .send_background_agent_message(
                &resolved_id,
                message.to_string(),
                crate::models::TaskMessageSource::System,
            ) {
            Ok(_) => IpcResponse::success(ApprovalHandledResponse { handled: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_background_agent_messages(
        core: &Arc<AppCore>,
        id: String,
        limit: Option<usize>,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match core
            .storage
            .background_agents
            .list_background_agent_messages(&resolved_id, limit.unwrap_or(50).max(1))
        {
            Ok(messages) => IpcResponse::success(messages),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
