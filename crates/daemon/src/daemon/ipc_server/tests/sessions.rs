use super::*;
use types::request::ChildRunListQuery;
use types::{ChatSessionSource, ChatTurnStatus};

fn assert_execution_thread_error(
    response: IpcResponse,
    expected_code: i32,
    expected_message: &str,
) {
    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, expected_code);
            assert_eq!(error.message, expected_message);
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

fn chat_session_with_completed_turn(agent_id: &str, model: &str, turn_id: &str) -> ChatSession {
    let mut session = ChatSession::new(agent_id.to_string(), model.to_string());
    session.record_turn_user_message(turn_id, "hello");
    session.complete_turn_with_assistant_message(turn_id, "done");
    session
}

fn save_chat_session(core: &AppCore, session: &ChatSession) {
    core.storage
        .file_sessions
        .write_session(
            &crate::session_log::FileSession::from_chat_session(session),
            true,
        )
        .unwrap();
}

fn load_chat_session(core: &AppCore, session_id: &str) -> ChatSession {
    core.storage
        .file_sessions
        .get(session_id)
        .unwrap()
        .expect("session")
        .to_chat_session()
}

#[tokio::test]
async fn get_execution_run_thread_returns_not_found_for_missing_run() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetExecutionRunThread {
            run_id: "missing-run".to_string(),
        },
    )
    .await;

    assert_execution_thread_error(response, 404, "ExecutionThread not found");
}

#[tokio::test]
async fn get_execution_run_thread_returns_bad_request_for_blank_run_id() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetExecutionRunThread {
            run_id: "   ".to_string(),
        },
    )
    .await;

    assert_execution_thread_error(response, 400, "run_id is required");
}

#[tokio::test]
async fn get_execution_run_thread_returns_existing_run_thread() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let session = chat_session_with_completed_turn("agent-1", "gpt-5", "run-1");
    let session_id = session.id.clone();
    save_chat_session(&core, &session);

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetExecutionRunThread {
            run_id: "run-1".to_string(),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let thread: crate::ExecutionThread =
                serde_json::from_value(value).expect("execution thread");
            assert_eq!(thread.focus.run_id.as_deref(), Some("run-1"));
            assert_eq!(
                thread.focus.session_id.as_deref(),
                Some(session_id.as_str())
            );
            assert_eq!(thread.timeline.events.len(), 2);
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_child_runs_returns_bad_request_for_blank_parent_run_id() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ListChildRuns {
            query: ChildRunListQuery {
                parent_run_id: "   ".to_string(),
            },
        },
    )
    .await;

    assert_execution_thread_error(response, 400, "parent_run_id is required");
}

#[tokio::test]
async fn list_child_runs_returns_empty_for_leaf_runs() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let session = chat_session_with_completed_turn("agent-1", "gpt-5", "run-1");
    save_chat_session(&core, &session);

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ListChildRuns {
            query: ChildRunListQuery {
                parent_run_id: "run-1".to_string(),
            },
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let runs: Vec<crate::RunSummary> = serde_json::from_value(value).expect("child runs");
            assert!(runs.is_empty());
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn get_execution_run_auxiliary_requests_return_empty_payloads() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let session = chat_session_with_completed_turn("agent-1", "gpt-5", "run-1");
    save_chat_session(&core, &session);

    let timeline_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetExecutionRunTimeline {
            run_id: "run-1".to_string(),
        },
    )
    .await;
    match timeline_response {
        IpcResponse::Success(value) => {
            let timeline: crate::RunTimeline =
                serde_json::from_value(value).expect("execution timeline");
            assert_eq!(timeline.events.len(), 2);
        }
        other => panic!("expected timeline success response, got {other:?}"),
    }
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
    save_chat_session(&core, &session);

    persist_ipc_user_message_if_needed(&core, &mut session, Some("hello"), "hello").unwrap();

    let stored = load_chat_session(&core, &session.id);
    assert_eq!(stored.messages.len(), 1);
    assert_eq!(stored.messages[0].role, ChatRole::User);
    assert_eq!(stored.messages[0].content, "hello");
}

#[tokio::test]
async fn persist_ipc_user_message_if_needed_deduplicates_latest_user_turn() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    session.add_message(ChatMessage::user("hello"));
    save_chat_session(&core, &session);

    persist_ipc_user_message_if_needed(&core, &mut session, Some("hello"), "hello").unwrap();

    let stored = load_chat_session(&core, &session.id);
    assert_eq!(stored.messages.len(), 1);
}

