use super::*;
#[tokio::test]
async fn execute_tool_browser_is_not_registered_in_core_runtime() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let tools_response =
        IpcServer::process(&core, &runtime_tool_registry, IpcRequest::GetAvailableTools).await;
    match tools_response {
        IpcResponse::Success(value) => {
            let tools = value
                .as_array()
                .expect("available tools should be an array");
            assert!(!tools.iter().any(|tool| tool.as_str() == Some("browser")));
        }
        other => panic!("expected available tools response, got {other:?}"),
    }

    let response = IpcServer::process(
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

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 500);
        }
        other => panic!("expected browser tool to be absent, got {other:?}"),
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
/// Skills are now registered as callable tools, not injected into the system prompt.
async fn build_agent_system_prompt_does_not_inject_skills() {
    let (core, _temp) = create_test_core().await;

    let mut variables = std::collections::HashMap::new();
    variables.insert("name".to_string(), "World".to_string());

    let agent_node = AgentNode::new()
        .with_prompt("Base prompt")
        .with_skills(vec!["skill-1".to_string()])
        .with_skill_variables(variables);

    let prompt = build_agent_system_prompt(&core, agent_node).unwrap();
    assert!(prompt.contains("Base prompt"));
    // Skills are now tools, not injected into prompt
    assert!(!prompt.contains("## Skill: Test Skill"));
}
