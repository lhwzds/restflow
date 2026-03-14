use super::runtime::{
    build_agent_system_prompt, load_chat_max_session_history_from_core,
    persist_ipc_user_message_if_needed, steer_chat_stream, subagent_config_from_defaults,
};
use super::*;
use crate::models::{AgentNode, ChannelSessionBinding, Skill};
use restflow_traits::SteerCommand;
use restflow_traits::store::ReplySender;
use restflow_traits::tool::ToolErrorCategory;
use tempfile::tempdir;
use uuid::Uuid;

async fn create_test_core() -> (Arc<AppCore>, tempfile::TempDir) {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("ipc-server-test.db");
    let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
    (core, temp)
}

#[tokio::test]
async fn resolve_chat_stream_trace_uses_session_agent_and_run_turn_id() {
    let (core, _temp) = create_test_core().await;
    let session = ChatSession::new("agent-trace".to_string(), "gpt-5".to_string());
    core.storage.chat_sessions.create(&session).unwrap();

    let trace = resolve_chat_stream_trace(&core, &session.id, "stream-123");

    assert_eq!(trace.run_id, "stream-123");
    assert_eq!(trace.parent_run_id, None);
    assert_eq!(trace.turn_id, "run-stream-123");
    assert_eq!(trace.session_id, session.id);
    assert_eq!(trace.scope_id, session.id);
    assert_eq!(trace.actor_id, "agent-trace");
}

#[tokio::test]
async fn resolve_chat_stream_trace_falls_back_when_session_is_missing() {
    let (core, _temp) = create_test_core().await;

    let trace = resolve_chat_stream_trace(&core, "missing-session", "stream-123");

    assert_eq!(trace.run_id, "stream-123");
    assert_eq!(trace.parent_run_id, None);
    assert_eq!(trace.turn_id, "run-stream-123");
    assert_eq!(trace.session_id, "missing-session");
    assert_eq!(trace.scope_id, "missing-session");
    assert_eq!(trace.actor_id, UNKNOWN_TRACE_ACTOR_ID);
}

#[test]
fn subagent_config_from_defaults_maps_max_iterations() {
    let defaults = AgentDefaults {
        max_parallel_subagents: 21,
        subagent_timeout_secs: 1200,
        max_iterations: 111,
        max_depth: 4,
        ..AgentDefaults::default()
    };

    let config = subagent_config_from_defaults(&defaults);

    assert_eq!(config.max_parallel_agents, 21);
    assert_eq!(config.subagent_timeout_secs, 1200);
    assert_eq!(config.max_iterations, 111);
    assert_eq!(config.max_depth, 4);
}

#[tokio::test]
async fn load_chat_max_session_history_from_core_uses_runtime_config() {
    let (core, _temp) = create_test_core().await;
    let mut config = core.storage.config.get_effective_config().unwrap();
    config.runtime_defaults.chat_max_session_history = 42;
    core.storage.config.update_config(config).unwrap();

    assert_eq!(load_chat_max_session_history_from_core(&core), 42);
}

#[tokio::test]
async fn persist_ipc_user_message_if_needed_adds_missing_user_turn() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    core.storage.chat_sessions.create(&session).unwrap();

    persist_ipc_user_message_if_needed(&core, &mut session, Some("hello"), "hello").unwrap();

    let stored = core
        .storage
        .chat_sessions
        .get(&session.id)
        .unwrap()
        .expect("session");
    assert_eq!(stored.messages.len(), 1);
    assert_eq!(stored.messages[0].role, ChatRole::User);
    assert_eq!(stored.messages[0].content, "hello");
}

#[tokio::test]
async fn persist_ipc_user_message_if_needed_deduplicates_latest_user_turn() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    session.add_message(ChatMessage::user("hello"));
    core.storage.chat_sessions.create(&session).unwrap();

    persist_ipc_user_message_if_needed(&core, &mut session, Some("hello"), "hello").unwrap();

    let stored = core
        .storage
        .chat_sessions
        .get(&session.id)
        .unwrap()
        .expect("session");
    assert_eq!(stored.messages.len(), 1);
}

#[tokio::test]
async fn persist_ipc_user_message_if_needed_auto_names_new_chat() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    core.storage.chat_sessions.create(&session).unwrap();

    persist_ipc_user_message_if_needed(
        &core,
        &mut session,
        Some("hello from ipc"),
        "hello from ipc",
    )
    .unwrap();

    let stored = core
        .storage
        .chat_sessions
        .get(&session.id)
        .unwrap()
        .expect("session");
    assert_eq!(stored.name, "hello from ipc");
}

#[test]
fn normalize_model_input_converts_to_serialized_form() {
    assert_eq!(
        normalize_model_input("MiniMax-M2.5").unwrap(),
        "minimax-m2-5"
    );
    assert_eq!(normalize_model_input("gpt-5.1").unwrap(), "gpt-5-1");
}

#[test]
fn normalize_model_input_rejects_unknown_value() {
    assert!(normalize_model_input("not-a-real-model").is_err());
}

