use super::*;
use crate::services::adapters::SkrunSkillProvider;
use crate::services::skill_mentions::parse_skill_mentions;
use ::agent::StreamDisplayMode;
use types::skill::{SkillInfo, SkillProvider};

fn should_force_non_stream(model: ModelId) -> bool {
    model.is_cli_model()
}

fn interactive_turn_failover_config(primary: ModelId) -> FailoverConfig {
    // Interactive turns may execute side-effecting tools. Retrying the whole
    // ReAct turn on a fallback model can replay already-run tool calls.
    FailoverConfig::with_fallbacks(primary, Vec::new())
}

#[derive(Default)]
pub struct SessionTurnRuntimeOptions {
    pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    pub stream_display_mode: StreamDisplayMode,
    pub workspace_root: Option<std::path::PathBuf>,
}

impl AgentRuntimeExecutor {
    pub(crate) fn resolve_stored_agent_for_session(
        &self,
        session: &mut ChatSession,
    ) -> Result<crate::StoredAgent> {
        if let Some(agent) = self.storage.agents.get_agent(session.agent_id.clone())? {
            return Ok(agent);
        }

        let fallback = self.storage.agents.resolve_default_agent()?;

        let fallback_model = fallback
            .agent
            .resolved_model_ref()
            .map(|model_ref| model_ref.model.as_serialized_str().to_string())
            .unwrap_or_else(|| ModelId::Gpt5_4.as_serialized_str().to_string());
        session.agent_id = fallback.id.clone();
        session.set_model_identity_from_raw(&fallback_model);

        Ok(fallback)
    }

    fn chat_message_to_llm_message(message: &ChatMessage) -> Message {
        match message.role {
            ChatRole::User => Message::user(message.content.clone()),
            ChatRole::Assistant => Message::assistant(message.content.clone()),
            ChatRole::System => Message::system(message.content.clone()),
        }
    }

    fn resolve_mentioned_skill_infos(&self, user_input: &str) -> Vec<SkillInfo> {
        let mentioned_ids = parse_skill_mentions(user_input);
        if mentioned_ids.is_empty() {
            return Vec::new();
        }

        let provider = SkrunSkillProvider::default();
        let skills = provider.list_skills();
        mentioned_ids
            .into_iter()
            .filter_map(|id| skills.iter().find(|skill| skill.id == id).cloned())
            .collect()
    }

    fn append_mentioned_skill_directive(
        mut system_prompt: String,
        mentioned_skills: &[SkillInfo],
    ) -> String {
        if mentioned_skills.is_empty() {
            return system_prompt;
        }

        system_prompt.push_str("\n\n## User-Mentioned Skills\n");
        system_prompt.push_str(
            "The latest user message explicitly mentioned these skills. Before applying a mentioned skill, call `load_skill` with `action=read` and `id` set to the skill id.\n\n",
        );
        for skill in mentioned_skills {
            let description = skill.description.as_deref().unwrap_or("No description");
            system_prompt.push_str(&format!(
                "- {} ({}): {}\n",
                skill.name, skill.id, description
            ));
        }
        system_prompt
    }

    fn session_messages_for_context(session: &ChatSession) -> Vec<ChatMessage> {
        if session.turns.iter().any(|turn| !turn.events.is_empty()) {
            return Self::completed_turn_messages_for_context(session);
        }

        if session.messages.is_empty() {
            return Vec::new();
        }

        if let Some(summary_id) = session.summary_message_id.as_ref()
            && let Some(idx) = session.messages.iter().position(|m| &m.id == summary_id)
        {
            let mut messages = session.messages[idx..].to_vec();
            if let Some(summary) = messages.first_mut() {
                summary.role = ChatRole::User;
            }
            return messages;
        }

        session.messages.clone()
    }

