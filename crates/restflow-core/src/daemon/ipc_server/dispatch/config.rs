use super::super::*;
use restflow_contracts::OkResponse;

impl IpcServer {
    pub(super) async fn handle_get_config(core: &Arc<AppCore>) -> IpcResponse {
        match config_service::get_config(core).await {
            Ok(config) => IpcResponse::success(config),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_global_config(core: &Arc<AppCore>) -> IpcResponse {
        match config_service::get_global_config(core).await {
            Ok(config) => IpcResponse::success(config),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_set_config(
        core: &Arc<AppCore>,
        config: crate::storage::SystemConfig,
    ) -> IpcResponse {
        match config_service::update_config(core, config).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
