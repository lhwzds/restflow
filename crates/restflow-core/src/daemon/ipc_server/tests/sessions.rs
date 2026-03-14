use super::*;
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