#[tokio::test]
async fn record_turn_event_in_session_store_persists_tool_events() {
    let (core, _temp) = create_test_core().await;
    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    save_chat_session(&core, &session);

    record_turn_event_in_session_store(
        &core,
        &session.id,
        "turn-1",
        types::ChatTurnEventKind::ToolCall {
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: "pwd".to_string(),
        },
    )
    .unwrap();

    let stored = load_chat_session(&core, &session.id);
    assert_eq!(stored.turns.len(), 1);
    assert_eq!(stored.turns[0].events.len(), 1);
    assert!(matches!(
        stored.turns[0].events[0].kind,
        types::ChatTurnEventKind::ToolCall { .. }
    ));
}

#[tokio::test]
async fn persist_ipc_user_message_if_needed_auto_names_new_chat() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    save_chat_session(&core, &session);

    persist_ipc_user_message_if_needed(
        &core,
        &mut session,
        Some("hello from ipc"),
        "hello from ipc",
    )
    .unwrap();

    let stored = load_chat_session(&core, &session.id);
    assert_eq!(stored.name, "hello from ipc");
}

#[tokio::test]
async fn persist_ipc_user_message_if_needed_hydrates_voice_metadata() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    save_chat_session(&core, &session);

    persist_ipc_user_message_if_needed(
        &core,
        &mut session,
        Some("[Voice message]"),
        "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice.webm\n\n[Transcript]\nhello from audio",
    )
    .unwrap();

    let stored = load_chat_session(&core, &session.id);
    let user = stored.messages.last().expect("voice message");
    assert_eq!(user.role, ChatRole::User);
    assert_eq!(
        user.media.as_ref().map(|media| media.file_path.as_str()),
        Some("/tmp/voice.webm")
    );
    assert_eq!(
        user.transcript
            .as_ref()
            .map(|transcript| transcript.text.as_str()),
        Some("hello from audio")
    );
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
    let session_service = SessionService::from_storage(&core.storage);

    let mut workspace = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    workspace.source_channel = Some(ChatSessionSource::Workspace);
    assert!(session_service.is_workspace_managed(&workspace).unwrap());

    let legacy = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    assert!(session_service.is_workspace_managed(&legacy).unwrap());
}

