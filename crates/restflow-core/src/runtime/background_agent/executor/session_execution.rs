use super::*;

fn should_force_non_stream(model: ModelId) -> bool {
    model.is_cli_model()
}

#[derive(Default)]
pub struct SessionTurnRuntimeOptions {
    pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    pub telemetry_context: Option<restflow_trace::TelemetryContext>,
}

impl AgentRuntimeExecutor {
    fn resolve_stored_agent_for_session(
        &self,
        session: &mut ChatSession,
    ) -> Result<crate::storage::agent::StoredAgent> {
        if let Some(agent) = self.storage.agents.get_agent(session.agent_id.clone())? {
            return Ok(agent);
        }

        let fallback = self.storage.agents.resolve_default_agent()?;

        let fallback_model = fallback
            .agent
            .model
            .map(|m| m.as_serialized_str().to_string())
            .unwrap_or_else(|| ModelId::Gpt5.as_serialized_str().to_string());
        session.agent_id = fallback.id.clone();
        session.model = fallback_model.clone();
        session.metadata.last_model = Some(fallback_model);

        Ok(fallback)
    }

    fn chat_message_to_llm_message(message: &ChatMessage) -> Message {
        match message.role {
            ChatRole::User => Message::user(message.content.clone()),
            ChatRole::Assistant => Message::assistant(message.content.clone()),
            ChatRole::System => Message::system(message.content.clone()),
        }
    }

    pub(super) fn truncate_ack_message(content: &str) -> String {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        let chars = trimmed.chars().count();
        if chars <= ACK_PHASE_MAX_CHARS {
            return trimmed.to_string();
        }

        let truncated: String = trimmed.chars().take(ACK_PHASE_MAX_CHARS).collect();
        format!("{truncated}...")
    }

    pub(super) fn build_ack_system_prompt(
        &self,
        agent_node: &AgentNode,
        agent_id: Option<&str>,
    ) -> Result<String> {
        let base_prompt = build_agent_system_prompt(self.storage.clone(), agent_node, agent_id)?;
        Ok(format!(
            "{base_prompt}\n\n## Temporary Acknowledgement Phase\n{ACK_PHASE_SYSTEM_DIRECTIVE}"
        ))
    }