#[tokio::test]
async fn is_workspace_managed_session_accepts_sessions_without_channel_bindings() {
    let (core, _temp) = create_test_core().await;

    let mut workspace = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    workspace.source_channel = Some(ChatSessionSource::Workspace);
    assert!(is_workspace_managed_session(&core.storage, &workspace).unwrap());

    let legacy = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    assert!(is_workspace_managed_session(&core.storage, &legacy).unwrap());
}

#[tokio::test]
async fn is_workspace_managed_session_rejects_sessions_with_channel_bindings() {
    let (core, _temp) = create_test_core().await;

    let mut telegram = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    telegram.source_channel = Some(ChatSessionSource::Telegram);
    core.storage.chat_sessions.create(&telegram).unwrap();
    core.storage
        .channel_session_bindings
        .upsert(&ChannelSessionBinding::new(
            "telegram",
            None,
            "chat-123",
            &telegram.id,
        ))
        .unwrap();

    assert!(!is_workspace_managed_session(&core.storage, &telegram).unwrap());
}

#[tokio::test]
async fn is_workspace_managed_session_rejects_legacy_external_and_backfills_binding() {
    let (core, _temp) = create_test_core().await;

    let telegram = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
        .with_source(ChatSessionSource::Telegram, "chat-legacy");
    core.storage.chat_sessions.create(&telegram).unwrap();

    assert!(!is_workspace_managed_session(&core.storage, &telegram).unwrap());

    let binding = core
        .storage
        .channel_session_bindings
        .get_by_route("telegram", None, "chat-legacy")
        .unwrap()
        .expect("legacy external route should be backfilled");
    assert_eq!(binding.session_id, telegram.id);
}

