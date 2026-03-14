use super::*;

pub(super) fn create_runtime_tool_registry(
    core: &Arc<AppCore>,
) -> anyhow::Result<restflow_ai::tools::ToolRegistry> {
    create_tool_registry(
        core.storage.skills.clone(),
        core.storage.memory.clone(),
        core.storage.chat_sessions.clone(),
        core.storage.channel_session_bindings.clone(),
        core.storage.tool_traces.clone(),
        core.storage.kv_store.clone(),
        core.storage.work_items.clone(),
        core.storage.secrets.clone(),
        core.storage.config.clone(),
        core.storage.agents.clone(),
        core.storage.background_agents.clone(),
        core.storage.triggers.clone(),
        core.storage.terminal_sessions.clone(),
        core.storage.deliverables.clone(),
        None,
        None,
        None,
    )
}

pub(super) fn get_runtime_tool_registry<'a>(
    core: &Arc<AppCore>,
    runtime_tool_registry: &'a OnceLock<restflow_ai::tools::ToolRegistry>,
) -> Result<&'a restflow_ai::tools::ToolRegistry, String> {
    if let Some(registry) = runtime_tool_registry.get() {
        return Ok(registry);
    }

    let registry = create_runtime_tool_registry(core).map_err(|error| error.to_string())?;
    let _ = runtime_tool_registry.set(registry);
    runtime_tool_registry
        .get()
        .ok_or_else(|| "runtime tool registry initialization failed".to_string())
}

pub(super) fn subagent_config_from_defaults(defaults: &AgentDefaults) -> SubagentConfig {
    SubagentConfig {
        max_parallel_agents: defaults.max_parallel_subagents,
        subagent_timeout_secs: defaults.subagent_timeout_secs,
        max_iterations: defaults.max_iterations,
        max_depth: defaults.max_depth,
    }
}

pub(super) fn load_agent_defaults_from_core(core: &Arc<AppCore>) -> AgentDefaults {
    match core.storage.config.get_effective_config() {
        Ok(config) => config.agent,
        Err(error) => {
            warn!(
                error = %error,
                "Failed to load system config for chat runtime; falling back to default agent config"
            );
            AgentDefaults::default()
        }
    }
}

pub(super) fn load_chat_max_session_history_from_core(core: &Arc<AppCore>) -> usize {
    match core.storage.config.get_effective_config() {
        Ok(config) => config.runtime_defaults.chat_max_session_history,
        Err(error) => {
            warn!(
                error = %error,
                "Failed to load runtime config for chat history; falling back to default history size"
            );
            DEFAULT_CHAT_MAX_SESSION_HISTORY
        }
    }
}

pub(super) fn create_chat_executor(
    core: &Arc<AppCore>,
    auth_manager: Arc<AuthProfileManager>,
) -> AgentRuntimeExecutor {
    let agent_defaults = load_agent_defaults_from_core(core);
    let (completion_tx, completion_rx) = mpsc::channel(128);
    let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
    let subagent_definitions = Arc::new(StorageBackedSubagentLookup::new(
        core.storage.agents.clone(),
    ));
    let subagent_config = subagent_config_from_defaults(&agent_defaults);
    let process_registry =
        Arc::new(ProcessRegistry::new().with_ttl_seconds(agent_defaults.process_session_ttl_secs));

    AgentRuntimeExecutor::new(
        core.storage.clone(),
        process_registry,
        auth_manager,
        subagent_tracker,
        subagent_definitions,
        subagent_config,
    )
}

pub(super) async fn cancel_chat_stream(stream_id: &str) -> bool {
    if let Some(handle) = active_chat_streams().lock().await.remove(stream_id) {
        handle.abort();
        active_chat_stream_steers().lock().await.remove(stream_id);
        let mut session_streams = active_chat_stream_sessions().lock().await;
        if let Some((session_id, _)) = session_streams
            .iter()
            .find(|(_, active_stream_id)| active_stream_id.as_str() == stream_id)
            .map(|(session_id, active_stream_id)| (session_id.clone(), active_stream_id.clone()))
        {
            session_streams.remove(&session_id);
        }
        true
    } else {
        false
    }
}

