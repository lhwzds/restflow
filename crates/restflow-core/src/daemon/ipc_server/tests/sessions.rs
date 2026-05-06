use super::*;
use crate::models::ChatSessionSource;
use crate::storage::Storage;
use crate::storage::simple_storage::{ChatSessionRawStorage, SimpleStorage};
use crate::{
    ExecutionTraceCategory, ExecutionTraceSource, LifecycleTrace, LogRecordTrace, MetricSampleTrace,
};
use restflow_contracts::request::{ChildRunListQuery, WireModelRef};

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

fn store_run_events(
    storage: &Arc<Storage>,
    task_id: &str,
    session_id: &str,
    run_id: &str,
    parent_run_id: Option<&str>,
) {
    let trace = restflow_ai::telemetry::RestflowTrace::new(
        run_id.to_string(),
        session_id.to_string(),
        task_id.to_string(),
        "agent-1".to_string(),
    )
    .with_parent_run_id(parent_run_id.map(|value| value.to_string()));
    let start = crate::models::execution_trace_builders::with_provider(
        crate::models::execution_trace_builders::with_effective_model(
            crate::models::execution_trace_builders::with_trace_context(
                crate::models::execution_trace_builders::lifecycle(
                    task_id,
                    "agent-1",
                    LifecycleTrace {
                        status: "running".to_string(),
                        message: Some("started".to_string()),
                        error: None,
                        ai_duration_ms: None,
                    },
                ),
                &trace,
            ),
            "openai/gpt-5",
        ),
        "openai",
    );
    let end = crate::models::execution_trace_builders::with_provider(
        crate::models::execution_trace_builders::with_effective_model(
            crate::models::execution_trace_builders::with_lifecycle(
                crate::models::execution_trace_builders::with_trace_context(
                    crate::models::execution_trace_builders::new_event(
                        task_id,
                        "agent-1",
                        ExecutionTraceCategory::Lifecycle,
                        ExecutionTraceSource::Runtime,
                    ),
                    &trace,
                ),
                LifecycleTrace {
                    status: "completed".to_string(),
                    message: Some("done".to_string()),
                    error: None,
                    ai_duration_ms: Some(1200),
                },
            ),
            "openai/gpt-5",
        ),
        "openai",
    );
    storage.execution_traces.store(&start).expect("store start");
    storage.execution_traces.store(&end).expect("store end");
}

fn store_run_telemetry(storage: &Arc<Storage>, task_id: &str, session_id: &str, run_id: &str) {
    let trace = restflow_ai::telemetry::RestflowTrace::new(run_id, session_id, task_id, "agent-1");
    let metric = crate::models::execution_trace_builders::with_trace_context(
        crate::models::execution_trace_builders::metric_sample(
            task_id,
            "agent-1",
            MetricSampleTrace {
                name: "llm_total_tokens".to_string(),
                value: 42.0,
                unit: Some("tokens".to_string()),
                dimensions: Vec::new(),
            },
        ),
        &trace,
    );
    let log = crate::models::execution_trace_builders::with_trace_context(
        crate::models::execution_trace_builders::log_record(
            task_id,
            "agent-1",
            LogRecordTrace {
                level: "warn".to_string(),
                message: format!("log-{run_id}"),
                fields: Vec::new(),
            },
        ),
        &trace,
    );
    storage
        .execution_traces
        .store(&metric)
        .expect("store metric");
    storage.execution_traces.store(&log).expect("store log");
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

    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    let session_id = session.id.clone();
    core.storage.chat_sessions.create(&session).unwrap();
    store_run_events(&core.storage, "task-1", &session_id, "run-1", None);

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
            assert!(thread.timeline.events.len() >= 2);
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

    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    let session_id = session.id.clone();
    core.storage.chat_sessions.create(&session).unwrap();
    store_run_events(&core.storage, "task-1", &session_id, "run-1", None);

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
async fn list_child_runs_returns_direct_children_for_parent_runs() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    let session_id = session.id.clone();
    core.storage.chat_sessions.create(&session).unwrap();
    store_run_events(&core.storage, "task-1", &session_id, "run-parent", None);
    store_run_events(
        &core.storage,
        "task-1",
        &session_id,
        "run-child",
        Some("run-parent"),
    );

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ListChildRuns {
            query: ChildRunListQuery {
                parent_run_id: "run-parent".to_string(),
            },
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let runs: Vec<crate::RunSummary> = serde_json::from_value(value).expect("child runs");
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].run_id.as_deref(), Some("run-child"));
            assert_eq!(runs[0].parent_run_id.as_deref(), Some("run-parent"));
            assert_eq!(runs[0].root_run_id.as_deref(), Some("run-parent"));
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn get_execution_trace_stats_filters_by_run_id() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    let session_id = session.id.clone();
    core.storage.chat_sessions.create(&session).unwrap();
    store_run_events(&core.storage, "task-1", &session_id, "run-1", None);
    store_run_events(&core.storage, "task-1", &session_id, "run-2", None);

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetExecutionTraceStats {
            run_id: Some("run-1".to_string()),
        },
    )
    .await;

    match response {
        IpcResponse::Success(value) => {
            let stats: crate::ExecutionTraceStats =
                serde_json::from_value(value).expect("execution trace stats");
            assert_eq!(stats.total_events, 2);
            assert_eq!(stats.lifecycle_count, 2);
        }
        other => panic!("expected success response, got {other:?}"),
    }
}