    fn completed_turn_messages_for_context(session: &ChatSession) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        for turn in &session.turns {
            if turn.status != ChatTurnStatus::Completed {
                continue;
            }

            let mut user_message: Option<String> = None;
            let mut assistant_message: Option<String> = None;
            for event in &turn.events {
                match &event.kind {
                    ChatTurnEventKind::UserMessage { content }
                        if user_message.is_none() && !content.trim().is_empty() =>
                    {
                        user_message = Some(content.clone());
                    }
                    ChatTurnEventKind::AssistantMessage { content }
                        if !content.trim().is_empty() =>
                    {
                        assistant_message = Some(content.clone());
                    }
                    _ => {}
                }
            }

            if let (Some(user), Some(assistant)) = (user_message, assistant_message) {
                messages.push(ChatMessage::user(user));
                messages.push(ChatMessage::assistant(assistant));
            }
        }

        messages
    }

    fn session_history_messages(
        session: &ChatSession,
        max_messages: usize,
        input_mode: SessionInputMode,
    ) -> Vec<Message> {
        let mut messages = Self::session_messages_for_context(session);
        if messages.is_empty() {
            return Vec::new();
        }

        // Exclude the latest user input because it will be passed to execute()
        // separately for persisted-input flows.
        if input_mode == SessionInputMode::PersistedInSession
            && matches!(messages.last().map(|m| &m.role), Some(ChatRole::User))
        {
            messages.pop();
        }

        let start = messages.len().saturating_sub(max_messages);
        messages[start..]
            .iter()
            .map(Self::chat_message_to_llm_message)
            .collect()
    }

    fn session_state_for_execution(
        system_prompt: String,
        session: &ChatSession,
        max_messages: usize,
        input_mode: SessionInputMode,
        user_input: &str,
        max_iterations: usize,
    ) -> ::agent::AgentState {
        let mut state = ::agent::AgentState::new(uuid::Uuid::new_v4().to_string(), max_iterations);
        state.add_message(Message::system(system_prompt));
        for message in Self::session_history_messages(session, max_messages, input_mode) {
            state.add_message(message);
        }
        state.add_message(Message::user(user_input.to_string()));
        state
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_session_with_client(
        &self,
        agent_node: &AgentNode,
        model: ModelId,
        llm_client: Arc<dyn LlmClient>,
        session: &ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
        emitter: Option<Box<dyn StreamEmitter>>,
        factory: Arc<dyn LlmClientFactory>,
        agent_id: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        stream_display_mode: StreamDisplayMode,
        workspace_root: Option<std::path::PathBuf>,
    ) -> Result<SessionExecutionResult> {
        let swappable = Arc::new(SwappableLlm::new(llm_client));
        let mentioned_skills = self.resolve_mentioned_skill_infos(user_input);
        let effective_tools =
            self.resolve_effective_tool_names(agent_node, agent_id, Some(user_input))?;
        let agent_defaults = self
            .storage
            .config
            .get_effective_config_for_workspace(None)
            .ok()
            .map(|c| c.agent)
            .unwrap_or_default();
        let bash_config = BashConfig {
            timeout_secs: agent_defaults.bash_timeout_secs,
            ..BashConfig::default()
        };
        let reply_sender = self.resolve_reply_sender(None, agent_id);
        let tools = self.build_tool_registry(
            Some(&effective_tools),
            swappable.clone(),
            swappable.clone(),
            factory.clone(),
            agent_id,
            Some(bash_config),
            reply_sender,
            workspace_root.as_deref(),
        )?;
        let system_prompt = Self::append_mentioned_skill_directive(
            build_agent_system_prompt(self.storage.clone(), agent_node, agent_id)?,
            &mentioned_skills,
        );

        let catalog = model_resolution::ModelCatalog::global().await;
        let model_entry = catalog.resolve(model).await;
        let context_window = model_entry
            .map(|entry| {
                entry
                    .capabilities
                    .input_limit
                    .unwrap_or(entry.capabilities.context_window)
            })
            .unwrap_or_else(|| Self::context_window_for_model(model));
        let max_tool_result_length = Self::effective_max_tool_result_length(
            agent_defaults.max_tool_result_length,
            context_window,
        );
        let execution_context =
            ExecutionContext::main(agent_id.unwrap_or(&session.agent_id), &session.id);

        let mut config = ReActAgentConfig::new(user_input.to_string())
            .with_system_prompt(system_prompt.clone())
            .with_tool_timeout(Duration::from_secs(agent_defaults.tool_timeout_secs))
            .with_max_iterations(agent_defaults.max_iterations)
            .with_context_window(context_window)
            .with_resource_limits(Self::chat_resource_limits(
                agent_defaults.max_tool_calls,
                agent_defaults.max_wall_clock_secs,
            ))
            .with_max_tool_result_length(max_tool_result_length)
            .with_max_tool_concurrency(agent_defaults.max_tool_concurrency)
            .with_prune_tool_max_chars(agent_defaults.prune_tool_max_chars)
            .with_compact_preserve_tokens(agent_defaults.compact_preserve_tokens)
            .with_stream_display_mode(stream_display_mode);
        if let Some(entry) = model_entry
            && !model.is_cli_model()
        {
            config = config.with_max_output_tokens(entry.capabilities.output_limit as u32);
        }
        if model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config = config.with_temperature(temp as f32);
        }
        config = Self::apply_llm_timeout(config, agent_defaults.llm_timeout_secs);
        if agent_defaults.auto_review_tools {
            config = config
                .with_tool_call_reviewer(Arc::new(LlmToolCallReviewer::new(swappable.clone())));
        }
        config = Self::apply_execution_context(config, &execution_context);

        let mut agent = ReActAgentExecutor::new(swappable.clone(), tools)
            .with_subagent_tracker(self.subagent_tracker.clone());
        if let Some(workspace_root) = workspace_root.as_ref() {
            agent = agent.with_workspace_root(workspace_root.clone());
        }
        if let Some(rx) = steer_rx {
            agent = agent.with_steer_channel(rx);
        }
        let state = Self::session_state_for_execution(
            system_prompt,
            session,
            max_history,
            input_mode,
            user_input,
            agent_defaults.max_iterations,
        );
        let force_non_stream = should_force_non_stream(model);
        let result = if force_non_stream {
            if let Some(mut emitter) = emitter {
                agent
                    .run_from_state_with_emitter(config, state, emitter.as_mut())
                    .await?
            } else {
                agent.run_from_state(config, state).await?
            }
        } else if let Some(mut emitter) = emitter {
            agent
                .execute_from_state(config, state, emitter.as_mut())
                .await?
        } else {
            agent.run_from_state(config, state).await?
        };
        if !result.success {
            return Err(anyhow!(
                "Agent execution failed: {}",
                result.error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }

        let active_model = swappable.current_model();
        let final_model = ModelId::for_provider_and_model(model.provider(), &active_model)
            .or_else(|| ModelId::from_api_name(&active_model))
            .or_else(|| ModelId::from_canonical_id(&active_model))
            .unwrap_or(model);
        let mut execution = SessionExecutionResult::new(
            result.answer.unwrap_or_default(),
            result.iterations as u32,
            active_model,
            final_model,
        );
        execution.metrics.message_count = result.state.messages.len();
        Ok(execution)
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_session_with_model(
        &self,
        agent_node: &AgentNode,
        model: ModelId,
        session: &ChatSession,
        user_input: &str,
        primary_provider: Provider,
        max_history: usize,
        input_mode: SessionInputMode,
        emitter: Option<Box<dyn StreamEmitter>>,
        agent_id: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        stream_display_mode: StreamDisplayMode,
        workspace_root: Option<std::path::PathBuf>,
    ) -> Result<SessionExecutionResult> {
        let model_specs = ModelId::build_model_specs();
        let api_keys = self
            .build_api_keys(agent_node.api_key_config.as_ref(), primary_provider)
            .await;
        let factory = Self::build_llm_factory(api_keys, model_specs);

        let api_key = if Self::should_skip_api_key_resolution() || model.is_codex_cli() {
            None
        } else if model.is_gemini_cli() {
            self.resolve_api_key_for_model(
                model.provider(),
                agent_node.api_key_config.as_ref(),
                primary_provider,
            )
            .await
            .ok()
        } else {
            Some(
                self.resolve_api_key_for_model(
                    model.provider(),
                    agent_node.api_key_config.as_ref(),
                    primary_provider,
                )
                .await?,
            )
        };

        let llm_client =
            Self::create_llm_client(factory.as_ref(), model, api_key.as_deref(), agent_node)?;
        self.execute_session_with_client(
            agent_node,
            model,
            llm_client,
            session,
            user_input,
            max_history,
            input_mode,
            emitter,
            factory,
            agent_id,
            steer_rx,
            stream_display_mode,
            workspace_root,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_session_with_profiles(
        &self,
        agent_node: &AgentNode,
        model: ModelId,
        session: &ChatSession,
        user_input: &str,
        primary_provider: Provider,
        max_history: usize,
        input_mode: SessionInputMode,
        emitter: Option<Box<dyn StreamEmitter>>,
        agent_id: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        stream_display_mode: StreamDisplayMode,
        workspace_root: Option<std::path::PathBuf>,
    ) -> Result<SessionExecutionResult> {
        if model.is_codex_cli() || agent_node.api_key_config.is_some() {
            return self
                .execute_session_with_model(
                    agent_node,
                    model,
                    session,
                    user_input,
                    primary_provider,
                    max_history,
                    input_mode,
                    emitter,
                    agent_id,
                    steer_rx,
                    stream_display_mode,
                    workspace_root,
                )
                .await;
        }

        let profiles = self
            .auth_manager
            .get_compatible_profiles_for_model_provider(model.provider())
            .await;
        if profiles.is_empty() {
            return self
                .execute_session_with_model(
                    agent_node,
                    model,
                    session,
                    user_input,
                    primary_provider,
                    max_history,
                    input_mode,
                    emitter,
                    agent_id,
                    steer_rx,
                    stream_display_mode,
                    workspace_root,
                )
                .await;
        }

        let mut last_error: Option<anyhow::Error> = None;
        let mut emitter = emitter;
        let mut steer_rx = steer_rx;
        for profile in profiles {
            let api_key = match profile.get_api_key(self.auth_manager.resolver()) {
                Ok(key) => key,
                Err(error) => {
                    warn!(
                        profile_id = %profile.id,
                        profile_name = %profile.name,
                        model = ?model,
                        error = %error,
                        "Skipping profile because credential resolution failed"
                    );
                    continue;
                }
            };

            let model_specs = ModelId::build_model_specs();
            let api_keys = self
                .build_api_keys(agent_node.api_key_config.as_ref(), primary_provider)
                .await;
            let factory = Self::build_llm_factory(api_keys, model_specs);
            let llm_client = Self::create_llm_client(
                factory.as_ref(),
                model,
                if Self::should_skip_api_key_resolution() {
                    None
                } else {
                    Some(api_key.as_str())
                },
                agent_node,
            )?;

            match self
                .execute_session_with_client(
                    agent_node,
                    model,
                    llm_client,
                    session,
                    user_input,
                    max_history,
                    input_mode,
                    emitter.take(),
                    factory,
                    agent_id,
                    steer_rx.take(),
                    stream_display_mode,
                    workspace_root.clone(),
                )
                .await
            {
                Ok(result) => {
                    if let Err(error) = self.auth_manager.mark_success(&profile.id).await {
                        warn!(
                            profile_id = %profile.id,
                            profile_name = %profile.name,
                            model = ?model,
                            error = %error,
                            "Failed to mark profile success"
                        );
                    }
                    return Ok(result);
                }
                Err(error) => {
                    if is_credential_error(&error) {
                        if let Err(mark_error) = self.auth_manager.mark_failure(&profile.id).await {
                            warn!(
                                profile_id = %profile.id,
                                profile_name = %profile.name,
                                model = ?model,
                                error = %mark_error,
                                "Failed to mark profile failure"
                            );
                        }

                        warn!(
                            profile_id = %profile.id,
                            profile_name = %profile.name,
                            model = ?model,
                            error = %error,
                            "Profile failed with credential-related error, trying next profile"
                        );
                        last_error = Some(error);
                        continue;
                    }

                    return Err(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow!("All profiles exhausted for provider {:?}", model.provider())
        }))
    }

    /// Execute a chat turn for an existing chat session.
    ///
    /// This method keeps chat execution in daemon-side runtime logic so UI
    /// clients (HTTP/MCP/CLI) can share the same execution behavior.
    pub async fn execute_session_turn(
        &self,
        session: &mut ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
    ) -> Result<SessionExecutionResult> {
        self.execute_session_turn_with_emitter(session, user_input, max_history, input_mode, None)
            .await
    }

    /// Execute a chat turn for an existing chat session with optional stream emitter.
    pub async fn execute_session_turn_with_emitter(
        &self,
        session: &mut ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<SessionExecutionResult> {
        self.execute_session_turn_with_emitter_and_steer(
            session,
            user_input,
            max_history,
            input_mode,
            emitter,
            SessionTurnRuntimeOptions {
                steer_rx: None,
                stream_display_mode: StreamDisplayMode::Buffered,
                workspace_root: None,
            },
        )
        .await
    }

    /// Execute a chat turn for an existing chat session with optional stream emitter
    /// and optional steer channel.
    pub async fn execute_session_turn_with_emitter_and_steer(
        &self,
        session: &mut ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
        emitter: Option<Box<dyn StreamEmitter>>,
        options: SessionTurnRuntimeOptions,
    ) -> Result<SessionExecutionResult> {
        let SessionTurnRuntimeOptions {
            steer_rx,
            stream_display_mode,
            workspace_root,
        } = options;
        let stored_agent = self.resolve_stored_agent_for_session(session)?;
        let agent_node = stored_agent.agent.clone();
        // Prefer the session's model (user override) over the agent's default
        let primary_model = if !session.model.is_empty() {
            match ModelId::from_api_name(&session.model)
                .or_else(|| ModelId::from_canonical_id(&session.model))
            {
                Some(model) => model,
                None => self.resolve_primary_model(&agent_node).await?,
            }
        } else {
            self.resolve_primary_model(&agent_node).await?
        };
        let primary_provider = primary_model.provider();
        self.run_preflight_check(
            &agent_node,
            primary_model,
            primary_provider,
            Some(user_input),
        )
        .await?;
        let failover_config = interactive_turn_failover_config(primary_model);
        let failover_manager = FailoverManager::new(failover_config);
        let retry_config = RetryConfig::default();
        let mut retry_state = RetryState::new();
        let session_snapshot = session.clone();
        let agent_id = session.agent_id.clone();
        let shared_emitter = share_stream_emitter(emitter);
        let mut steer_rx = steer_rx;

        loop {
            let node = agent_node.clone();
            let session_for_execution = session_snapshot.clone();
            let result = execute_with_failover(&failover_manager, |model| {
                let node = node.clone();
                let session_for_execution = session_for_execution.clone();
                let agent_id = agent_id.clone();
                let emitter = clone_shared_emitter(&shared_emitter);
                let steer_rx = steer_rx.take();
                let workspace_root = workspace_root.clone();
                async move {
                    self.execute_session_with_profiles(
                        &node,
                        model,
                        &session_for_execution,
                        user_input,
                        primary_provider,
                        max_history,
                        input_mode,
                        emitter,
                        Some(agent_id.as_str()),
                        steer_rx,
                        stream_display_mode,
                        workspace_root.clone(),
                    )
                    .await
                }
            })
            .await;

            match result {
                Ok((mut exec_result, final_model)) => {
                    exec_result.final_model = final_model;
                    exec_result.metrics.final_model = Some(final_model);
                    return Ok(exec_result);
                }
                Err(err) => {
                    let error_msg = err.to_string();
                    if retry_state.should_retry(&retry_config, &error_msg) {
                        retry_state.record_failure(&error_msg, &retry_config);
                        let delay = retry_state.calculate_delay(&retry_config);
                        sleep(delay).await;
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::agent::StreamDisplayMode;
    use ::agent::llm::Role;
    use types::skill::{SkillInfo, SkillSource};

    #[test]
    fn should_force_non_stream_for_all_cli_models() {
        assert!(should_force_non_stream(ModelId::CodexCli));
        assert!(should_force_non_stream(ModelId::GeminiCli));
        assert!(should_force_non_stream(ModelId::OpenCodeCli));
        assert!(!should_force_non_stream(ModelId::Glm5_1CodingPlan));
        assert!(!should_force_non_stream(ModelId::Gpt5));
    }

    #[test]
    fn interactive_turn_failover_config_does_not_replay_turn_on_fallbacks() {
        let config = interactive_turn_failover_config(ModelId::DeepseekChat);

        assert_eq!(config.primary, ModelId::DeepseekChat);
        assert!(config.fallbacks.is_empty());
    }

    #[test]
    fn session_turn_runtime_options_default_to_buffered_display() {
        let options = SessionTurnRuntimeOptions::default();
        assert_eq!(options.stream_display_mode, StreamDisplayMode::Buffered);
    }

    #[test]
    fn session_history_messages_skip_canceled_turns() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::user("run stale tool"));
        session.record_turn_user_message("turn-1", "run stale tool");
        session.record_turn_event(
            "turn-1",
            ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: "{\"command\":\"sleep 15\"}".to_string(),
            },
        );
        session.cancel_turn("turn-1");
        session.add_message(ChatMessage::user("latest request"));
        session.record_turn_user_message("turn-2", "latest request");

        let history = AgentRuntimeExecutor::session_history_messages(
            &session,
            20,
            SessionInputMode::PersistedInSession,
        );

        assert!(history.is_empty());
    }

    #[test]
    fn session_state_for_execution_uses_latest_user_after_canceled_tool_turn() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::user("run stale tool"));
        session.record_turn_user_message("turn-1", "run stale tool");
        session.record_turn_event(
            "turn-1",
            ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: "{\"command\":\"sleep 20; echo stale\"}".to_string(),
            },
        );
        session.cancel_turn("turn-1");
        session.add_message(ChatMessage::user("latest request only"));
        session.record_turn_user_message("turn-2", "latest request only");

        let state = AgentRuntimeExecutor::session_state_for_execution(
            "system prompt".to_string(),
            &session,
            20,
            SessionInputMode::PersistedInSession,
            "latest request only",
            4,
        );

        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[0].role, Role::System);
        assert_eq!(state.messages[0].content, "system prompt");
        assert_eq!(state.messages[1].role, Role::User);
        assert_eq!(state.messages[1].content, "latest request only");
        assert!(
            state
                .messages
                .iter()
                .all(|message| !message.content.contains("stale"))
        );
    }

    #[test]
    fn session_history_messages_keep_completed_turns() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::user("old request"));
        session.add_message(ChatMessage::assistant("old answer"));
        session.record_turn_user_message("turn-1", "old request");
        session.complete_turn_with_assistant_message("turn-1", "old answer");
        session.add_message(ChatMessage::user("latest request"));
        session.record_turn_user_message("turn-2", "latest request");

        let history = AgentRuntimeExecutor::session_history_messages(
            &session,
            20,
            SessionInputMode::PersistedInSession,
        );

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, Role::User);
        assert_eq!(history[0].content, "old request");
        assert_eq!(history[1].role, Role::Assistant);
        assert_eq!(history[1].content, "old answer");
    }

    #[test]
    fn mentioned_skill_directive_lists_ids_without_content() {
        let prompt = AgentRuntimeExecutor::append_mentioned_skill_directive(
            "Base prompt".to_string(),
            &[SkillInfo {
                id: "team".to_string(),
                name: "Team".to_string(),
                description: Some("Coordinate subagents".to_string()),
                tags: None,
                kind: Some("markdown".to_string()),
                executable: false,
                suggested_tools: Vec::new(),
                source: SkillSource::System,
                read_only: true,
                source_ref: None,
            }],
        );

        assert!(prompt.contains("User-Mentioned Skills"));
        assert!(prompt.contains("load_skill"));
        assert!(prompt.contains("Team (team): Coordinate subagents"));
        assert!(!prompt.contains("# Team"));
    }
}
