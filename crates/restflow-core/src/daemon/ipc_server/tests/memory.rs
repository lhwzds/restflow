use super::*;
#[tokio::test]
async fn process_search_memory_returns_matching_chunk() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let agent = core
        .storage
        .agents
        .create_agent(
            "Test Agent".to_string(),
            AgentNode {
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
            },
        )
        .unwrap();

    let chunk = crate::models::MemoryChunk::new(agent.id.clone(), "remember this note".to_string());
    core.storage.memory.store_chunk(&chunk).unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::SearchMemory {
            query: "remember".to_string(),
            agent_id: Some(agent.id.clone()),
            limit: Some(10),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let result: crate::models::MemorySearchResult =
                serde_json::from_value(value).expect("memory search result");
            assert_eq!(result.total_count, 1);
            assert_eq!(result.chunks.len(), 1);
            assert_eq!(result.chunks[0].id, chunk.id);
        }
        other => panic!("expected success response, got {other:?}"),
    }
}