#[tokio::test]
async fn get_execution_trace_stats_rejects_blank_run_id_filter() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetExecutionTraceStats {
            run_id: Some("   ".to_string()),
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 400);
            assert!(error.message.contains("run_id is required"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn get_execution_run_telemetry_requests_filter_by_run_id() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    let session_id = session.id.clone();
    core.storage.chat_sessions.create(&session).unwrap();
    store_run_events(&core.storage, "task-1", &session_id, "run-1", None);
    store_run_events(&core.storage, "task-1", &session_id, "run-2", None);
    store_run_telemetry(&core.storage, "task-1", &session_id, "run-1");
    store_run_telemetry(&core.storage, "task-1", &session_id, "run-2");

    let timeline_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetExecutionRunTimeline {
            run_id: "run-1".to_string(),
        },
    )
    .await;
    let metrics_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::GetExecutionRunMetrics {
            run_id: "run-1".to_string(),
        },
    )
    .await;
    let logs_response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::QueryExecutionRunLogs {
            run_id: "run-1".to_string(),
        },
    )
    .await;

    match timeline_response {
        IpcResponse::Success(value) => {
            let timeline: crate::ExecutionTimeline =
                serde_json::from_value(value).expect("execution timeline");
            assert_eq!(timeline.events.len(), 4);
            assert!(
                timeline
                    .events
                    .iter()
                    .all(|event| event.run_id.as_deref() == Some("run-1"))
            );
        }
        other => panic!("expected timeline success response, got {other:?}"),
    }

    match metrics_response {
        IpcResponse::Success(value) => {
            let metrics: crate::ExecutionMetricsResponse =
                serde_json::from_value(value).expect("execution metrics");
            assert_eq!(metrics.samples.len(), 1);
            assert_eq!(metrics.samples[0].run_id.as_deref(), Some("run-1"));
        }
        other => panic!("expected metrics success response, got {other:?}"),
    }

    match logs_response {
        IpcResponse::Success(value) => {
            let logs: crate::ExecutionLogResponse =
                serde_json::from_value(value).expect("execution logs");
            assert_eq!(logs.events.len(), 1);
            assert_eq!(logs.events[0].run_id.as_deref(), Some("run-1"));
        }
        other => panic!("expected logs success response, got {other:?}"),
    }
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
async fn record_turn_event_in_session_store_persists_tool_events() {
    let (core, _temp) = create_test_core().await;
    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    core.storage.chat_sessions.create(&session).unwrap();

    record_turn_event_in_session_store(
        &core,
        &session.id,
        "turn-1",
        crate::models::ChatTurnEventKind::ToolCall {
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: "pwd".to_string(),
        },
    )
    .unwrap();

    let stored = core
        .storage
        .chat_sessions
        .get(&session.id)
        .unwrap()
        .expect("session");
    assert_eq!(stored.turns.len(), 1);
    assert_eq!(stored.turns[0].events.len(), 1);
    assert!(matches!(
        stored.turns[0].events[0].kind,
        crate::models::ChatTurnEventKind::ToolCall { .. }
    ));
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

#[tokio::test]
async fn persist_ipc_user_message_if_needed_hydrates_voice_metadata() {
    let (core, _temp) = create_test_core().await;
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    core.storage.chat_sessions.create(&session).unwrap();

    persist_ipc_user_message_if_needed(
        &core,
        &mut session,
        Some("[Voice message]"),
        "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice.webm\n\n[Transcript]\nhello from audio",
    )
    .unwrap();

    let stored = core
        .storage
        .chat_sessions
        .get(&session.id)
        .unwrap()
        .expect("session");
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
async fn delete_session_rejects_background_bound_workspace_session() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    session.source_channel = Some(ChatSessionSource::Workspace);
    core.storage.chat_sessions.create(&session).unwrap();

    core.storage
        .tasks
        .create_task_from_spec(crate::models::TaskSpec {
            name: "bound-task".to_string(),
            agent_id: "agent-1".to_string(),
            chat_session_id: Some(session.id.clone()),
            description: None,
            input: Some("run".to_string()),
            input_template: None,
            schedule: crate::models::TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: None,
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
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 409);
            assert!(error.message.contains("bound to task"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn switch_session_model_rejects_background_bound_workspace_session() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    session.source_channel = Some(ChatSessionSource::Workspace);
    core.storage.chat_sessions.create(&session).unwrap();

    core.storage
        .tasks
        .create_task_from_spec(crate::models::TaskSpec {
            name: "bound-task".to_string(),
            agent_id: "agent-1".to_string(),
            chat_session_id: Some(session.id.clone()),
            description: None,
            input: Some("run".to_string()),
            input_template: None,
            schedule: crate::models::TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::SwitchSessionModel {
            session_id: session.id.clone(),
            model_ref: WireModelRef {
                provider: "openai".to_string(),
                model: "gpt-5.5".to_string(),
            },
            reason: None,
        },
    )
    .await;
    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 409);
            assert!(error.message.contains("bound to task"));
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
        .tasks
        .create_task_from_spec(crate::models::TaskSpec {
            name: "bound-task".to_string(),
            agent_id: "agent-1".to_string(),
            chat_session_id: Some(session.id.clone()),
            description: None,
            input: Some("run".to_string()),
            input_template: None,
            schedule: crate::models::TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: None,
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
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 409);
            assert!(error.message.contains("bound to task"));
        }
        other => panic!("expected error response, got {other:?}"),
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
async fn execute_chat_session_returns_not_found_for_missing_session() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteChatSession {
            session_id: "missing-session".to_string(),
            user_input: None,
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 404);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::NotFound);
            assert_eq!(error.message, "Session not found");
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_chat_session_returns_bad_request_without_user_message() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    core.storage.chat_sessions.create(&session).unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteChatSession {
            session_id: session.id.clone(),
            user_input: None,
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 400);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::Validation);
            assert_eq!(error.message, "No user message found in session");
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_chat_session_persists_voice_message_when_preprocess_fails() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    core.storage.chat_sessions.create(&session).unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteChatSession {
            session_id: session.id.clone(),
            user_input: Some(
                "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice.webm".to_string(),
            ),
        },
    )
    .await;

    match response {
        IpcResponse::Error(error) => {
            assert_eq!(error.code, 400);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::Validation);
            assert!(error.message.contains("Voice transcription failed:"));
        }
        other => panic!("expected error response, got {other:?}"),
    }

    let stored = core
        .storage
        .chat_sessions
        .get(&session.id)
        .unwrap()
        .expect("session");
    assert_eq!(stored.messages.len(), 1);
    assert_eq!(stored.messages[0].role, ChatRole::User);
    assert!(stored.messages[0].content.contains("media_type: voice"));
    assert!(!stored.messages[0].content.contains("instruction:"));
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
            assert_eq!(error.kind, restflow_contracts::ErrorKind::Validation);
            assert!(error.message.contains("Invalid request payload"));
        }
        other => panic!("expected error response, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_chat_session_returns_internal_error_for_malformed_session_payload() {
    let (core, _temp) = create_test_core().await;
    let runtime_tool_registry = OnceLock::new();
    let raw_storage = ChatSessionRawStorage::new(core.storage.get_db()).unwrap();

    raw_storage.put_raw("bad-session", b"{bad-json").unwrap();

    let response = IpcServer::process(
        &core,
        &runtime_tool_registry,
        IpcRequest::ExecuteChatSession {
            session_id: "bad-session".to_string(),
            user_input: None,
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
