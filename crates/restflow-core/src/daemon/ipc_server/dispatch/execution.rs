use super::super::*;
use crate::services::session_policy::SessionPolicy;
use restflow_contracts::request::WireModelRef;

impl IpcServer {
    pub(super) async fn handle_list_run_artifacts(
        core: &Arc<AppCore>,
        run_id: Option<String>,
        task_id: Option<String>,
    ) -> IpcResponse {
        let result = if let Some(run_id) = run_id {
            core.storage.run_artifacts.list_by_run(&run_id)
        } else if let Some(task_id) = task_id {
            core.storage.run_artifacts.list_by_task(&task_id)
        } else {
            return IpcResponse::error(400, "ListRunArtifacts requires run_id or task_id");
        };

        match result {
            Ok(artifacts) => IpcResponse::success(artifacts),
            Err(error) => IpcResponse::error(500, error.to_string()),
        }
    }

    pub(super) async fn handle_switch_session_model(
        core: &Arc<AppCore>,
        session_id: String,
        model_ref: WireModelRef,
    ) -> IpcResponse {
        let mut session = match core.storage.chat_sessions.get(&session_id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("session"),
            Err(error) => return IpcResponse::error(500, error.to_string()),
        };
        if let Err(error) = SessionPolicy::from_storage(&core.storage)
            .ensure_workspace_operation_allowed(&session, "switch model")
        {
            return IpcResponse::error(409, error.to_string());
        }
        session.provider = model_ref.provider;
        session.model = model_ref.model;
        session.updated_at = chrono::Utc::now().timestamp_millis();

        match core.storage.chat_sessions.update(&session) {
            Ok(()) => match core.storage.chat_sessions.get(&session_id) {
                Ok(Some(session)) => IpcResponse::success(session),
                Ok(None) => IpcResponse::not_found("session"),
                Err(error) => IpcResponse::error(500, error.to_string()),
            },
            Err(error) => IpcResponse::error(500, error.to_string()),
        }
    }
}
