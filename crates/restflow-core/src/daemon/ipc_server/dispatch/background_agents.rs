use super::super::runtime::parse_background_agent_status;
use super::super::*;

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
        let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id) {
            Ok(id) => id,
            Err(_) => return IpcResponse::not_found("Background agent"),
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
        match core.storage.background_agents.list_events_for_task(&id) {
            Ok(events) => IpcResponse::success(events),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_background_agent(
        core: &Arc<AppCore>,
        spec: crate::models::BackgroundAgentSpec,
    ) -> IpcResponse {
        match core.storage.background_agents.create_background_agent(spec) {
            Ok(task) => IpcResponse::success(task),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_background_agent(
        core: &Arc<AppCore>,
        id: String,
        patch: crate::models::BackgroundAgentPatch,
    ) -> IpcResponse {
        match core
            .storage
            .background_agents
            .update_background_agent(&id, patch)
        {
            Ok(task) => IpcResponse::success(task),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_background_agent(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        match core.storage.background_agents.delete_task(&id) {
            Ok(deleted) => {
                IpcResponse::success(serde_json::json!({ "deleted": deleted, "id": id }))
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_control_background_agent(
        core: &Arc<AppCore>,
        id: String,
        action: crate::models::BackgroundAgentControlAction,
    ) -> IpcResponse {
        let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id) {
            Ok(id) => id,
            Err(_) => return IpcResponse::not_found("Background agent"),
        };
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
        let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id) {
            Ok(id) => id,
            Err(_) => return IpcResponse::not_found("Background agent"),
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
        let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id) {
            Ok(id) => id,
            Err(_) => return IpcResponse::not_found("Background agent"),
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
        let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id) {
            Ok(id) => id,
            Err(_) => return IpcResponse::not_found("Background agent"),
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
            Ok(_) => {
                // Simplified placeholder:
                // approval is currently injected as a system message so running
                // background agents can continue without a dedicated approval queue.
                // Keep handled=false to make this fallback explicit to callers.
                IpcResponse::success(serde_json::json!({ "handled": false }))
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_background_agent_messages(
        core: &Arc<AppCore>,
        id: String,
        limit: Option<usize>,
    ) -> IpcResponse {
        match core
            .storage
            .background_agents
            .list_background_agent_messages(&id, limit.unwrap_or(50).max(1))
        {
            Ok(messages) => IpcResponse::success(messages),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