#[tokio::test]
async fn search_sessions_applies_agent_filter_and_limit() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    for index in 0..3 {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.rename(format!("match agent one {index}"));
        session.add_message(ChatMessage::user("needle"));
        save_chat_session(&core, &session);
    }
    let mut other_agent = ChatSession::new("agent-2".to_string(), "gpt-5".to_string());
    other_agent.rename("match agent two");
    other_agent.add_message(ChatMessage::user("needle"));
    save_chat_session(&core, &other_agent);

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::SearchSessions {
            query: "needle".to_string(),
            agent_id: Some("agent-1".to_string()),
            limit: Some(2),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let sessions: Vec<types::ChatSessionSummary> =
                serde_json::from_value(value).expect("session summaries");
            assert_eq!(sessions.len(), 2);
            assert!(sessions.iter().all(|session| session.agent_id == "agent-1"));
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn apply_effective_session_source_defaults_to_workspace_when_no_external_route() {
    let (core, _temp) = create_test_core().await;
    let session_service = SessionService::from_storage(&core.storage);

    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    session_service
        .apply_effective_source(&mut session)
        .unwrap();
    assert_eq!(session.source_channel, Some(ChatSessionSource::Workspace));
    assert!(session.source_conversation_id.is_none());
}

#[tokio::test]
async fn steer_chat_stream_delivers_message_to_registered_stream() {
    let (core, _temp) = create_test_core().await;
    let session_service = SessionService::from_storage(&core.storage);
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    session.add_message(ChatMessage::user("start"));
    save_chat_session(&core, &session);
    let session_id = session.id.clone();
    let stream_id = format!("stream-{}", Uuid::new_v4());
    let turn_id = stream_id.clone();
    let (tx, mut rx) = mpsc::channel::<SteerMessage>(1);

    active_chat_stream_sessions().lock().await.insert(
        session_id.clone(),
        ActiveChatStreamBinding::new(stream_id.clone(), turn_id.clone(), None),
    );
    active_chat_stream_steers()
        .lock()
        .await
        .insert(stream_id.clone(), tx);

    let steered = steer_chat_stream(&core, &session_id, "continue with option B", None).await;
    assert!(steered);

    let message = rx.recv().await.expect("steer message");
    match message.command {
        SteerCommand::Message { instruction } => {
            assert_eq!(instruction, "continue with option B")
        }
        _ => panic!("expected message steer command"),
    }
    let stored = session_service
        .get_session_view(&session_id)
        .unwrap()
        .expect("stored session");
    assert!(stored.messages.iter().any(|message| {
        message.role == ChatRole::User && message.content == "continue with option B"
    }));
    let turn = stored
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .expect("stored turn");
    assert!(turn.events.iter().any(|event| matches!(
        &event.kind,
        ChatTurnEventKind::UserMessage { content } if content == "continue with option B"
    )));

    active_chat_stream_sessions()
        .lock()
        .await
        .remove(&session_id);
    active_chat_stream_steers().lock().await.remove(&stream_id);
}

#[tokio::test]
async fn steer_chat_stream_rejects_different_owner_scope() {
    let (core, _temp) = create_test_core().await;
    let session_id = format!("session-{}", Uuid::new_v4());
    let stream_id = format!("stream-{}", Uuid::new_v4());
    let owner_scope = types::ExecutionScope::foreground("client-a", "terminal-a");
    let other_scope = types::ExecutionScope::foreground("client-b", "terminal-b");
    let (tx, _rx) = mpsc::channel::<SteerMessage>(1);

    active_chat_stream_sessions().lock().await.insert(
        session_id.clone(),
        ActiveChatStreamBinding::new(
            stream_id.clone(),
            stream_id.clone(),
            Some(owner_scope.clone()),
        ),
    );
    active_chat_stream_steers()
        .lock()
        .await
        .insert(stream_id.clone(), tx);

    let steered = steer_chat_stream(&core, &session_id, "continue", Some(&other_scope)).await;
    assert!(!steered);

    active_chat_stream_sessions()
        .lock()
        .await
        .remove(&session_id);
    active_chat_stream_steers().lock().await.remove(&stream_id);
}

#[tokio::test]
async fn steer_chat_stream_returns_false_when_no_active_session_stream() {
    let (core, _temp) = create_test_core().await;
    let session_id = format!("session-{}", Uuid::new_v4());
    let steered = steer_chat_stream(&core, &session_id, "test", None).await;
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

#[tokio::test]
async fn session_reply_sender_ignores_blank_messages() {
    let buffer = Arc::new(Mutex::new(VecDeque::new()));
    let (tx, mut rx) = mpsc::unbounded_channel::<StreamFrame>();
    let sender = SessionReplySender::new(buffer.clone(), Some(tx));
    ReplySender::send(&sender, "   ".to_string()).await.unwrap();

    let guard = buffer.lock().await;
    assert!(guard.is_empty());
    drop(guard);

    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn ipc_stream_emitter_persists_assistant_segments_before_tools() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
    let turn_id = "turn-stream-segments".to_string();
    session.record_turn_user_message(&turn_id, "run tools");
    save_chat_session(&core, &session);
    let (tx, _rx) = mpsc::unbounded_channel::<StreamFrame>();
    let mut emitter = IpcStreamEmitter::new(
        core.clone(),
        session.id.clone(),
        turn_id.clone(),
        tx,
        Arc::new(AtomicBool::new(false)),
    );

    emitter.emit_text_delta("Planning first.").await;
    emitter
        .emit_tool_call_start("call-1", "bash", "{\"command\":\"pwd\"}")
        .await;
    emitter.emit_text_delta("Done.").await;
    emitter.emit_complete().await;

    let stored = SessionService::from_storage(&core.storage)
        .get_session_view(&session.id)
        .unwrap()
        .expect("stored session");
    let turn = stored
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .expect("stored turn");
    assert!(matches!(
        &turn.events[1].kind,
        ChatTurnEventKind::AssistantMessage { content } if content == "Planning first."
    ));
    assert!(matches!(
        &turn.events[2].kind,
        ChatTurnEventKind::ToolCall { call_id, .. } if call_id == "call-1"
    ));
    assert!(matches!(
        &turn.events[3].kind,
        ChatTurnEventKind::AssistantMessage { content } if content == "Done."
    ));
}

#[tokio::test]
async fn ipc_stream_emitter_persists_partial_assistant_segment_on_drop() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
    let turn_id = "turn-stream-cancel".to_string();
    session.record_turn_user_message(&turn_id, "cancel stream");
    save_chat_session(&core, &session);
    let (tx, _rx) = mpsc::unbounded_channel::<StreamFrame>();
    let mut emitter = IpcStreamEmitter::new(
        core.clone(),
        session.id.clone(),
        turn_id.clone(),
        tx,
        Arc::new(AtomicBool::new(false)),
    );

    emitter.emit_text_delta("partial answer").await;
    drop(emitter);

    let stored = SessionService::from_storage(&core.storage)
        .get_session_view(&session.id)
        .unwrap()
        .expect("stored session");
    let turn = stored
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .expect("stored turn");
    assert!(matches!(
        &turn.events[1].kind,
        ChatTurnEventKind::AssistantMessage { content } if content == "partial answer"
    ));
}

#[tokio::test]
async fn cancel_chat_stream_persists_partial_assistant_before_canceled_event() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
    let turn_id = "turn-stream-cancel-order".to_string();
    session.record_turn_user_message(&turn_id, "cancel stream");
    save_chat_session(&core, &session);
    let session_id = session.id.clone();
    let (tx, _rx) = mpsc::unbounded_channel::<StreamFrame>();
    let (emitted_tx, emitted_rx) = tokio::sync::oneshot::channel::<()>();
    let worker_core = core.clone();
    let worker_session_id = session_id.clone();
    let worker_turn_id = turn_id.clone();
    let handle = tokio::spawn(async move {
        let mut emitter = IpcStreamEmitter::new(
            worker_core,
            worker_session_id,
            worker_turn_id,
            tx,
            Arc::new(AtomicBool::new(false)),
        );
        emitter.emit_text_delta("partial answer").await;
        let _ = emitted_tx.send(());
        std::future::pending::<()>().await;
    });
    active_chat_streams()
        .lock()
        .await
        .insert(turn_id.clone(), handle);
    active_chat_stream_sessions().lock().await.insert(
        session_id.clone(),
        ActiveChatStreamBinding::new(turn_id.clone(), turn_id.clone(), None),
    );
    emitted_rx.await.expect("worker emitted partial answer");

    assert!(cancel_chat_stream(&core, &turn_id).await);

    let stored = SessionService::from_storage(&core.storage)
        .get_session_view(&session_id)
        .unwrap()
        .expect("stored session");
    let turn = stored
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .expect("stored turn");
    assert_eq!(turn.status, ChatTurnStatus::Canceled);
    assert!(matches!(
        &turn.events[1].kind,
        ChatTurnEventKind::AssistantMessage { content } if content == "partial answer"
    ));
    assert!(matches!(turn.events[2].kind, ChatTurnEventKind::Canceled));
}

