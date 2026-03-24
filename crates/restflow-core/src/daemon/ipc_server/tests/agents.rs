use super::*;
use crate::daemon::request_mapper::to_contract;
use crate::models::{ApiKeyConfig, ModelId};
use restflow_contracts::request::{AgentNode as ContractAgentNode, WireModelRef};
use restflow_contracts::{ApprovalHandledResponse, DeleteWithIdResponse};
use restflow_storage::SimpleStorage;

fn raw_agent_storage(core: &Arc<AppCore>) -> restflow_storage::AgentStorage {
    restflow_storage::AgentStorage::new(core.storage.get_db()).unwrap()
}

fn ensure_test_agent_with_id(core: &Arc<AppCore>, id: &str) {
    if core
        .storage
        .agents
        .get_agent(id.to_string())
        .unwrap()
        .is_some()
    {
        return;
    }

    let stored = crate::storage::agent::StoredAgent {
        id: id.to_string(),
        name: format!("Agent {id}"),
        agent: AgentNode::with_model(ModelId::Gpt5)
            .with_api_key(ApiKeyConfig::Direct("test-key".to_string())),
        prompt_file: None,
        created_at: Some(0),
        updated_at: Some(0),
    };
    let raw = serde_json::to_vec(&stored).unwrap();
    raw_agent_storage(core).put_raw(id, &raw).unwrap();
}

fn background_agent_spec(name: &str) -> crate::models::BackgroundAgentSpec {
    crate::models::BackgroundAgentSpec {
        name: name.to_string(),
        agent_id: "agent-1".to_string(),
        chat_session_id: None,
        description: None,
        input: Some("run".to_string()),
        input_template: None,
        schedule: crate::models::BackgroundAgentSchedule::default(),
        notification: None,
        execution_mode: None,
        timeout_secs: None,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: Vec::new(),
        continuation: None,
    }
}

fn configure_default_agent(core: &Arc<AppCore>) -> String {
    let default_id = core.storage.agents.resolve_default_agent_id().unwrap();
    core.storage
        .agents
        .update_agent(
            default_id.clone(),
            None,
            Some(
                AgentNode::with_model(ModelId::Gpt5)
                    .with_api_key(ApiKeyConfig::Direct("test-key".to_string())),
            ),
        )
        .unwrap();
    default_id
}

fn raw_background_agent_storage(core: &Arc<AppCore>) -> restflow_storage::BackgroundAgentStorage {
    restflow_storage::BackgroundAgentStorage::new(core.storage.get_db()).unwrap()
}

fn insert_background_agent_with_id(
    core: &Arc<AppCore>,
    id: &str,
) -> crate::models::BackgroundAgent {
    ensure_test_agent_with_id(core, "agent-1");
    let mut task = crate::models::BackgroundAgent::new(
        id.to_string(),
        format!("Task {id}"),
        "agent-1".to_string(),
        crate::models::BackgroundAgentSchedule::default(),
    );
    task.input = Some("run".to_string());
    let raw = serde_json::to_vec(&task).unwrap();
    raw_background_agent_storage(core)
        .put_task_raw_with_status(id, task.status.as_str(), &raw)
        .unwrap();
    task
}

