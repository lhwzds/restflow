use super::*;
use crate::services::operation_assessment::OperationAssessorAdapter;
use crate::storage::AuthProfileStorage;
use ai::StreamDisplayMode;
use thiserror::Error;

#[derive(Debug, Error)]
pub(super) enum ExecuteChatSessionError {
    #[error("Session not found")]
    SessionNotFound,
    #[error("No user message found in session")]
    MissingUserMessage,
    #[error("Voice transcription failed: {0}")]
    VoicePreprocessFailed(String),
    #[error("Interactive execution completed without assistant output")]
    EmptyAssistantOutput,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ExecuteChatSessionError {
    pub(super) fn status_code(&self) -> i32 {
        match self {
            Self::SessionNotFound => 404,
            Self::MissingUserMessage => 400,
            Self::VoicePreprocessFailed(_) => 400,
            Self::EmptyAssistantOutput => 500,
            Self::Internal(_) => 500,
        }
    }
}

pub(super) struct ExecuteChatSessionRequest {
    pub session_id: String,
    pub user_input: Option<String>,
    pub turn_id: String,
    pub workspace_root: Option<String>,
    pub ack_frame_tx: Option<mpsc::UnboundedSender<StreamFrame>>,
    pub emitter: Option<Box<dyn StreamEmitter>>,
    pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
}

pub(super) fn create_runtime_tool_registry_with_assessment(
    core: &Arc<AppCore>,
) -> anyhow::Result<ai::tools::ToolRegistry> {
    crate::services::tool_registry::create_tool_registry_with_assessor(
        core.storage.config.clone(),
        None,
        None,
        Some(Arc::new(OperationAssessorAdapter::new(core.clone()))),
    )
}

pub(super) fn get_runtime_tool_registry<'a>(
    core: &Arc<AppCore>,
    runtime_tool_registry: &'a OnceLock<ai::tools::ToolRegistry>,
) -> Result<&'a ai::tools::ToolRegistry, String> {
    if let Some(registry) = runtime_tool_registry.get() {
        return Ok(registry);
    }

    let registry =
        create_runtime_tool_registry_with_assessment(core).map_err(|error| error.to_string())?;
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

pub(super) async fn cancel_chat_stream(core: &Arc<AppCore>, stream_id: &str) -> bool {
    if let Some(handle) = active_chat_streams().lock().await.remove(stream_id) {
        handle.abort();
        let _ = handle.await;
        active_chat_stream_steers().lock().await.remove(stream_id);
        let mut session_streams = active_chat_stream_sessions().lock().await;
        if let Some((session_id, _)) = session_streams
            .iter()
            .find(|(_, binding)| binding.stream_id == stream_id)
            .map(|(session_id, binding)| (session_id.clone(), binding.stream_id.clone()))
        {
            session_streams.remove(&session_id);
            if let Err(error) = cancel_turn_in_session_store(core, &session_id, stream_id) {
                warn!(
                    session_id = %session_id,
                    turn_id = %stream_id,
                    error = %error,
                    "Failed to persist canceled chat turn"
                );
            }
        }
        true
    } else {
        false
    }
}

pub(super) async fn steer_chat_stream(
    core: &Arc<AppCore>,
    session_id: &str,
    instruction: &str,
    scope: Option<&types::ExecutionScope>,
) -> bool {
    let binding = {
        let session_streams = active_chat_stream_sessions().lock().await;
        session_streams.get(session_id).and_then(|binding| {
            if scope.is_some() && binding.scope.as_ref() != scope {
                None
            } else {
                Some(binding.clone())
            }
        })
    };

    let Some(binding) = binding else {
        return false;
    };

    let sender = {
        let steers = active_chat_stream_steers().lock().await;
        steers.get(&binding.stream_id).cloned()
    };
    let Some(sender) = sender else {
        return false;
    };

    let steer = SteerMessage::message(instruction.to_string(), SteerSource::User);
    match sender.send(steer).await {
        Ok(()) => persist_steer_user_update(core, session_id, &binding.turn_id, instruction)
            .map(|_| true)
            .unwrap_or(false),
        Err(_) => {
            active_chat_stream_steers()
                .lock()
                .await
                .remove(&binding.stream_id);
            let mut session_streams = active_chat_stream_sessions().lock().await;
            if session_streams
                .get(session_id)
                .is_some_and(|active| active.stream_id == binding.stream_id)
            {
                session_streams.remove(session_id);
            }
            false
        }
    }
}

