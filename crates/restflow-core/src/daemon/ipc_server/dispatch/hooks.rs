use super::super::runtime::sample_hook_context;
use super::super::*;
use restflow_contracts::{DeleteResponse, OkResponse};

impl IpcServer {
    pub(super) async fn handle_list_hooks(core: &Arc<AppCore>) -> IpcResponse {
        match core.storage.hooks.list() {
            Ok(hooks) => IpcResponse::success(hooks),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_hook(
        core: &Arc<AppCore>,
        hook: crate::models::Hook,
    ) -> IpcResponse {
        match core.storage.hooks.create(&hook) {
            Ok(()) => IpcResponse::success(hook),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_hook(
        core: &Arc<AppCore>,
        id: String,
        hook: crate::models::Hook,
    ) -> IpcResponse {
        match core.storage.hooks.update(&id, &hook) {
            Ok(()) => IpcResponse::success(hook),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_hook(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match core.storage.hooks.delete(&id) {
            Ok(deleted) => IpcResponse::success(DeleteResponse { deleted }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_test_hook(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let hook = match core.storage.hooks.get(&id) {
            Ok(Some(hook)) => hook,
            Ok(None) => return IpcResponse::not_found("Hook"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let scheduler = Arc::new(crate::hooks::BackgroundAgentHookScheduler::new(
            core.storage.background_agents.clone(),
        ));
        let executor = crate::hooks::HookExecutor::with_storage(core.storage.hooks.clone())
            .with_task_scheduler(scheduler);
        let context = sample_hook_context(&hook.event);
        match executor.execute_hook(&hook, &context).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