#[tokio::test]
async fn delete_session_rejects_background_bound_workspace_session() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    session.source_channel = Some(ChatSessionSource::Workspace);
    core.storage.chat_sessions.create(&session).unwrap();

    core.storage
        .background_agents
        .create_background_agent(crate::models::BackgroundAgentSpec {
            name: "bound-task".to_string(),
            agent_id: "agent-1".to_string(),
            chat_session_id: Some(session.id.clone()),
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
        })
        .unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::DeleteSession {
            id: session.id.clone(),
        },
    )
    .await;
    match response {
        IpcResponse::Error { code, message, .. } => {
            assert_eq!(code, 409);
            assert!(message.contains("bound to background task"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn archive_session_rejects_background_bound_workspace_session() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    session.source_channel = Some(ChatSessionSource::Workspace);
    core.storage.chat_sessions.create(&session).unwrap();

    core.storage
        .background_agents
        .create_background_agent(crate::models::BackgroundAgentSpec {
            name: "bound-task".to_string(),
            agent_id: "agent-1".to_string(),
            chat_session_id: Some(session.id.clone()),
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
        })
        .unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ArchiveSession {
            id: session.id.clone(),
        },
    )
    .await;
    match response {
        IpcResponse::Error { code, message, .. } => {
            assert_eq!(code, 409);
            assert!(message.contains("bound to background task"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_get_background_agent_returns_created_task() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let task = core
        .storage
        .background_agents
        .create_background_agent(crate::models::BackgroundAgentSpec {
            name: "ipc-background".to_string(),
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
        })
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
async fn process_search_memory_returns_matching_chunk() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let agent = core
        .storage
        .agents
        .create_agent(
            "Test Agent".to_string(),
            AgentNode {
                model: Some(crate::models::AIModel::ClaudeSonnet4_5),
                model_ref: Some(crate::models::ModelRef::from_model(
                    crate::models::AIModel::ClaudeSonnet4_5,
                )),
                prompt: Some("You are a helpful assistant".to_string()),
                temperature: Some(0.7),
                codex_cli_reasoning_effort: None,
                codex_cli_execution_mode: None,
                api_key_config: Some(crate::models::ApiKeyConfig::Direct("test_key".to_string())),
                tools: Some(vec!["add".to_string()]),
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
async fn apply_effective_session_source_uses_binding_data() {
    let (core, _temp) = create_test_core().await;

    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
        .with_source(ChatSessionSource::Workspace, "stale-conv");
    core.storage.chat_sessions.create(&session).unwrap();
    core.storage
        .channel_session_bindings
        .upsert(&ChannelSessionBinding::new(
            "telegram",
            None,
            "chat-888",
            &session.id,
        ))
        .unwrap();

    apply_effective_session_source(&core.storage, &mut session).unwrap();
    assert_eq!(session.source_channel, Some(ChatSessionSource::Telegram));
    assert_eq!(session.source_conversation_id.as_deref(), Some("chat-888"));
}

#[tokio::test]
async fn apply_effective_session_source_backfills_legacy_external_binding() {
    let (core, _temp) = create_test_core().await;

    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
        .with_source(ChatSessionSource::Telegram, "legacy-conv");
    apply_effective_session_source(&core.storage, &mut session).unwrap();
    assert_eq!(session.source_channel, Some(ChatSessionSource::Telegram));
    assert_eq!(
        session.source_conversation_id.as_deref(),
        Some("legacy-conv")
    );

    let binding = core
        .storage
        .channel_session_bindings
        .get_by_route("telegram", None, "legacy-conv")
        .unwrap()
        .expect("legacy route should be backfilled");
    assert_eq!(binding.session_id, session.id);
}

#[tokio::test]
async fn apply_effective_session_source_defaults_to_workspace_when_no_external_route() {
    let (core, _temp) = create_test_core().await;

    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    apply_effective_session_source(&core.storage, &mut session).unwrap();
    assert_eq!(session.source_channel, Some(ChatSessionSource::Workspace));
    assert!(session.source_conversation_id.is_none());
}

#[test]
fn build_rebuilt_external_session_preserves_binding_and_runtime_config() {
    let mut source = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
        .with_source(ChatSessionSource::Telegram, "chat-123")
        .with_name("channel:chat-123")
        .with_skill("skill-1")
        .with_retention("7d");
    source.source_conversation_id = Some("chat-123".to_string());

    let rebuilt = build_rebuilt_external_session(&source, ChatSessionSource::Telegram, "chat-123")
        .expect("rebuilt session");
    assert_ne!(rebuilt.id, source.id);
    assert_eq!(rebuilt.agent_id, source.agent_id);
    assert_eq!(rebuilt.model, source.model);
    assert_eq!(rebuilt.skill_id, source.skill_id);
    assert_eq!(rebuilt.retention, source.retention);
    assert_eq!(rebuilt.source_channel, Some(ChatSessionSource::Telegram));
    assert_eq!(rebuilt.source_conversation_id.as_deref(), Some("chat-123"));
    assert_eq!(rebuilt.name, "channel:chat-123");
}

#[test]
fn build_rebuilt_external_session_rejects_workspace_session() {
    let mut source = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    source.source_channel = Some(ChatSessionSource::Workspace);
    source.source_conversation_id = Some("chat-123".to_string());
    let err = build_rebuilt_external_session(&source, ChatSessionSource::Workspace, "chat-123")
        .expect_err("should fail");
    assert!(err.to_string().contains("not externally managed"));
}

#[tokio::test]
async fn resolve_external_session_route_prefers_binding_over_legacy_fields() {
    let (core, _temp) = create_test_core().await;

    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
        .with_source(ChatSessionSource::Telegram, "legacy-chat");
    session.source_conversation_id = Some("legacy-chat".to_string());
    core.storage.chat_sessions.create(&session).unwrap();
    core.storage
        .channel_session_bindings
        .upsert(&ChannelSessionBinding::new(
            "discord",
            None,
            "binding-chat",
            &session.id,
        ))
        .unwrap();

    let (channel, conversation_id) =
        resolve_external_session_route(&core.storage, &session).unwrap();
    assert_eq!(channel, ChatSessionSource::Discord);
    assert_eq!(conversation_id, "binding-chat");
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

#[tokio::test]
async fn steer_chat_stream_delivers_message_to_registered_stream() {
    let session_id = format!("session-{}", Uuid::new_v4());
    let stream_id = format!("stream-{}", Uuid::new_v4());
    let (tx, mut rx) = mpsc::channel::<SteerMessage>(1);

    active_chat_stream_sessions()
        .lock()
        .await
        .insert(session_id.clone(), stream_id.clone());
    active_chat_stream_steers()
        .lock()
        .await
        .insert(stream_id.clone(), tx);

    let steered = steer_chat_stream(&session_id, "continue with option B").await;
    assert!(steered);

    let message = rx.recv().await.expect("steer message");
    match message.command {
        SteerCommand::Message { instruction } => {
            assert_eq!(instruction, "continue with option B")
        }
        _ => panic!("expected message steer command"),
    }

    active_chat_stream_sessions()
        .lock()
        .await
        .remove(&session_id);
    active_chat_stream_steers().lock().await.remove(&stream_id);
}

#[tokio::test]
async fn steer_chat_stream_returns_false_when_no_active_session_stream() {
    let session_id = format!("session-{}", Uuid::new_v4());
    let steered = steer_chat_stream(&session_id, "test").await;
    assert!(!steered);
}

#[tokio::test]
async fn session_reply_sender_buffers_message_and_emits_ack_frame() {
    let buffer = Arc::new(Mutex::new(VecDeque::new()));
    let (tx, mut rx) = mpsc::unbounded_channel::<StreamFrame>();
    let sender = SessionReplySender::new(buffer.clone(), Some(tx));
    ReplySender::send(&sender, "Working on it".to_string())
        .await
        .unwrap();

    let mut guard = buffer.lock().await;
    assert_eq!(guard.pop_front(), Some("Working on it".to_string()));
    drop(guard);

    let frame = rx.recv().await.expect("ack stream frame");
    match frame {
        StreamFrame::Ack { content } => assert_eq!(content, "Working on it"),
        _ => panic!("expected ack frame"),
    }
}
