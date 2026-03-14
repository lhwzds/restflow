use super::super::*;

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
        match agent_service::update_agent(core, &id, name, agent).await {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_agent(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match agent_service::delete_agent(core, &id).await {
            Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