fn persist_steer_user_update(
    core: &Arc<AppCore>,
    session_id: &str,
    turn_id: &str,
    instruction: &str,
) -> Result<()> {
    let instruction = instruction.trim();
    if instruction.is_empty() {
        return Ok(());
    }
    let session_service = SessionService::from_storage(&core.storage);
    let Some(mut session) = session_service.get_session_view(session_id)? else {
        return Ok(());
    };
    let already_latest = session
        .messages
        .last()
        .is_some_and(|message| message.role == ChatRole::User && message.content == instruction);
    if !already_latest {
        session.add_message(ChatMessage::user(instruction));
    }
    session.record_turn_user_message(turn_id, instruction);
    session_service.save_existing_session(&session, "ipc")?;
    Ok(())
}

pub(super) fn latest_assistant_payload(session: &ChatSession) -> Option<(String, Option<u32>)> {
    session
        .messages
        .iter()
        .rev()
        .find(|message| message.role == ChatRole::Assistant && !message.content.trim().is_empty())
        .map(|message| {
            (
                message.content.trim().to_string(),
                message.execution.as_ref().map(|exec| exec.tokens_used),
            )
        })
}

fn latest_turn_assistant_output(session: &ChatSession, turn_start_index: usize) -> Option<String> {
    session
        .messages
        .iter()
        .skip(turn_start_index)
        .rev()
        .find(|message| message.role == ChatRole::Assistant && !message.content.trim().is_empty())
        .map(|message| message.content.trim().to_string())
}

fn select_final_assistant_output(
    execution_output: &str,
    buffered_replies: &[String],
    session: &ChatSession,
    turn_start_index: usize,
) -> Option<String> {
    let trimmed = execution_output.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }

    if let Some(content) = buffered_replies
        .iter()
        .rev()
        .map(|reply| reply.trim())
        .find(|reply| !reply.is_empty())
        .map(ToOwned::to_owned)
    {
        return Some(content);
    }

    latest_turn_assistant_output(session, turn_start_index)
}

fn latest_turn_assistant_matches(
    session: &ChatSession,
    turn_start_index: usize,
    assistant_output: &str,
) -> bool {
    let trimmed = assistant_output.trim();
    !trimmed.is_empty()
        && latest_turn_assistant_output(session, turn_start_index).as_deref() == Some(trimmed)
}

