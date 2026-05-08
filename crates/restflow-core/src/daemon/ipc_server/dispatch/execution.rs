use super::super::*;
use restflow_traits::request::WireModelRef;

impl IpcServer {
    pub(super) async fn handle_list_run_artifacts(
        _core: &Arc<AppCore>,
        run_id: Option<String>,
        task_id: Option<String>,
    ) -> IpcResponse {
        if run_id.is_none() && task_id.is_none() {
            return IpcResponse::error(400, "ListRunArtifacts requires run_id or task_id");
        }
        IpcResponse::success(Vec::<crate::models::RunArtifact>::new())
    }

    pub(super) async fn handle_switch_session_model(
        core: &Arc<AppCore>,
        session_id: String,
        model_ref: WireModelRef,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.switch_session_model(&session_id, model_ref.provider, model_ref.model)
        {
            Ok(Some(session)) => IpcResponse::success(session),
            Ok(None) => IpcResponse::not_found("session"),
            Err(error) => ipc_session_lifecycle_error(error),
        }
    }
}
