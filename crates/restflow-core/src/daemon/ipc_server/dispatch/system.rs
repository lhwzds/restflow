use super::super::runtime::build_auth_manager;
use super::super::*;
use crate::auth::{provider_available, secret_or_env_exists};
use crate::models::{ModelId, ModelMetadataDTO, Provider, provider_display_order};

fn is_catalog_model(model: ModelId) -> bool {
    !model.is_opencode_cli() && !model.is_gemini_cli()
}

async fn available_providers(core: &Arc<AppCore>) -> Result<Vec<Provider>, String> {
    let auth_manager = build_auth_manager(core)
        .await
        .map_err(|err| err.to_string())?;
    let _ = auth_manager.discover().await;

    let mut providers = Vec::new();
    for provider in Provider::all().iter().copied() {
        let available = provider_available(&auth_manager, provider, |key| {
            secret_or_env_exists(&core.storage.secrets, key)
        })
        .await;

        if available {
            providers.push(provider);
        }
    }

    providers.sort_by_key(|provider| provider_display_order(*provider));
    Ok(providers)
}

async fn available_model_catalog(core: &Arc<AppCore>) -> Result<Vec<ModelMetadataDTO>, String> {
    let providers = available_providers(core).await?;
    let mut models = ModelId::all_with_metadata()
        .into_iter()
        .filter(|metadata| is_catalog_model(metadata.model))
        .filter(|metadata| providers.contains(&metadata.provider))
        .collect::<Vec<_>>();

    models.sort_by(|left, right| {
        provider_display_order(left.provider)
            .cmp(&provider_display_order(right.provider))
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(models)
}

impl IpcServer {
    pub(super) async fn handle_ping() -> IpcResponse {
        IpcResponse::Pong
    }

    pub(super) async fn handle_get_status() -> IpcResponse {
        IpcResponse::success(build_daemon_status())
    }

    pub(super) async fn handle_execute_chat_session_stream_unsupported() -> IpcResponse {
        IpcResponse::error(-3, "Chat session streaming requires direct stream handler")
    }

    pub(super) async fn handle_subscribe_background_agent_events_unsupported() -> IpcResponse {
        IpcResponse::error(-3, "Background agent event streaming requires stream mode")
    }

    pub(super) async fn handle_subscribe_session_events_unsupported() -> IpcResponse {
        IpcResponse::error(-3, "Session event streaming requires stream mode")
    }

    pub(super) async fn handle_get_system_info() -> IpcResponse {
        IpcResponse::success(serde_json::json!({
            "pid": std::process::id(),
        }))
    }

    pub(super) async fn handle_get_available_models(core: &Arc<AppCore>) -> IpcResponse {
        match available_model_catalog(core).await {
            Ok(models) => IpcResponse::success(models),
            Err(err) => IpcResponse::error(500, err),
        }
    }

    pub(super) async fn handle_list_mcp_servers() -> IpcResponse {
        IpcResponse::success(Vec::<String>::new())
    }

    pub(super) async fn handle_shutdown() -> IpcResponse {
        IpcResponse::success(serde_json::json!({ "shutting_down": true }))
    }
}