pub(super) async fn execute_chat_session(
    core: &Arc<AppCore>,
    request: ExecuteChatSessionRequest,
) -> std::result::Result<ChatSession, ExecuteChatSessionError> {
    let ExecuteChatSessionRequest {
        session_id,
        user_input,
        turn_id,
        workspace_root,
        ack_frame_tx,
        emitter,
        steer_rx,
    } = request;
    let mut session = load_chat_session_for_execution(core, &session_id)?;

    let explicit_user_input = user_input.as_deref();
    let input = match explicit_user_input {
        Some(input) if !input.trim().is_empty() => input.to_string(),
        _ => session
            .messages
            .iter()
            .rev()
            .find(|msg| msg.role == ChatRole::User)
            .map(|msg| msg.content.clone())
            .ok_or(ExecuteChatSessionError::MissingUserMessage)?,
    };
    let mut persisted_input = input.clone();
    let mut agent_input = input.clone();
    if let Some(descriptor) = detect_voice_message(&input, None, None) {
        let normalized_input = descriptor.persisted_content(None);
        match preprocess_voice_message(&core.storage, &descriptor).await {
            Ok(result) => {
                persisted_input = result.persisted_input;
                agent_input = result.agent_input;
            }
            Err(error) => {
                if explicit_user_input.is_some() {
                    persist_ipc_user_message_if_needed(
                        core,
                        &mut session,
                        explicit_user_input,
                        &normalized_input,
                    )?;
                } else if replace_latest_user_message_content(
                    &mut session,
                    &input,
                    &normalized_input,
                ) {
                    SessionService::from_storage(&core.storage)
                        .save_existing_session(&session, "ipc")?;
                }
                return Err(ExecuteChatSessionError::VoicePreprocessFailed(
                    error.to_string(),
                ));
            }
        }
    }

    if explicit_user_input.is_some() {
        persist_ipc_user_message_if_needed(
            core,
            &mut session,
            explicit_user_input,
            &persisted_input,
        )?;
    } else if replace_latest_user_message_content(&mut session, &input, &persisted_input) {
        SessionService::from_storage(&core.storage).save_existing_session(&session, "ipc")?;
    }
    record_turn_user_message_in_session_store(core, &mut session, &turn_id, &persisted_input)?;

    let turn_start_index = session.messages.len();
    let reply_buffer = Arc::new(Mutex::new(VecDeque::<String>::new()));
    let auth_manager = Arc::new(build_auth_manager(core).await?);
    let reply_sender = Arc::new(SessionReplySender::new(
        reply_buffer.clone(),
        ack_frame_tx.clone(),
    ));
    let executor = create_chat_executor(core, auth_manager).with_reply_sender(reply_sender);
    let chat_max_session_history = load_chat_max_session_history_from_core(core);

    let orchestrator = AgentOrchestratorImpl::from_runtime_executor(executor);
    let traced_execution = match orchestrator
        .run_traced_interactive_session_turn(InteractiveSessionRequest {
            session: &mut session,
            user_input: &agent_input,
            max_history: chat_max_session_history,
            input_mode: SessionInputMode::PersistedInSession,
            run_id: turn_id.clone(),
            timeout_secs: None,
            emitter,
            steer_rx,
            stream_display_mode: StreamDisplayMode::Streaming,
            workspace_root: workspace_root.map(std::path::PathBuf::from),
        })
        .await
    {
        Ok(execution) => execution,
        Err(error) => {
            let message = error.to_string();
            fail_turn_in_session_store(core, &session_id, &turn_id, &message)?;
            return Err(anyhow::Error::new(error).into());
        }
    };
    let duration_ms = traced_execution.duration_ms;
    let exec_result = traced_execution.execution;

    let original_persisted_input = persisted_input.clone();
    let (execution, final_persisted_input) = build_turn_persistence_payload(
        &original_persisted_input,
        duration_ms,
        exec_result.iterations,
    );

    if final_persisted_input != original_persisted_input {
        replace_latest_user_message_content(
            &mut session,
            &original_persisted_input,
            &final_persisted_input,
        );
    }
    let buffered_replies = {
        let mut guard = reply_buffer.lock().await;
        std::mem::take(&mut *guard)
    };
    let buffered_replies = buffered_replies
        .into_iter()
        .filter(|reply| !reply.trim().is_empty())
        .collect::<Vec<_>>();
    sync_session_view_from_session_store(core, &mut session)?;
    for reply in &buffered_replies {
        session.add_message(ChatMessage::assistant(reply.as_str()));
    }
    let assistant_output = select_final_assistant_output(
        &exec_result.output,
        &buffered_replies,
        &session,
        turn_start_index,
    )
    .ok_or_else(|| {
        let _ = fail_turn_in_session_store(
            core,
            &session_id,
            &turn_id,
            "Interactive execution completed without assistant output",
        );
        ExecuteChatSessionError::EmptyAssistantOutput
    })?;
    sync_turns_from_session_store(core, &mut session)?;
    session.complete_turn_with_assistant_message(&turn_id, &assistant_output);
    if latest_turn_assistant_matches(&session, turn_start_index, &assistant_output) {
        if let Some(message) = session.messages.last_mut() {
            message.execution = Some(execution);
        }
        if let Some(model) = Some(exec_result.final_model) {
            session.set_model_identity(model);
        } else {
            session.set_model_identity_from_raw(&exec_result.active_model);
        }
        SessionService::from_storage(&core.storage).save_existing_session(&session, "ipc")?;
    } else {
        SessionService::from_storage(&core.storage).persist_interactive_turn(
            &mut session,
            PersistInteractiveTurnRequest {
                original_input: &original_persisted_input,
                persisted_input: &final_persisted_input,
                assistant_output: &assistant_output,
                active_model: Some(&exec_result.active_model),
                final_model: Some(exec_result.final_model),
                execution,
                source: "ipc",
            },
        )?;
    }
    Ok(session)
}