    fn session_messages_for_context(session: &ChatSession) -> Vec<ChatMessage> {
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

    #[allow(clippy::too_many_arguments)]
    async fn generate_ack_with_model(
        &self,
        agent_node: &AgentNode,
        model: ModelId,
        session: &ChatSession,
        user_input: &str,
        primary_provider: Provider,
        input_mode: SessionInputMode,
        agent_id: Option<&str>,
    ) -> Result<Option<String>> {
        let model_specs = ModelId::build_model_specs();
        let api_keys = self
            .build_api_keys(agent_node.api_key_config.as_ref(), primary_provider)
            .await;
        let factory = Arc::new(DefaultLlmClientFactory::new(api_keys, model_specs));

        let api_key = if model.is_codex_cli() {
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

        let mut messages = Vec::new();
        messages.push(Message::system(
            self.build_ack_system_prompt(agent_node, agent_id)?,
        ));
        messages.extend(Self::session_history_messages(
            session,
            ACK_PHASE_MAX_HISTORY,
            input_mode,
        ));
        messages.push(Message::user(user_input.to_string()));

        let mut request = CompletionRequest::new(messages).with_max_tokens(ACK_PHASE_MAX_TOKENS);
        if model.supports_temperature() {
            request = request.with_temperature(0.2);
        }

        let response = tokio::time::timeout(
            Duration::from_secs(ACK_PHASE_TIMEOUT_SECS),
            llm_client.complete(request),
        )
        .await
        .map_err(|_| anyhow!("Acknowledgement phase timed out"))??;

        let content = Self::truncate_ack_message(response.content.unwrap_or_default().as_str());
        if content.is_empty() {
            return Ok(None);
        }
        Ok(Some(content))
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
        telemetry_context: Option<restflow_trace::TelemetryContext>,
    ) -> Result<SessionExecutionResult> {
        let swappable = Arc::new(SwappableLlm::new(llm_client));
        let effective_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
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
            None,
        )?;
        let system_prompt = build_agent_system_prompt(self.storage.clone(), agent_node, agent_id)?;

        let catalog = ModelCatalog::global().await;
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
        let telemetry_context = telemetry_context.unwrap_or_else(|| {
            restflow_trace::TelemetryContext::new(restflow_trace::RestflowTrace::new(
                session.id.clone(),
                session.id.clone(),
                session.id.clone(),
                agent_id.unwrap_or(&session.agent_id),
            ))
            .with_requested_model(model.as_serialized_str())
            .with_effective_model(model.as_serialized_str())
            .with_provider(model.provider().as_canonical_str())
        });

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
            .with_compact_preserve_tokens(agent_defaults.compact_preserve_tokens);
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
        config = Self::apply_execution_context(config, &execution_context);
        config = config
            .with_telemetry_sink(crate::telemetry::build_core_telemetry_sink(
                self.storage.as_ref(),
            ))
            .with_telemetry_context(telemetry_context.clone());

        let mut agent = ReActAgentExecutor::new(swappable.clone(), tools)
            .with_subagent_tracker(self.subagent_tracker.clone());
        if let Some(rx) = steer_rx {
            agent = agent.with_steer_channel(rx);
        }
        let history_messages = Self::session_history_messages(session, max_history, input_mode);
        let force_non_stream = should_force_non_stream(model);
        let result = if history_messages.is_empty() {
            if force_non_stream {
                if let Some(mut emitter) = emitter {
                    agent.run_with_emitter(config, emitter.as_mut()).await?
                } else {
                    agent.run(config).await?
                }
            } else if let Some(mut emitter) = emitter {
                #[allow(deprecated)]
                {
                    agent.execute_streaming(config, emitter.as_mut()).await?
                }
            } else {
                agent.run(config).await?
            }
        } else {
            let mut state = restflow_ai::AgentState::new(
                uuid::Uuid::new_v4().to_string(),
                agent_defaults.max_iterations,
            );
            state.add_message(Message::system(system_prompt));
            for message in history_messages {
                state.add_message(message);
            }
            state.add_message(Message::user(user_input.to_string()));
            if force_non_stream {
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
            }
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
        telemetry_context: Option<restflow_trace::TelemetryContext>,
    ) -> Result<SessionExecutionResult> {
        let model_specs = ModelId::build_model_specs();
        let api_keys = self
            .build_api_keys(agent_node.api_key_config.as_ref(), primary_provider)
            .await;
        let factory = Arc::new(DefaultLlmClientFactory::new(api_keys, model_specs));

        let api_key = if model.is_codex_cli() {
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
            telemetry_context,
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
        telemetry_context: Option<restflow_trace::TelemetryContext>,
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
                    telemetry_context,
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
                    telemetry_context,
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
            let factory = Arc::new(DefaultLlmClientFactory::new(api_keys, model_specs));
            let llm_client = Self::create_llm_client(
                factory.as_ref(),
                model,
                Some(api_key.as_str()),
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
                    telemetry_context.clone(),
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

    /// Generate a short assistant acknowledgement for the latest user message.
    ///
    /// This phase runs before the main tool-enabled execution to provide fast
    /// user feedback. It uses a temporary system prompt and a direct LLM
    /// completion request with strict limits.
    pub async fn generate_session_acknowledgement(
        &self,
        session: &mut ChatSession,
        user_input: &str,
        input_mode: SessionInputMode,
    ) -> Result<Option<String>> {
        let input = user_input.trim();
        if input.is_empty() {
            return Ok(None);
        }

        let stored_agent = self.resolve_stored_agent_for_session(session)?;
        let agent_node = stored_agent.agent.clone();

        // Prefer the session's model (user override) over the agent's default.
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

        self.generate_ack_with_model(
            &agent_node,
            primary_model,
            session,
            input,
            primary_model.provider(),
            input_mode,
            Some(session.agent_id.as_str()),
        )
        .await
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
        self.execute_session_turn_with_emitter(
            session,
            user_input,
            max_history,
            input_mode,
            None,
            None,
        )
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
        telemetry_context: Option<restflow_trace::TelemetryContext>,
    ) -> Result<SessionExecutionResult> {
        self.execute_session_turn_with_emitter_and_steer(
            session,
            user_input,
            max_history,
            input_mode,
            emitter,
            SessionTurnRuntimeOptions {
                steer_rx: None,
                telemetry_context,
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
            telemetry_context,
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
        let failover_config = self
            .build_failover_config(primary_model, agent_node.api_key_config.as_ref())
            .await;
        let failover_manager = FailoverManager::new(failover_config);
        let retry_config = RetryConfig::default();
        let mut retry_state = RetryState::new();
        let session_snapshot = session.clone();
        let agent_id = session.agent_id.clone();
        let shared_emitter = share_stream_emitter(emitter);
        let mut steer_rx = steer_rx;
        let telemetry_sink = crate::telemetry::build_core_telemetry_sink(self.storage.as_ref());
        let base_telemetry_context = telemetry_context.unwrap_or_else(|| {
            restflow_trace::TelemetryContext::new(restflow_trace::RestflowTrace::new(
                session.id.clone(),
                session.id.clone(),
                session.id.clone(),
                session.agent_id.clone(),
            ))
            .with_requested_model(primary_model.as_serialized_str())
            .with_effective_model(primary_model.as_serialized_str())
            .with_provider(primary_provider.as_canonical_str())
        });

        loop {
            let node = agent_node.clone();
            let session_for_execution = session_snapshot.clone();
            let mut previous_attempt_model: Option<ModelId> = None;
            let telemetry_sink = telemetry_sink.clone();
            let base_telemetry_context = base_telemetry_context.clone();
            let result = execute_with_failover(&failover_manager, |model| {
                let node = node.clone();
                let session_for_execution = session_for_execution.clone();
                let agent_id = agent_id.clone();
                let previous_model = previous_attempt_model.replace(model);
                let emitter = clone_shared_emitter(&shared_emitter);
                let steer_rx = steer_rx.take();
                let telemetry_sink = telemetry_sink.clone();
                let telemetry_context = base_telemetry_context
                    .clone()
                    .with_effective_model(model.as_serialized_str())
                    .with_attempt(previous_attempt_model.map(|_| 2).unwrap_or(1));
                async move {
                    if let Some(previous_model) = previous_model
                        && previous_model != model
                    {
                        telemetry_sink
                            .emit(
                                restflow_trace::ExecutionEventEnvelope::from_telemetry_context(
                                    &telemetry_context,
                                    restflow_trace::ExecutionEvent::ModelSwitch {
                                        from_model: previous_model.as_serialized_str().to_string(),
                                        to_model: model.as_serialized_str().to_string(),
                                        reason: Some("failover".to_string()),
                                        success: true,
                                    },
                                )
                                .with_effective_model(model.as_serialized_str().to_string()),
                            )
                            .await;
                    }
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
                        Some(telemetry_context),
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

    #[test]
    fn should_force_non_stream_for_all_cli_models() {
        assert!(should_force_non_stream(ModelId::ClaudeCodeSonnet));
        assert!(should_force_non_stream(ModelId::CodexCli));
        assert!(should_force_non_stream(ModelId::GeminiCli));
        assert!(should_force_non_stream(ModelId::OpenCodeCli));
        assert!(!should_force_non_stream(ModelId::Gpt5));
    }
}
