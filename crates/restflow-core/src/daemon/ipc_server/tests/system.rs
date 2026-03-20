use super::super::runtime::build_auth_manager;
use super::*;
use crate::auth::{AuthProvider, Credential, CredentialSource};
use crate::daemon::request_mapper::to_contract;
#[tokio::test]
async fn process_get_system_info_returns_pid() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response =
        IpcServer::process(&core, &runtime_tool_registry, IpcRequest::GetSystemInfo).await;

    match response {
        IpcResponse::Success(value) => {
            let pid = value
                .get("pid")
                .and_then(|value| value.as_u64())
                .expect("pid");
            assert!(pid > 0);
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_build_agent_system_prompt_returns_prompt_payload() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::BuildAgentSystemPrompt {
            agent_node: to_contract(AgentNode::new().with_prompt("Base prompt"))
                .expect("contract agent node"),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let prompt = value
                .get("prompt")
                .and_then(|value| value.as_str())
                .expect("prompt");
            assert!(prompt.contains("Base prompt"));
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_available_models_returns_openai_catalog_when_secret_exists() {
    let (core, _temp) = create_test_core().await;
    core.storage
        .secrets
        .set_secret("OPENAI_API_KEY", "test-openai-key", None)
        .expect("store openai key");
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetAvailableModels,
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let models: Vec<crate::models::ModelMetadataDTO> =
                serde_json::from_value(value).expect("model catalog");
            assert!(
                models
                    .iter()
                    .any(|model| model.provider == crate::models::Provider::OpenAI)
            );
            assert!(
                models
                    .iter()
                    .any(|model| model.model == crate::models::ModelId::Gpt5)
            );
            assert!(
                !models
                    .iter()
                    .any(|model| model.provider == crate::models::Provider::OpenAI
                        && model.model == crate::models::ModelId::CodexCli)
            );
            assert!(
                !models
                    .iter()
                    .any(|model| model.model == crate::models::ModelId::OpenCodeCli)
            );
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_available_models_returns_minimax_m27_catalog_when_secret_exists() {
    let (core, _temp) = create_test_core().await;
    core.storage
        .secrets
        .set_secret("MINIMAX_API_KEY", "test-minimax-key", None)
        .expect("store minimax key");
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetAvailableModels,
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let models: Vec<crate::models::ModelMetadataDTO> =
                serde_json::from_value(value).expect("model catalog");
            assert!(
                models
                    .iter()
                    .any(|model| model.provider == crate::models::Provider::MiniMax)
            );
            assert!(
                models
                    .iter()
                    .any(|model| model.model == crate::models::ModelId::MiniMaxM27)
            );
            assert!(
                models
                    .iter()
                    .any(|model| { model.model == crate::models::ModelId::MiniMaxM27Highspeed })
            );
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_available_models_returns_cli_provider_catalogs_from_auth_profiles() {
    let (core, _temp) = create_test_core().await;
    let manager = build_auth_manager(&core).await.expect("auth manager");
    manager
        .add_profile_from_credential(
            "Claude Code",
            Credential::OAuth {
                access_token: "claude-token".to_string(),
                refresh_token: None,
                expires_at: None,
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::ClaudeCode,
        )
        .await
        .expect("add claude-code profile");
    manager
        .add_profile_from_credential(
            "Codex",
            Credential::OAuth {
                access_token: "codex-token".to_string(),
                refresh_token: None,
                expires_at: None,
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::OpenAICodex,
        )
        .await
        .expect("add codex profile");
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetAvailableModels,
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let models: Vec<crate::models::ModelMetadataDTO> =
                serde_json::from_value(value).expect("model catalog");
            assert!(
                models
                    .iter()
                    .any(|model| model.provider == crate::models::Provider::ClaudeCode)
            );
            assert!(
                models
                    .iter()
                    .any(|model| model.provider == crate::models::Provider::Codex)
            );
            assert!(
                models
                    .iter()
                    .any(|model| model.model == crate::models::ModelId::ClaudeCodeSonnet)
            );
            assert!(
                models
                    .iter()
                    .any(|model| model.model == crate::models::ModelId::Gpt5_4Codex)
            );
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_available_models_returns_all_configured_catalog_groups() {
    let (core, _temp) = create_test_core().await;
    core.storage
        .secrets
        .set_secret("OPENAI_API_KEY", "test-openai-key", None)
        .expect("store openai key");
    core.storage
        .secrets
        .set_secret("MINIMAX_CODING_PLAN_API_KEY", "test-minimax-key", None)
        .expect("store minimax key");
    core.storage
        .secrets
        .set_secret("ZAI_CODING_PLAN_API_KEY", "test-zai-key", None)
        .expect("store zai key");

    let manager = build_auth_manager(&core).await.expect("auth manager");
    manager
        .add_profile_from_credential(
            "Claude Code",
            Credential::OAuth {
                access_token: "claude-token".to_string(),
                refresh_token: None,
                expires_at: None,
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::ClaudeCode,
        )
        .await
        .expect("add claude-code profile");
    manager
        .add_profile_from_credential(
            "Codex",
            Credential::OAuth {
                access_token: "codex-token".to_string(),
                refresh_token: None,
                expires_at: None,
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::OpenAICodex,
        )
        .await
        .expect("add codex profile");

    let runtime_tool_registry = OnceLock::new();
    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetAvailableModels,
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let models: Vec<crate::models::ModelMetadataDTO> =
                serde_json::from_value(value).expect("model catalog");
            let providers: std::collections::HashSet<_> =
                models.iter().map(|model| model.provider).collect();
            assert_eq!(
                providers,
                std::collections::HashSet::from([
                    crate::models::Provider::OpenAI,
                    crate::models::Provider::MiniMaxCodingPlan,
                    crate::models::Provider::ZaiCodingPlan,
                    crate::models::Provider::ClaudeCode,
                    crate::models::Provider::Codex,
                ])
            );
        }
        other => panic!("expected success response, got {other:?}"),
    }
}
