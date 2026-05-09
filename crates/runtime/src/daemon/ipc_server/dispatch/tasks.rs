use super::super::*;
use types::ApprovalHandledResponse;

fn task_storage_removed() -> IpcResponse {
    IpcResponse::error(410, "legacy task storage has been removed")
}

impl IpcServer {
    pub(super) async fn handle_list_tasks(
        core: &Arc<AppCore>,
        status: Option<String>,
    ) -> IpcResponse {
        let _ = (core, status);
        IpcResponse::success(Vec::<crate::models::Task>::new())
    }

    pub(super) async fn handle_list_runnable_tasks(
        core: &Arc<AppCore>,
        current_time: Option<i64>,
    ) -> IpcResponse {
        let _ = (core, current_time);
        IpcResponse::success(Vec::<crate::models::Task>::new())
    }

    pub(super) async fn handle_get_task(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let _ = (core, id);
        IpcResponse::not_found("Task")
    }

    pub(super) async fn handle_get_task_history(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let _ = (core, id);
        IpcResponse::success(Vec::<crate::models::TaskEvent>::new())
    }

    pub(super) async fn handle_create_task(
        core: &Arc<AppCore>,
        spec: crate::models::TaskSpec,
    ) -> IpcResponse {
        let _ = (core, spec);
        task_storage_removed()
    }

    pub(super) async fn handle_create_task_from_session(
        core: &Arc<AppCore>,
        request: types::store::TaskConvertSessionRequest,
    ) -> IpcResponse {
        let _ = (core, request);
        task_storage_removed()
    }

    pub(super) async fn handle_update_task(
        core: &Arc<AppCore>,
        id: String,
        patch: crate::models::TaskPatch,
    ) -> IpcResponse {
        let _ = (core, id, patch);
        task_storage_removed()
    }

    pub(super) async fn handle_delete_task(
        core: &Arc<AppCore>,
        id: String,
        approval_id: Option<String>,
    ) -> IpcResponse {
        let _ = (core, id, approval_id);
        task_storage_removed()
    }

    pub(super) async fn handle_control_task(
        core: &Arc<AppCore>,
        id: String,
        action: crate::models::TaskControlAction,
        approval_id: Option<String>,
    ) -> IpcResponse {
        let _ = (core, id, action, approval_id);
        task_storage_removed()
    }

    pub(super) async fn handle_get_task_progress(
        core: &Arc<AppCore>,
        id: String,
        event_limit: Option<usize>,
    ) -> IpcResponse {
        let _ = (core, id, event_limit);
        task_storage_removed()
    }

    pub(super) async fn handle_send_task_message(
        core: &Arc<AppCore>,
        id: String,
        message: String,
        source: Option<crate::models::TaskMessageSource>,
    ) -> IpcResponse {
        let _ = (core, id, message, source);
        task_storage_removed()
    }

    pub(super) async fn handle_task_approval(
        core: &Arc<AppCore>,
        id: String,
        approved: bool,
    ) -> IpcResponse {
        let _ = (core, id, approved);
        IpcResponse::success(ApprovalHandledResponse { handled: false })
    }

    pub(super) async fn handle_list_task_messages(
        core: &Arc<AppCore>,
        id: String,
        limit: Option<usize>,
    ) -> IpcResponse {
        let _ = (core, id, limit);
        IpcResponse::success(Vec::<crate::models::TaskMessage>::new())
    }
}