pub(super) fn record_turn_event_in_session_store(
    core: &Arc<AppCore>,
    session_id: &str,
    turn_id: &str,
    event: ChatTurnEventKind,
) -> Result<()> {
    let session_service = SessionService::from_storage(&core.storage);
    let Some(mut session) = session_service.get_session_view(session_id)? else {
        return Ok(());
    };
    session.record_turn_event(turn_id, event);
    session_service.save_existing_session(&session, "ipc")?;
    Ok(())
}

fn record_turn_user_message_in_session_store(
    core: &Arc<AppCore>,
    session: &mut ChatSession,
    turn_id: &str,
    content: &str,
) -> Result<()> {
    sync_turns_from_session_store(core, session)?;
    session.record_turn_user_message(turn_id, content);
    SessionService::from_storage(&core.storage).save_existing_session(session, "ipc")?;
    Ok(())
}

fn sync_turns_from_session_store(core: &Arc<AppCore>, session: &mut ChatSession) -> Result<()> {
    if let Some(stored) =
        SessionService::from_storage(&core.storage).get_session_view(&session.id)?
    {
        session.turns = stored.turns;
    }
    Ok(())
}

fn sync_session_view_from_session_store(
    core: &Arc<AppCore>,
    session: &mut ChatSession,
) -> Result<()> {
    if let Some(stored) =
        SessionService::from_storage(&core.storage).get_session_view(&session.id)?
    {
        session.messages = stored.messages;
        session.turns = stored.turns;
        session.updated_at = stored.updated_at;
        session.metadata = stored.metadata;
    }
    Ok(())
}

fn fail_turn_in_session_store(
    core: &Arc<AppCore>,
    session_id: &str,
    turn_id: &str,
    message: &str,
) -> Result<()> {
    let session_service = SessionService::from_storage(&core.storage);
    let Some(mut session) = session_service.get_session_view(session_id)? else {
        return Ok(());
    };
    session.fail_turn(turn_id, message);
    session_service.save_existing_session(&session, "ipc")?;
    Ok(())
}

fn cancel_turn_in_session_store(
    core: &Arc<AppCore>,
    session_id: &str,
    turn_id: &str,
) -> Result<()> {
    let session_service = SessionService::from_storage(&core.storage);
    let Some(mut session) = session_service.get_session_view(session_id)? else {
        return Ok(());
    };
    session.cancel_turn(turn_id);
    session_service.save_existing_session(&session, "ipc")?;
    Ok(())
}

fn load_chat_session_for_execution(
    core: &Arc<AppCore>,
    session_id: &str,
) -> std::result::Result<ChatSession, ExecuteChatSessionError> {
    let Some(session) =
        SessionService::from_storage(&core.storage).materialize_session_for_runtime(session_id)?
    else {
        return Err(ExecuteChatSessionError::SessionNotFound);
    };
    Ok(session)
}