#[tokio::test]
async fn daemon_execute_chat_session_request_is_unsupported() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteChatSession {
            session_id: "session-1".to_string(),
            user_input: Some("hello".to_string()),
            workspace_root: None,
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, -3);
            assert_eq!(error.kind, types::ErrorKind::Protocol);
            assert_eq!(
                error.message,
                "Foreground chat execution runs in the TUI process"
            );
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn daemon_execute_chat_session_stream_request_is_unsupported() {
    let (core, _temp) = create_test_core().await;

    let err = IpcServer::open_stream(
        core,
        IpcRequest::ExecuteChatSessionStream {
            session_id: "session-1".to_string(),
            user_input: Some("hello".to_string()),
            stream_id: "turn-1".to_string(),
            workspace_root: None,
            scope: None,
        },
    )
    .await
    .expect_err("daemon foreground stream should be unsupported");

    assert_eq!(
        err.to_string(),
        "Foreground chat streaming runs in the TUI process"
    );
}

#[tokio::test]
async fn foreground_chat_stream_reports_missing_session_without_daemon_ipc() {
    let (core, _temp) = create_test_core().await;
    let mut rx = open_foreground_chat_session_stream(
        core,
        "missing-session".to_string(),
        Some("hello".to_string()),
        "turn-missing".to_string(),
        None,
    )
    .await
    .expect("foreground stream");

    let first = rx.recv().await.expect("start frame");
    assert!(matches!(first, StreamFrame::Start { .. }));
    let second = rx.recv().await.expect("error frame");
    assert!(matches!(
        second,
        StreamFrame::Error(error) if error.message == "Session not found"
    ));
}

#[tokio::test]
async fn add_message_returns_bad_request_for_invalid_role_payload() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::AddMessage {
            session_id: "missing-session".to_string(),
            role: "not_a_role".to_string(),
            content: "hello".to_string(),
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 400);
            assert_eq!(error.kind, types::ErrorKind::Validation);
            assert!(error.message.contains("Invalid request payload"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}
