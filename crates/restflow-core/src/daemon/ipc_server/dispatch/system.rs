use super::super::runtime::build_auth_manager;
use super::super::*;
use crate::auth::AuthProvider;
use crate::models::{ModelId, ModelMetadataDTO, Provider};

fn provider_sort_key(provider: Provider) -> usize {
    match provider {
        Provider::OpenAI => 0,
        Provider::MiniMaxCodingPlan => 1,
        Provider::ZaiCodingPlan => 2,
        Provider::ClaudeCode => 3,
        Provider::Codex => 4,
        Provider::Anthropic => 10,
        Provider::Google => 11,
        Provider::DeepSeek => 12,
        Provider::Groq => 13,
        Provider::OpenRouter => 14,
        Provider::XAI => 15,
        Provider::Qwen => 16,
        Provider::Zai => 17,
        Provider::Moonshot => 18,
        Provider::Doubao => 19,
        Provider::Yi => 20,
        Provider::SiliconFlow => 21,
        Provider::MiniMax => 22,
    }
}

fn has_non_empty_secret(core: &Arc<AppCore>, key: &str) -> bool {
    if core
        .storage
        .secrets
        .get_non_empty(key)
        .ok()
        .flatten()
        .is_some()
    {
        return true;
    }

    std::env::var(key)
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

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
        let available = match provider {
            Provider::ClaudeCode => auth_manager
                .get_available_profile(AuthProvider::ClaudeCode)
                .await
                .is_some(),
            Provider::Codex => auth_manager
                .get_available_profile(AuthProvider::OpenAICodex)
                .await
                .is_some(),
            Provider::OpenAI => {
                has_non_empty_secret(core, "OPENAI_API_KEY")
                    || auth_manager
                        .get_available_profile(AuthProvider::OpenAI)
                        .await
                        .is_some()
            }
            Provider::Anthropic => {
                has_non_empty_secret(core, "ANTHROPIC_API_KEY")
                    || auth_manager
                        .get_available_profile(AuthProvider::Anthropic)
                        .await
                        .is_some()
            }
            Provider::Google => {
                has_non_empty_secret(core, "GEMINI_API_KEY")
                    || has_non_empty_secret(core, "GOOGLE_API_KEY")
                    || auth_manager
                        .get_available_profile(AuthProvider::Google)
                        .await
                        .is_some()
            }
            other => other
                .api_key_env()
                .map(|env_name| has_non_empty_secret(core, env_name))
                .unwrap_or(false),
        };

        if available {
            providers.push(provider);
        }
    }

    providers.sort_by_key(|provider| provider_sort_key(*provider));
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
        provider_sort_key(left.provider)
            .cmp(&provider_sort_key(right.provider))
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