pub(super) fn persist_ipc_user_message_if_needed(
    core: &Arc<AppCore>,
    session: &mut ChatSession,
    explicit_user_input: Option<&str>,
    persisted_input: &str,
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
        .map(|message| message.role == ChatRole::User && message.content == persisted_input)
        .unwrap_or(false);
    if already_persisted {
        return Ok(());
    }

    let mut message = ChatMessage::user(persisted_input);
    hydrate_voice_message_metadata(&mut message);
    session.add_message(message);
    if session.name == "New Chat" && session.messages.len() == 1 {
        session.auto_name_from_first_message();
    }
    SessionService::from_storage(&core.storage).save_existing_session(session, "ipc")?;
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

pub(crate) async fn build_auth_manager(core: &Arc<AppCore>) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig::default();
    let secrets = Arc::new(core.storage.secrets.clone());
    let profile_storage = AuthProfileStorage::new_namespace(core.storage.namespace())?;
    let manager = AuthProfileManager::with_storage(config, secrets, Some(profile_storage));
    manager.initialize().await?;
    Ok(manager)
}

pub(super) fn parse_task_status(status: &str) -> Result<TaskStatus> {
    match status.to_lowercase().as_str() {
        "active" => Ok(TaskStatus::Active),
        "paused" => Ok(TaskStatus::Paused),
        "running" => Ok(TaskStatus::Running),
        "completed" => Ok(TaskStatus::Completed),
        "failed" => Ok(TaskStatus::Failed),
        "interrupted" => Ok(TaskStatus::Interrupted),
        _ => Err(anyhow::anyhow!("Unknown task status: {}", status)),
    }
}

pub(super) fn build_agent_system_prompt(
    core: &Arc<AppCore>,
    agent_node: AgentNode,
) -> Result<String> {
    crate::runtime::agent::build_agent_system_prompt(core.storage.clone(), &agent_node, None)
}

#[cfg(test)]
mod tests {
    use super::{
        latest_assistant_payload, latest_turn_assistant_matches, select_final_assistant_output,
    };
    use crate::models::{ChatMessage, ChatSession};

    #[test]
    fn final_output_prefers_non_empty_execution_output() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::assistant("buffered reply"));

        let output =
            select_final_assistant_output("final answer", &[], &session, session.messages.len());

        assert_eq!(output.as_deref(), Some("final answer"));
    }

    #[test]
    fn final_output_uses_latest_non_empty_buffered_reply_when_execution_output_is_blank() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::assistant("older reply"));

        let output = select_final_assistant_output(
            "   ",
            &["".to_string(), "ack reply".to_string()],
            &session,
            session.messages.len(),
        );

        assert_eq!(output.as_deref(), Some("ack reply"));
    }

    #[test]
    fn final_output_uses_current_turn_assistant_when_execution_output_is_blank() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::assistant("previous turn"));
        let turn_start_index = session.messages.len();
        session.add_message(ChatMessage::assistant("current turn"));

        let output = select_final_assistant_output("", &[], &session, turn_start_index);

        assert_eq!(output.as_deref(), Some("current turn"));
    }

    #[test]
    fn final_output_is_missing_when_no_non_empty_assistant_text_exists() {
        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());

        let output = select_final_assistant_output("", &[], &session, 0);

        assert!(output.is_none());
    }

    #[test]
    fn latest_turn_assistant_match_requires_matching_current_turn_assistant_message() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::assistant("previous turn"));
        let turn_start_index = session.messages.len();
        session.add_message(ChatMessage::assistant("ack reply"));

        assert!(latest_turn_assistant_matches(
            &session,
            turn_start_index,
            "ack reply"
        ));
        assert!(!latest_turn_assistant_matches(
            &session,
            turn_start_index,
            "something else"
        ));
        assert!(!latest_turn_assistant_matches(
            &session,
            turn_start_index,
            "previous turn"
        ));
    }

    #[test]
    fn latest_assistant_payload_skips_empty_assistant_messages() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::assistant("visible"));
        session.add_message(ChatMessage::assistant("   "));

        let payload = latest_assistant_payload(&session);

        assert_eq!(
            payload.as_ref().map(|(content, _)| content.as_str()),
            Some("visible")
        );
    }
}
