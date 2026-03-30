use super::super::*;
use restflow_contracts::{DeleteResponse, OkResponse};

impl IpcServer {
    pub(super) async fn handle_list_hooks(core: &Arc<AppCore>) -> IpcResponse {
        let service = crate::services::hook_capability::HookCapabilityService::from_storage(
            core.storage.as_ref(),
        );
        match service.list() {
            Ok(hooks) => IpcResponse::success(hooks),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_hook(
        core: &Arc<AppCore>,
        hook: crate::models::Hook,
    ) -> IpcResponse {
        let service = crate::services::hook_capability::HookCapabilityService::from_storage(
            core.storage.as_ref(),
        );
        match service.create(hook) {
            Ok(hook) => IpcResponse::success(hook),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_hook(
        core: &Arc<AppCore>,
        id: String,
        hook: crate::models::Hook,
    ) -> IpcResponse {
        let service = crate::services::hook_capability::HookCapabilityService::from_storage(
            core.storage.as_ref(),
        );
        match service.update(&id, hook) {
            Ok(hook) => IpcResponse::success(hook),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_hook(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let service = crate::services::hook_capability::HookCapabilityService::from_storage(
            core.storage.as_ref(),
        );
        match service.delete(&id) {
            Ok(deleted) => IpcResponse::success(DeleteResponse { deleted }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_test_hook(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let service = crate::services::hook_capability::HookCapabilityService::from_storage(
            core.storage.as_ref(),
        );
        match service.test(&id).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) if err.to_string().contains("Hook not found:") => {
                IpcResponse::not_found("Hook")
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
