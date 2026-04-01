use super::super::*;
use crate::services::operation_assessment::{
    assess_agent_create, assess_agent_update, assessment_summary,
};
use restflow_contracts::request::AgentNode as ContractAgentNode;
use restflow_contracts::{ErrorKind, OkResponse};
use restflow_traits::OperationAssessment;
use restflow_traits::store::{AgentCreateRequest, AgentUpdateRequest};
use serde_json::json;

fn assessment_details(assessment: &OperationAssessment) -> serde_json::Value {
    json!({ "assessment": assessment })
}

fn blocked_assessment_response(assessment: OperationAssessment) -> IpcResponse {
    IpcResponse::error_payload(restflow_contracts::ErrorPayload::with_kind(
        400,
        ErrorKind::Validation,
        assessment_summary(&assessment),
        Some(assessment_details(&assessment)),
    ))
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
        if !assessment.blockers.is_empty() {
            return blocked_assessment_response(assessment);
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
        if !assessment.blockers.is_empty() {
            return blocked_assessment_response(assessment);
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
