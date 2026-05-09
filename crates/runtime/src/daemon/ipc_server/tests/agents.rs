use super::*;
use crate::daemon::request_mapper::to_contract;
use types::CleanupReportResponse;
use types::request::{AgentNode as ContractAgentNode, WireModelRef};

#[tokio::test]
async fn process_run_cleanup_returns_report() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(&core, &runtime_tool_registry, IpcRequest::RunCleanup).await;

    match response {
        IpcResponse::Success(value) => {
            let report: CleanupReportResponse =
                serde_json::from_value(value).expect("cleanup report");
            assert_eq!(report.chat_sessions, 0);
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_set_and_get_secret_round_trip() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let set_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::SetSecret {
            key: "TEST_SECRET".to_string(),
            value: "secret-value".to_string(),
            description: Some("test secret".to_string()),
        },
    )
    .await;
    match set_response {
        IpcResponse::Success(_) => {}
        other => panic!("expected success response, got {other:?}"),
    }

    let get_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetSecret {
            key: "TEST_SECRET".to_string(),
        },
    )
    .await;

    match get_response {
        IpcResponse::Success(value) => {
            assert_eq!(value["value"], "secret-value");
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_create_agent_returns_stored_agent() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateAgent {
            name: "IPC Agent".to_string(),
            agent: to_contract(AgentNode {
                model_ref: Some(crate::models::ModelRef::from_model(
                    crate::models::ModelId::ClaudeSonnet4_5,
                )),
                prompt: Some("You are a helpful assistant".to_string()),
                temperature: Some(0.7),
                codex_cli_reasoning_effort: None,
                codex_cli_execution_mode: None,
                api_key_config: Some(crate::models::ApiKeyConfig::Direct("test_key".to_string())),
                tools: None,
                skills: None,
                skill_variables: None,
                skill_preflight_policy_mode: None,
                model_routing: None,
            })
            .expect("contract agent node"),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            assert_eq!(value["name"], "IPC Agent");
            assert!(value["id"].as_str().is_some());
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_create_agent_with_warning_persists_without_confirmation() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateAgent {
            name: "warning-agent".to_string(),
            agent: to_contract(AgentNode::new()).expect("contract agent"),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            assert_eq!(value["name"], "warning-agent");
            assert!(value["id"].as_str().is_some());
            let agents = core.storage.agents.list_agents().unwrap();
            assert_eq!(agents.len(), 2, "warning should not block persistence");
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_create_agent_still_returns_stored_agent_when_provider_is_unconfigured() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateAgent {
            name: "warning-agent".to_string(),
            agent: to_contract(AgentNode::new()).expect("contract agent"),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            assert_eq!(value["name"], "warning-agent");
            assert!(value["id"].as_str().is_some());
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_create_agent_rejects_invalid_wire_model_ref() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateAgent {
            name: "invalid-agent".to_string(),
            agent: ContractAgentNode {
                model_ref: Some(WireModelRef {
                    provider: "unknown-provider".to_string(),
                    model: "gpt-5".to_string(),
                }),
                ..ContractAgentNode::default()
            },
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 400);
            assert_eq!(error.kind, types::ErrorKind::Validation);
            let details = error.details.expect("validation details");
            assert_eq!(details["type"], "validation_error");
            assert_eq!(details["errors"][0]["field"], "model_ref.provider");
        }
        other => panic!("expected validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_config_returns_system_config() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(&core, &runtime_tool_registry, IpcRequest::GetConfig).await;

    match response {
        IpcResponse::Success(value) => {
            let _config: crate::storage::SystemConfig =
                serde_json::from_value(value).expect("system config");
        }
        other => panic!("expected success response, got {other:?}"),
    }
}
