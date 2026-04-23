use super::super::*;
use crate::auth::secret_or_env_exists;
use crate::models::{
    ModelId, ModelMetadataDTO, Provider, provider_allows_secret_env, provider_display_order,
};

fn is_catalog_model(model: ModelId) -> bool {
    !model.is_opencode_cli() && !model.is_gemini_cli() && !is_legacy_openai_model(model)
}

fn is_legacy_openai_model(model: ModelId) -> bool {
    matches!(
        model,
        ModelId::Gpt5
            | ModelId::Gpt5Mini
            | ModelId::Gpt5Nano
            | ModelId::Gpt5Pro
            | ModelId::Gpt5_1
            | ModelId::Gpt5_2
    )
}

fn available_providers(core: &Arc<AppCore>) -> Vec<Provider> {
    let mut providers = Vec::new();
    for provider in Provider::all().iter().copied() {
        let available = provider == Provider::Codex
            || provider_allows_secret_env(provider)
                && provider
                    .api_key_env_candidates()
                    .any(|key| secret_or_env_exists(&core.storage.secrets, key));

        if available {
            providers.push(provider);
        }
    }

    providers.sort_by_key(|provider| provider_display_order(*provider));
    providers
}

fn available_model_catalog(core: &Arc<AppCore>) -> Vec<ModelMetadataDTO> {
    let providers = available_providers(core);
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

    models
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

    pub(super) async fn handle_subscribe_task_events_unsupported() -> IpcResponse {
        IpcResponse::error(-3, "Task event streaming requires stream mode")
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
        IpcResponse::success(available_model_catalog(core))
    }

    pub(super) async fn handle_list_mcp_servers() -> IpcResponse {
        IpcResponse::success(Vec::<String>::new())
    }

    pub(super) async fn handle_shutdown() -> IpcResponse {
        IpcResponse::success(serde_json::json!({ "shutting_down": true }))
    }
}