pub(super) async fn steer_chat_stream(session_id: &str, instruction: &str) -> bool {
    let stream_id = {
        let session_streams = active_chat_stream_sessions().lock().await;
        session_streams.get(session_id).cloned()
    };

    let Some(stream_id) = stream_id else {
        return false;
    };

    let sender = {
        let steers = active_chat_stream_steers().lock().await;
        steers.get(&stream_id).cloned()
    };
    let Some(sender) = sender else {
        return false;
    };

    let steer = SteerMessage::message(instruction.to_string(), SteerSource::User);
    match sender.send(steer).await {
        Ok(()) => true,
        Err(_) => {
            active_chat_stream_steers().lock().await.remove(&stream_id);
            let mut session_streams = active_chat_stream_sessions().lock().await;
            if session_streams.get(session_id) == Some(&stream_id) {
                session_streams.remove(session_id);
            }
            false
        }
    }
}

pub(super) fn latest_assistant_payload(session: &ChatSession) -> Option<(String, Option<u32>)> {
    session
        .messages
        .iter()
        .rev()
        .find(|message| message.role == ChatRole::Assistant)
        .map(|message| {
            (
                message.content.clone(),
                message.execution.as_ref().map(|exec| exec.tokens_used),
            )
        })
}

pub(super) async fn execute_chat_session(
    core: &Arc<AppCore>,
    session_id: String,
    user_input: Option<String>,
    turn_id: String,
    ack_frame_tx: Option<mpsc::UnboundedSender<StreamFrame>>,
    emitter: Option<Box<dyn StreamEmitter>>,
    steer_rx: Option<mpsc::Receiver<SteerMessage>>,
) -> Result<ChatSession> {
    let mut session = core
        .storage
        .chat_sessions
        .get(&session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

    let explicit_user_input = user_input.as_deref();
    let input = match explicit_user_input {
        Some(input) if !input.trim().is_empty() => input.to_string(),
        _ => session
            .messages
            .iter()
            .rev()
            .find(|msg| msg.role == ChatRole::User)
            .map(|msg| msg.content.clone())
            .ok_or_else(|| anyhow::anyhow!("No user message found in session"))?,
    };

    persist_ipc_user_message_if_needed(core, &mut session, explicit_user_input, &input)?;

    let reply_buffer = Arc::new(Mutex::new(VecDeque::<String>::new()));
    let auth_manager = Arc::new(build_auth_manager(core).await?);
    let reply_sender = Arc::new(SessionReplySender::new(
        reply_buffer.clone(),
        ack_frame_tx.clone(),
    ));
    let executor = create_chat_executor(core, auth_manager).with_reply_sender(reply_sender);
    let chat_max_session_history = load_chat_max_session_history_from_core(core);

    match executor
        .generate_session_acknowledgement(
            &mut session,
            &input,
            SessionInputMode::PersistedInSession,
        )
        .await
    {
        Ok(Some(ack_content)) => {
            session.add_message(ChatMessage::assistant(&ack_content));
            match core.storage.chat_sessions.update(&session) {
                Ok(()) => {
                    publish_session_event(ChatSessionEvent::MessageAdded {
                        session_id: session.id.clone(),
                        source: "ipc".to_string(),
                    });
                    if let Some(tx) = ack_frame_tx.as_ref() {
                        let _ = tx.send(StreamFrame::Ack {
                            content: ack_content,
                        });
                    }
                }
                Err(err) => {
                    warn!(
                        session_id = %session.id,
                        error = %err,
                        "Failed to persist acknowledgement message"
                    );
                }
            }
        }
        Ok(None) => {}
        Err(err) => {
            warn!(
                session_id = %session.id,
                error = %err,
                "Failed to generate acknowledgement message"
            );
        }
    }

    let orchestrator = AgentOrchestratorImpl::from_runtime_executor(executor);
    let traced_execution = orchestrator
        .run_traced_interactive_session_turn(InteractiveSessionRequest {
            session: &mut session,
            user_input: &input,
            max_history: chat_max_session_history,
            input_mode: SessionInputMode::PersistedInSession,
            run_id: turn_id,
            tool_trace_storage: core.storage.tool_traces.clone(),
            execution_trace_storage: core.storage.execution_traces.clone(),
            timeout_secs: None,
            emitter,
            steer_rx,
        })
        .await
        .map_err(anyhow::Error::new)?;
    let trace = traced_execution.trace;
    let duration_ms = traced_execution.duration_ms;
    let exec_result = traced_execution.execution;

    let (execution, persisted_input) = build_turn_persistence_payload(
        &core.storage.tool_traces,
        &session.id,
        &trace.turn_id,
        &input,
        duration_ms,
        exec_result.iterations,
    );

    if persisted_input != input {
        replace_latest_user_message_content(&mut session, &input, &persisted_input);
    }
    let buffered_replies = {
        let mut guard = reply_buffer.lock().await;
        std::mem::take(&mut *guard)
    };
    for reply in buffered_replies {
        session.add_message(ChatMessage::assistant(&reply));
    }
    let assistant_message = ChatMessage::assistant(&exec_result.output).with_execution(execution);
    session.add_message(assistant_message);
    if let Some(normalized_model) = AIModel::normalize_model_id(&exec_result.active_model) {
        // Only update last_model metadata; preserve the user's chosen session model
        // so that switch_model calls during execution don't permanently override it.
        session.metadata.last_model = Some(normalized_model);
    }
    SessionService::from_storage(&core.storage).save_existing_session(&session, "ipc")?;
    Ok(session)
}

pub(super) fn persist_ipc_user_message_if_needed(
    core: &Arc<AppCore>,
    session: &mut ChatSession,
    explicit_user_input: Option<&str>,
    effective_input: &str,
) -> Result<()> {
    let Some(raw_input) = explicit_user_input.map(str::trim) else {
        return Ok(());
    };
    if raw_input.is_empty() {
        return Ok(());
    }

    let already_persisted = session
        .messages
        .last()
        .map(|message| message.role == ChatRole::User && message.content == effective_input)
        .unwrap_or(false);
    if already_persisted {
        return Ok(());
    }

    session.add_message(ChatMessage::user(effective_input));
    if session.name == "New Chat" && session.messages.len() == 1 {
        session.auto_name_from_first_message();
    }
    core.storage.chat_sessions.update(session)?;
    publish_session_event(ChatSessionEvent::MessageAdded {
        session_id: session.id.clone(),
        source: "ipc".to_string(),
    });
    Ok(())
}

pub(super) fn resolve_agent_id(core: &Arc<AppCore>, agent_id: Option<String>) -> Result<String> {
    if let Some(agent_id) = agent_id {
        return core.storage.agents.resolve_existing_agent_id(&agent_id);
    }

    let agents = core.storage.agents.list_agents()?;
    let agent = agents
        .first()
        .ok_or_else(|| anyhow::anyhow!("No agents available"))?;
    Ok(agent.id.clone())
}

pub(super) async fn build_auth_manager(core: &Arc<AppCore>) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig {
        auto_discover: false,
        ..AuthManagerConfig::default()
    };
    let db = core.storage.get_db();
    let secrets = Arc::new(core.storage.secrets.clone());
    let profile_storage = AuthProfileStorage::new(db)?;
    let manager = AuthProfileManager::with_storage(config, secrets, Some(profile_storage));
    manager.initialize().await?;
    Ok(manager)
}

