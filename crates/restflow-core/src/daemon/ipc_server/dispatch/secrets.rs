use super::super::*;

impl IpcServer {
    pub(super) async fn handle_list_secrets(core: &Arc<AppCore>) -> IpcResponse {
        match secrets_service::list_secrets(core).await {
            Ok(secrets) => IpcResponse::success(secrets),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_secret(core: &Arc<AppCore>, key: String) -> IpcResponse {
        match secrets_service::get_secret(core, &key).await {
            Ok(Some(value)) => IpcResponse::success(serde_json::json!({ "value": value })),
            Ok(None) => IpcResponse::not_found("Secret"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_set_secret(
        core: &Arc<AppCore>,
        key: String,
        value: String,
        description: Option<String>,
    ) -> IpcResponse {
        match secrets_service::set_secret(core, &key, &value, description).await {
            Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_secret(
        core: &Arc<AppCore>,
        key: String,
        value: String,
        description: Option<String>,
    ) -> IpcResponse {
        match secrets_service::create_secret(core, &key, &value, description).await {
            Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_secret(
        core: &Arc<AppCore>,
        key: String,
        value: String,
        description: Option<String>,
    ) -> IpcResponse {
        match secrets_service::update_secret(core, &key, &value, description).await {
            Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_secret(core: &Arc<AppCore>, key: String) -> IpcResponse {
        match secrets_service::delete_secret(core, &key).await {
            Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
