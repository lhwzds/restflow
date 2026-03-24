use super::super::runtime::parse_background_agent_status;
use super::super::*;
use crate::boundary::background_agent::{
    core_patch_to_update_request, core_spec_to_create_request,
};
use crate::daemon::request_mapper::to_contract;
use crate::services::operation_assessment::{
    assess_background_agent_control, assess_background_agent_create,
    assess_background_agent_update, assessment_requires_confirmation, assessment_summary,
    ensure_assessment_confirmed,
};
use crate::storage::background_agent::ResolveTaskIdError;
use restflow_contracts::{ApprovalHandledResponse, DeleteWithIdResponse};
use restflow_traits::OperationAssessment;
use restflow_traits::store::BackgroundAgentControlRequest;
use serde_json::json;

fn assessment_details(assessment: &OperationAssessment) -> serde_json::Value {
    json!({ "assessment": assessment })
}

fn maybe_preview_or_confirm(
    assessment: OperationAssessment,
    preview: bool,
    confirmation_token: Option<String>,
) -> std::result::Result<Option<IpcResponse>, anyhow::Error> {
    if preview {
        return Ok(Some(IpcResponse::success(json!({
            "status": "preview",
            "assessment": assessment,
        }))));
    }

    if !assessment.blockers.is_empty() {
        return Ok(Some(IpcResponse::error_with_details(
            400,
            assessment_summary(&assessment),
            Some(assessment_details(&assessment)),
        )));
    }

    if assessment_requires_confirmation(&assessment)
        && ensure_assessment_confirmed(&assessment, confirmation_token.as_deref()).is_err()
    {
        return Ok(Some(IpcResponse::error_with_details(
            428,
            assessment_summary(&assessment),
            Some(assessment_details(&assessment)),
        )));
    }

    Ok(None)
}

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
        preview: bool,
        confirmation_token: Option<String>,
    ) -> IpcResponse {
        let assessment = match assess_background_agent_create(
            core,
            match core_spec_to_create_request(&spec) {
                Ok(request) => request,
                Err(err) => return IpcResponse::error(500, err.to_string()),
            },
        )
        .await
        {
            Ok(assessment) => assessment,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match maybe_preview_or_confirm(assessment, preview, confirmation_token) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(err) => return IpcResponse::error(500, err.to_string()),
        }

        match core.storage.background_agents.create_background_agent(spec) {
            Ok(task) => IpcResponse::success(task),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_background_agent(
        core: &Arc<AppCore>,
        id: String,
        patch: crate::models::BackgroundAgentPatch,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        let assessment = match assess_background_agent_update(
            core,
            match core_patch_to_update_request(resolved_id.clone(), &patch) {
                Ok(request) => request,
                Err(err) => return IpcResponse::error(500, err.to_string()),
            },
        )
        .await
        {
            Ok(assessment) => assessment,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match maybe_preview_or_confirm(assessment, preview, confirmation_token) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(err) => return IpcResponse::error(500, err.to_string()),
        }
        match core
            .storage
            .background_agents
            .update_background_agent(&resolved_id, patch)
        {
            Ok(task) => IpcResponse::success(task),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_background_agent(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match core.storage.background_agents.delete_task(&resolved_id) {
            Ok(deleted) => IpcResponse::success(DeleteWithIdResponse {
                id: resolved_id,
                deleted,
            }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_control_background_agent(
        core: &Arc<AppCore>,
        id: String,
        action: crate::models::BackgroundAgentControlAction,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        let assessment = match assess_background_agent_control(
            core,
            BackgroundAgentControlRequest {
                id: resolved_id.clone(),
                action: match to_contract(action.clone()) {
                    Ok(value) => value,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                },
            },
        )
        .await
        {
            Ok(assessment) => assessment,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match maybe_preview_or_confirm(assessment, preview, confirmation_token) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(err) => return IpcResponse::error(500, err.to_string()),
        }
        match core
            .storage
            .background_agents
            .control_background_agent(&resolved_id, action)
        {
            Ok(task) => IpcResponse::success(task),
            Err(err) => IpcResponse::error(500, err.to_string()),
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
        match core
            .storage
            .background_agents
            .get_background_agent_progress(&resolved_id, event_limit.unwrap_or(10))
        {
            Ok(progress) => IpcResponse::success(progress),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_send_background_agent_message(
        core: &Arc<AppCore>,
        id: String,
        message: String,
        source: Option<crate::models::BackgroundMessageSource>,
    ) -> IpcResponse {
        let resolved_id = match resolve_background_agent_id(core, &id) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match core
            .storage
            .background_agents
            .send_background_agent_message(
                &resolved_id,
                message,
                source.unwrap_or(crate::models::BackgroundMessageSource::User),
            ) {
            Ok(msg) => IpcResponse::success(msg),
            Err(err) => IpcResponse::error(500, err.to_string()),
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
                crate::models::BackgroundMessageSource::System,
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