pub(super) fn parse_background_agent_status(status: &str) -> Result<BackgroundAgentStatus> {
    match status.to_lowercase().as_str() {
        "active" => Ok(BackgroundAgentStatus::Active),
        "paused" => Ok(BackgroundAgentStatus::Paused),
        "running" => Ok(BackgroundAgentStatus::Running),
        "completed" => Ok(BackgroundAgentStatus::Completed),
        "failed" => Ok(BackgroundAgentStatus::Failed),
        "interrupted" => Ok(BackgroundAgentStatus::Interrupted),
        _ => Err(anyhow::anyhow!(
            "Unknown background agent status: {}",
            status
        )),
    }
}

pub(super) fn sample_hook_context(event: &HookEvent) -> HookContext {
    let now = chrono::Utc::now().timestamp_millis();
    match event {
        HookEvent::TaskFailed | HookEvent::TaskInterrupted => HookContext {
            event: event.clone(),
            task_id: "hook-test-task".to_string(),
            task_name: "hook test task".to_string(),
            agent_id: "hook-test-agent".to_string(),
            success: Some(false),
            output: None,
            error: Some("Hook test error".to_string()),
            duration_ms: Some(321),
            timestamp: now,
        },
        _ => HookContext {
            event: event.clone(),
            task_id: "hook-test-task".to_string(),
            task_name: "hook test task".to_string(),
            agent_id: "hook-test-agent".to_string(),
            success: Some(true),
            output: Some("Hook test output".to_string()),
            error: None,
            duration_ms: Some(321),
            timestamp: now,
        },
    }
}

pub(super) fn build_agent_system_prompt(
    core: &Arc<AppCore>,
    agent_node: AgentNode,
) -> Result<String> {
    crate::runtime::agent::build_agent_system_prompt(core.storage.clone(), &agent_node, None)
}
