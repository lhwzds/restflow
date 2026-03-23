use super::super::*;
use crate::services::operation_assessment::{
    assess_agent_create, assess_agent_update, assessment_requires_confirmation, assessment_summary,
    ensure_assessment_confirmed,
};
use restflow_contracts::OkResponse;
use restflow_contracts::request::AgentNode as ContractAgentNode;
use restflow_traits::OperationAssessment;
use restflow_traits::store::{AgentCreateRequest, AgentUpdateRequest};
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

impl IpcServer {
    pub(super) async fn handle_list_agents(core: &Arc<AppCore>) -> IpcResponse {
        match agent_service::list_agents(core).await {
            Ok(agents) => IpcResponse::success(agents),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_agent(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match agent_service::get_agent(core, &id).await {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_agent(
        core: &Arc<AppCore>,
        name: String,
        agent: crate::models::AgentNode,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> IpcResponse {
        let assessment = match assess_agent_create(
            core,
            AgentCreateRequest {
                name: name.clone(),
                agent: ContractAgentNode::from(agent.clone()),
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

        match agent_service::create_agent(core, name, agent).await {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_agent(
        core: &Arc<AppCore>,
        id: String,
        name: Option<String>,
        agent: Option<crate::models::AgentNode>,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> IpcResponse {
        let assessment = match assess_agent_update(
            core,
            AgentUpdateRequest {
                id: id.clone(),
                name: name.clone(),
                agent: agent.clone().map(ContractAgentNode::from),
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

        match agent_service::update_agent(core, &id, name, agent).await {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_agent(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match agent_service::delete_agent(core, &id).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
