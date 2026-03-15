use super::*;
use restflow_contracts::{ApprovalHandledResponse, DeleteWithIdResponse};

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

#[tokio::test]
async fn process_get_background_agent_returns_created_task() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

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
    let raw_storage = restflow_storage::BackgroundAgentStorage::new(core.storage.get_db()).unwrap();

    for id in ["shared-1", "shared-2"] {
        let task = crate::models::BackgroundAgent::new(
            id.to_string(),
            format!("Task {id}"),
            "agent-1".to_string(),
            crate::models::BackgroundAgentSchedule::default(),
        );
        let raw = serde_json::to_vec(&task).unwrap();
        raw_storage
            .put_task_raw_with_status(id, task.status.as_str(), &raw)
            .unwrap();
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
    let raw_storage = restflow_storage::BackgroundAgentStorage::new(core.storage.get_db()).unwrap();

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
            spec: crate::models::WorkItemSpec {
                folder: "inbox".to_string(),
                title: "Follow up".to_string(),
                content: "Review ipc dispatch split".to_string(),
                priority: Some("p1".to_string()),
                tags: vec!["ipc".to_string()],
            },
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
            agent: AgentNode {
                model: Some(crate::models::AIModel::ClaudeSonnet4_5),
                model_ref: Some(crate::models::ModelRef::from_model(
                    crate::models::AIModel::ClaudeSonnet4_5,
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
            },
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
            skill: skill.clone(),
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
