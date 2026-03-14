use super::*;
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
            agent_node: AgentNode::new().with_prompt("Base prompt"),
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
