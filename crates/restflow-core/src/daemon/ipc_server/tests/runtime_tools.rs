use super::*;
#[tokio::test]
async fn execute_tool_browser_session_persists_between_process_calls() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let create_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteTool {
            name: "browser".to_string(),
            input: serde_json::json!({
                "action": "new_session",
                "headless": true
            }),
        },
    )
    .await;

    let session_id = match create_response {
        IpcResponse::Success(value) => {
            assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(true));
            value
                .get("result")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
                .map(|v| v.to_string())
                .expect("browser new_session should return an id")
        }
        other => panic!("expected success response, got {other:?}"),
    };

    let list_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteTool {
            name: "browser".to_string(),
            input: serde_json::json!({
                "action": "list_sessions"
            }),
        },
    )
    .await;

    match list_response {
        IpcResponse::Success(value) => {
            assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(true));
            let sessions = value
                .get("result")
                .and_then(|v| v.as_array())
                .expect("browser list_sessions should return an array");
            assert!(
                sessions.iter().any(|session| {
                    session.get("id").and_then(|v| v.as_str()) == Some(session_id.as_str())
                }),
                "created browser session should be visible in list_sessions"
            );
        }
        other => panic!("expected success response, got {other:?}"),
    }

    let close_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteTool {
            name: "browser".to_string(),
            input: serde_json::json!({
                "action": "close_session",
                "session_id": session_id
            }),
        },
    )
    .await;

    match close_response {
        IpcResponse::Success(value) => {
            assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(true));
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_tool_failure_includes_structured_error_metadata() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteTool {
            name: "bash".to_string(),
            input: serde_json::json!({
                "command": "definitely_not_a_real_command_restflow_12345",
                "yolo_mode": true
            }),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let result: ToolExecutionResult =
                serde_json::from_value(value.clone()).expect("tool result should deserialize");
            assert!(!result.success);
            assert!(result.error.is_some());
            assert_eq!(result.error_category, Some(ToolErrorCategory::Config));
            assert_eq!(result.retryable, Some(false));
            assert_eq!(result.retry_after_ms, None);

            assert_eq!(value["error_category"], "Config");
            assert_eq!(value["retryable"], false);
            assert!(value.get("retry_after_ms").is_some());
        }
        other => panic!("expected success response with failed tool payload, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_tool_manage_teams_start_team_is_available() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteTool {
            name: "manage_teams".to_string(),
            input: serde_json::json!({
                "operation": "start_team",
                "members": [
                    { "agent_id": "default" }
                ]
            }),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let result: ToolExecutionResult =
                serde_json::from_value(value).expect("tool result should deserialize");
            assert!(result.success, "manage_teams should succeed");
            assert_eq!(result.result["operation"], "start_team");
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
/// Skills are now registered as callable tools, not injected into the system prompt.
async fn build_agent_system_prompt_does_not_inject_skills() {
    let (core, _temp) = create_test_core().await;

    let skill = Skill::new(
        "skill-1".to_string(),
        "Test Skill".to_string(),
        None,
        None,
        "Hello {{name}}".to_string(),
    );
    core.storage.skills.create(&skill).unwrap();

    let mut variables = std::collections::HashMap::new();
    variables.insert("name".to_string(), "World".to_string());

    let agent_node = AgentNode::new()
        .with_prompt("Base prompt")
        .with_skills(vec![skill.id.clone()])
        .with_skill_variables(variables);

    let prompt = build_agent_system_prompt(&core, agent_node).unwrap();
    assert!(prompt.contains("Base prompt"));
    // Skills are now tools, not injected into prompt
    assert!(!prompt.contains("## Skill: Test Skill"));
}