#[tokio::test]
async fn process_get_background_agent_returns_created_task() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    ensure_test_agent_with_id(&core, "agent-1");

    let task = core
        .storage
        .background_agents
        .create_background_agent(background_agent_spec("ipc-background"))
        .unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetBackgroundAgent {
            id: task.id.clone(),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let returned: crate::models::BackgroundAgent =
                serde_json::from_value(value).expect("background agent");
            assert_eq!(returned.id, task.id);
            assert_eq!(returned.name, "ipc-background");
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_delete_background_agent_returns_delete_with_id_response() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    ensure_test_agent_with_id(&core, "agent-1");

    let task = core
        .storage
        .background_agents
        .create_background_agent(background_agent_spec("ipc-delete"))
        .unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::DeleteBackgroundAgent {
            id: task.id.clone(),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let deleted: DeleteWithIdResponse =
                serde_json::from_value(value).expect("delete response");
            assert_eq!(deleted.id, task.id);
            assert!(deleted.deleted);
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_handle_background_agent_approval_returns_typed_response() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    ensure_test_agent_with_id(&core, "agent-1");

    let task = core
        .storage
        .background_agents
        .create_background_agent(background_agent_spec("ipc-approval"))
        .unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::HandleBackgroundAgentApproval {
            id: task.id.clone(),
            approved: true,
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let handled: ApprovalHandledResponse =
                serde_json::from_value(value).expect("approval response");
            assert!(handled.handled);
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_background_agent_returns_not_found_for_missing_task() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetBackgroundAgent {
            id: "missing-task".to_string(),
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 404);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::NotFound);
            assert!(error.message.contains("Background agent"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_background_agent_returns_bad_request_for_ambiguous_prefix() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    for id in ["shared-1", "shared-2"] {
        insert_background_agent_with_id(&core, id);
    }

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetBackgroundAgent {
            id: "shared".to_string(),
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 400);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::Validation);
            assert!(error.message.contains("ambiguous"));
            assert!(error.message.contains("shared-1"));
            assert!(error.message.contains("shared-2"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_background_agent_returns_internal_error_when_resolution_scan_fails() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let raw_storage = raw_background_agent_storage(&core);

    raw_storage
        .put_task_raw_with_status("bad-task", "active", b"{bad-json")
        .unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetBackgroundAgent {
            id: "missing-task".to_string(),
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 500);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::Internal);
            assert!(error.message.contains("key must be a string"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_update_background_agent_resolves_unique_prefix() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let task = insert_background_agent_with_id(&core, "prefix-update-1");

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::UpdateBackgroundAgent {
            id: "prefix-update".to_string(),
            patch: to_contract(crate::models::BackgroundAgentPatch {
                description: Some("updated description".to_string()),
                ..Default::default()
            })
            .expect("contract patch"),
            preview: false,
            confirmation_token: None,
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let updated: crate::models::BackgroundAgent =
                serde_json::from_value(value).expect("background agent");
            assert_eq!(updated.id, task.id);
            assert_eq!(updated.description.as_deref(), Some("updated description"));
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_create_background_agent_accepts_default_agent_alias() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let default_agent_id = configure_default_agent(&core);

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateBackgroundAgent {
            spec: to_contract(crate::models::BackgroundAgentSpec {
                agent_id: "default".to_string(),
                ..background_agent_spec("ipc-default-alias")
            })
            .expect("contract spec"),
            preview: false,
            confirmation_token: None,
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let created: crate::models::BackgroundAgent =
                serde_json::from_value(value).expect("background agent");
            assert_eq!(created.agent_id, default_agent_id);
            assert_eq!(created.name, "ipc-default-alias");
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_update_background_agent_accepts_default_agent_alias() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let default_agent_id = configure_default_agent(&core);
    let task = insert_background_agent_with_id(&core, "update-default-agent");

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::UpdateBackgroundAgent {
            id: task.id.clone(),
            patch: to_contract(crate::models::BackgroundAgentPatch {
                agent_id: Some("default".to_string()),
                ..Default::default()
            })
            .expect("contract patch"),
            preview: false,
            confirmation_token: None,
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let updated: crate::models::BackgroundAgent =
                serde_json::from_value(value).expect("background agent");
            assert_eq!(updated.id, task.id);
            assert_eq!(updated.agent_id, default_agent_id);
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_delete_background_agent_rejects_ambiguous_prefix() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    for id in ["dup-delete-1", "dup-delete-2"] {
        insert_background_agent_with_id(&core, id);
    }

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::DeleteBackgroundAgent {
            id: "dup-delete".to_string(),
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 400);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::Validation);
            assert!(error.message.contains("ambiguous"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_background_agent_history_returns_not_found_for_missing_task() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetBackgroundAgentHistory {
            id: "missing-history".to_string(),
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 404);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::NotFound);
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_list_background_agent_messages_returns_internal_error_when_resolution_scan_fails()
{
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    raw_background_agent_storage(&core)
        .put_task_raw_with_status("broken-task", "active", b"{bad-json")
        .unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ListBackgroundAgentMessages {
            id: "missing-messages".to_string(),
            limit: Some(5),
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 500);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::Internal);
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_control_background_agent_resolves_unique_prefix() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let task = insert_background_agent_with_id(&core, "prefix-control-1");

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ControlBackgroundAgent {
            id: "prefix-control".to_string(),
            action: to_contract(crate::models::BackgroundAgentControlAction::Pause)
                .expect("contract action"),
            preview: false,
            confirmation_token: None,
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let updated: crate::models::BackgroundAgent =
                serde_json::from_value(value).expect("background agent");
            assert_eq!(updated.id, task.id);
            assert_eq!(updated.status, crate::models::BackgroundAgentStatus::Paused);
        }
        other => panic!("expected success response, got {other:?}"),
    }
}
#[tokio::test]
async fn process_list_auth_profiles_returns_empty_by_default() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response =
        IpcServer::process(&core, &runtime_tool_registry, IpcRequest::ListAuthProfiles).await;

    match response {
        IpcResponse::Success(value) => {
            let profiles: Vec<crate::auth::AuthProfile> =
                serde_json::from_value(value).expect("auth profiles");
            assert!(profiles.is_empty());
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_create_terminal_session_returns_session() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateTerminalSession,
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let session: crate::models::TerminalSession =
                serde_json::from_value(value).expect("terminal session");
            assert!(session.id.starts_with("terminal-"));
            assert!(!session.name.is_empty());
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_list_hooks_returns_empty_by_default() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(&core, &runtime_tool_registry, IpcRequest::ListHooks).await;

    match response {
        IpcResponse::Success(value) => {
            let hooks: Vec<crate::models::Hook> = serde_json::from_value(value).expect("hooks");
            assert!(hooks.is_empty());
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
async fn process_create_work_item_returns_item() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateWorkItem {
            spec: to_contract(crate::models::WorkItemSpec {
                folder: "inbox".to_string(),
                title: "Follow up".to_string(),
                content: "Review ipc dispatch split".to_string(),
                priority: Some("p1".to_string()),
                tags: vec!["ipc".to_string()],
            })
            .expect("contract work item spec"),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let item: crate::models::WorkItem = serde_json::from_value(value).expect("work item");
            assert_eq!(item.folder, "inbox");
            assert_eq!(item.title, "Follow up");
            assert_eq!(item.content, "Review ipc dispatch split");
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
                model: Some(crate::models::ModelId::ClaudeSonnet4_5),
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
            preview: false,
            confirmation_token: None,
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
async fn process_create_agent_preview_returns_warning_assessment_without_persisting() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateAgent {
            name: "preview-agent".to_string(),
            agent: to_contract(AgentNode::new()).expect("contract agent"),
            preview: true,
            confirmation_token: None,
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            assert_eq!(value["status"], "preview");
            assert_eq!(value["assessment"]["status"], "warning");
            assert_eq!(value["assessment"]["requires_confirmation"], true);
            assert!(value["assessment"]["confirmation_token"].is_string());
            let agents = core.storage.agents.list_agents().unwrap();
            assert_eq!(agents.len(), 1, "preview must not persist a new agent");
        }
        other => panic!("expected preview response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_create_agent_requires_confirmation_for_unconfigured_provider() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateAgent {
            name: "warning-agent".to_string(),
            agent: to_contract(AgentNode::new()).expect("contract agent"),
            preview: false,
            confirmation_token: None,
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 428);
            assert_eq!(
                error.kind,
                restflow_contracts::ErrorKind::ConfirmationRequired
            );
            let details = error.details.expect("confirmation details");
            assert_eq!(details["assessment"]["status"], "warning");
            assert!(details["assessment"]["confirmation_token"].is_string());
        }
        other => panic!("expected confirmation_required response, got {other:?}"),
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
                model: None,
                model_ref: Some(WireModelRef {
                    provider: "unknown-provider".to_string(),
                    model: "gpt-5".to_string(),
                }),
                ..ContractAgentNode::default()
            },
            preview: false,
            confirmation_token: None,
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 400);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::Validation);
            let details = error.details.expect("validation details");
            assert_eq!(details["type"], "validation_error");
            assert_eq!(details["errors"][0]["field"], "model_ref.provider");
        }
        other => panic!("expected validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn process_create_background_agent_requires_confirmation_when_agent_provider_missing() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let stored_agent = core
        .storage
        .agents
        .create_agent("warning background agent".to_string(), AgentNode::new())
        .unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateBackgroundAgent {
            spec: to_contract(crate::models::BackgroundAgentSpec {
                name: "bg-warning".to_string(),
                agent_id: stored_agent.id.clone(),
                chat_session_id: None,
                description: Some("warn before save".to_string()),
                input: Some("run".to_string()),
                input_template: None,
                schedule: crate::models::BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("contract spec"),
            preview: false,
            confirmation_token: None,
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 428);
            assert_eq!(
                error.kind,
                restflow_contracts::ErrorKind::ConfirmationRequired
            );
            let details = error.details.expect("confirmation details");
            assert_eq!(details["assessment"]["status"], "warning");
        }
        other => panic!("expected confirmation_required response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_create_skill_and_get_skill_round_trip() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let skill = Skill::new(
        "skill-ipc-test".to_string(),
        "IPC Skill".to_string(),
        Some("Created through ipc".to_string()),
        Some(vec!["ipc".to_string()]),
        "Use this skill for testing".to_string(),
    );

    let create_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::CreateSkill {
            skill: to_contract(skill.clone()).expect("contract skill"),
        },
    )
    .await;
    match create_response {
        IpcResponse::Success(_) => {}
        other => panic!("expected success response, got {other:?}"),
    }

    let get_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetSkill {
            id: skill.id.clone(),
        },
    )
    .await;

    match get_response {
        IpcResponse::Success(value) => {
            let returned: Skill = serde_json::from_value(value).expect("skill");
            assert_eq!(returned.id, skill.id);
            assert_eq!(returned.name, "IPC Skill");
        }
        other => panic!("expected success response, got {other:?}"),
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
