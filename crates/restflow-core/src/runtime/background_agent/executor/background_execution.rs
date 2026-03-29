use super::*;
use async_trait::async_trait;

fn should_force_non_stream(model: ModelId) -> bool {
    model.is_cli_model()
}

fn generated_run_id() -> String {
    format!(
        "{}-{}",
        chrono::Utc::now().timestamp_millis(),
        uuid::Uuid::new_v4()
    )
}

impl AgentRuntimeExecutor {
    fn background_telemetry_context(
        telemetry_context: Option<restflow_telemetry::TelemetryContext>,
        background_task_id: Option<&str>,
        background_task_snapshot: Option<&crate::models::BackgroundAgent>,
        agent_id: Option<&str>,
    ) -> restflow_telemetry::TelemetryContext {
        let resolved_agent_id = agent_id.unwrap_or("unknown-agent").to_string();
        if let Some(mut context) = telemetry_context {
            context.trace.actor_id = resolved_agent_id;
            return context;
        }

        let run_id = generated_run_id();
        let session_id = background_task_snapshot
            .as_ref()
            .map(|task| task.chat_session_id.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| run_id.clone());
        let scope_id = background_task_id.unwrap_or(run_id.as_str()).to_string();
        restflow_telemetry::TelemetryContext::new(restflow_telemetry::RestflowTrace::new(
            run_id,
            session_id,
            scope_id,
            resolved_agent_id,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_agent_with_client(
        &self,
        agent_node: &AgentNode,
        model: ModelId,
        llm_client: Arc<dyn LlmClient>,
        background_task_id: Option<&str>,
        input: Option<&str>,
        _memory_config: &MemoryConfig,
        resource_limits: &crate::models::ResourceLimits,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        factory: Arc<dyn LlmClientFactory>,
        agent_id: Option<&str>,
        initial_state: Option<restflow_ai::AgentState>,
        telemetry_context: restflow_telemetry::TelemetryContext,
    ) -> Result<ExecutionResult> {
        let background_task_snapshot = if let Some(task_id) = background_task_id {
            match self.storage.background_agents.get_task(task_id) {
                Ok(task) => task,
                Err(error) => {
                    warn!(
                        task_id,
                        error = %error,
                        "Failed to load background task for execution context"
                    );
                    None
                }
            }
        } else {
            None
        };
        let workspace_root = background_task_snapshot
            .as_ref()
            .and_then(|task| match &task.execution_mode {
                crate::models::ExecutionMode::Cli(config) => config.working_dir.as_deref(),
                crate::models::ExecutionMode::Api => None,
            })
            .and_then(|path| {
                let path = std::path::PathBuf::from(path);
                path.is_absolute().then_some(path)
            });

        // Load agent execution defaults from system config (runtime-configurable).
        let agent_defaults = self
            .storage
            .config
            .get_effective_config_for_workspace(workspace_root.as_deref())
            .ok()
            .map(|c| c.agent)
            .unwrap_or_default();

        let swappable = Arc::new(SwappableLlm::new(llm_client));
        let effective_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
        let bash_config = BashConfig {
            working_dir: workspace_root
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            timeout_secs: agent_defaults.bash_timeout_secs,
            ..BashConfig::default()
        };
        let reply_sender = self.resolve_reply_sender(background_task_id, agent_id);
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
        let system_prompt =
            self.build_background_system_prompt(agent_node, agent_id, background_task_id, input)?;
        let goal = input.unwrap_or("Execute the agent task");
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
            resource_limits.max_output_bytes,
            context_window,
        );
        let execution_context = background_task_id.map(|task_id| {
            let chat_session_id = background_task_snapshot
                .as_ref()
                .map(|task| task.chat_session_id.trim().to_string())
                .filter(|session_id| !session_id.is_empty())
                .unwrap_or_else(|| "unknown".to_string());
            ExecutionContext::background(
                agent_id.unwrap_or("unknown-agent"),
                chat_session_id,
                task_id,
            )
        });
        if max_tool_result_length < resource_limits.max_output_bytes {
            debug!(
                model = ?model,
                requested_max_output_bytes = resource_limits.max_output_bytes,
                context_window,
                clamped_max_tool_result_length = max_tool_result_length,
                "Clamped max tool result length based on context window"
            );
        }

        let mut config = ReActAgentConfig::new(goal.to_string())
            .with_system_prompt(system_prompt)
            .with_prompt_flags(Self::non_main_agent_prompt_flags())
            .with_tool_timeout(Duration::from_secs(agent_defaults.tool_timeout_secs))
            .with_max_iterations(agent_defaults.max_iterations)
            .with_context_window(context_window)
            .with_resource_limits(Self::to_agent_resource_limits(resource_limits))
            .with_max_tool_result_length(max_tool_result_length)
            .with_max_tool_concurrency(agent_defaults.max_tool_concurrency)
            .with_prune_tool_max_chars(agent_defaults.prune_tool_max_chars)
            .with_compact_preserve_tokens(agent_defaults.compact_preserve_tokens)
            .with_yolo_mode(background_task_id.is_some());
        if let Some(entry) = model_entry
            && !model.is_cli_model()
        {
            config = config.with_max_output_tokens(entry.capabilities.output_limit as u32);
        }
        if let Some(task_id) = background_task_id
            && let Ok(tool_output_dir) = Self::create_tool_output_dir_for_task(task_id)
        {
            config = config.with_tool_output_dir(tool_output_dir);
        }
        if model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config = config.with_temperature(temp as f32);
        }
        config = Self::apply_llm_timeout(config, agent_defaults.llm_timeout_secs);
        if let Some(model_routing) = agent_node.model_routing.as_ref() {
            config = config.with_model_routing(AiModelRoutingConfig::from(model_routing));
            if model_routing.enabled {
                let switcher: Arc<dyn LlmSwitcher> = Arc::new(RuntimeModelSwitcher {
                    swappable: swappable.clone(),
                    factory: factory.clone(),
                    agent_node: agent_node.clone(),
                });
                config = config.with_model_switcher(switcher);
            }
        }
        if let Some(context) = execution_context.as_ref() {
            config = Self::apply_execution_context(config, context);
        }
        config = config
            .with_telemetry_sink(crate::telemetry::build_core_telemetry_sink(
                self.storage.as_ref(),
            ))
            .with_telemetry_context(telemetry_context);
        if let Some(task) = background_task_snapshot.as_ref() {
            let checkpoint_durability = match task.durability_mode {
                DurabilityMode::Sync => CheckpointDurability::PerTurn,
                DurabilityMode::Async => CheckpointDurability::Periodic { interval: 5 },
                DurabilityMode::Exit => CheckpointDurability::OnComplete,
            };
            config = config.with_checkpoint_durability(checkpoint_durability);

            let checkpoints = self.storage.background_agents.clone();
            let task_id_owned = task.id.clone();
            config = config.with_checkpoint_callback(move |state| {
                let checkpoints = checkpoints.clone();
                let task_id = task_id_owned.clone();
                let state = state.clone();
                async move {
                    let state_json = serde_json::to_vec(&state)
                        .map_err(|e| AiError::Agent(format!("Failed to encode state: {e}")))?;
                    let mut checkpoint = AgentCheckpoint::new(
                        state.execution_id.clone(),
                        Some(task_id),
                        state.version,
                        state.iteration,
                        state_json,
                        "periodic_checkpoint".to_string(),
                    );
                    // Atomic checkpoint + savepoint: first save with savepoint (no savepoint_id in data),
                    // then re-save with savepoint_id embedded to close the race window.
                    let savepoint_id = checkpoints
                        .save_checkpoint_with_savepoint(&checkpoint)
                        .map_err(|e| {
                            AiError::Agent(format!("Failed to save checkpoint with savepoint: {e}"))
                        })?;
                    checkpoint.savepoint_id = Some(savepoint_id);
                    checkpoints
                        .save_checkpoint_with_savepoint_id(&checkpoint)
                        .map_err(|e| {
                            AiError::Agent(format!(
                                "Failed to persist checkpoint with savepoint id: {e}"
                            ))
                        })?;
                    Ok(())
                }
            });
        }

        let mut agent = ReActAgentExecutor::new(swappable.clone(), tools)
            .with_subagent_tracker(self.subagent_tracker.clone());
        if let Some(workspace_root) = workspace_root {
            agent = agent.with_workspace_root(workspace_root);
        }
        if let Some(rx) = steer_rx {
            agent = agent.with_steer_channel(rx);
        }

        let force_non_stream = should_force_non_stream(model);

        let result = if let Some(state) = initial_state {
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
        } else if force_non_stream {
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
        };
        if result.success {
            let message_count = result.state.messages.len();
            let messages = result.state.messages;
            let output = result.answer.unwrap_or_default();
            let active_model = swappable.current_model();
            let final_model = ModelId::for_provider_and_model(model.provider(), &active_model)
                .or_else(|| ModelId::from_api_name(&active_model))
                .or_else(|| ModelId::from_canonical_id(&active_model))
                .unwrap_or(model);
            Ok(ExecutionResult::success(output, messages).with_metrics(
                crate::runtime::background_agent::ExecutionMetrics {
                    iterations: Some(result.iterations as u32),
                    active_model: Some(active_model),
                    final_model: Some(final_model),
                    message_count,
                    ..crate::runtime::background_agent::ExecutionMetrics::default()
                },
            ))
        } else {
            Err(anyhow!(
                "Agent execution failed: {}",
                result.error.unwrap_or_else(|| "unknown error".to_string())
            ))
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_with_model(
        &self,
        agent_node: &AgentNode,
        model: ModelId,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        resource_limits: &crate::models::ResourceLimits,
        primary_provider: Provider,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        agent_id: Option<&str>,
        initial_state: Option<restflow_ai::AgentState>,
        telemetry_context: restflow_telemetry::TelemetryContext,
    ) -> Result<ExecutionResult> {
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
        self.execute_agent_with_client(
            agent_node,
            model,
            llm_client,
            background_task_id,
            input,
            memory_config,
            resource_limits,
            steer_rx,
            emitter,
            factory,
            agent_id,
            initial_state,
            telemetry_context,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_with_profiles(
        &self,
        agent_node: &AgentNode,
        model: ModelId,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        resource_limits: &crate::models::ResourceLimits,
        primary_provider: Provider,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        agent_id: Option<&str>,
        initial_state: Option<restflow_ai::AgentState>,
        telemetry_context: restflow_telemetry::TelemetryContext,
    ) -> Result<ExecutionResult> {
        if model.is_codex_cli() {
            return self
                .execute_with_model(
                    agent_node,
                    model,
                    background_task_id,
                    input,
                    memory_config,
                    resource_limits,
                    primary_provider,
                    steer_rx,
                    emitter,
                    agent_id,
                    initial_state,
                    telemetry_context,
                )
                .await;
        }

        if agent_node.api_key_config.is_some() {
            return self
                .execute_with_model(
                    agent_node,
                    model,
                    background_task_id,
                    input,
                    memory_config,
                    resource_limits,
                    primary_provider,
                    steer_rx,
                    emitter,
                    agent_id,
                    initial_state,
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
                .execute_with_model(
                    agent_node,
                    model,
                    background_task_id,
                    input,
                    memory_config,
                    resource_limits,
                    primary_provider,
                    steer_rx,
                    emitter,
                    agent_id,
                    initial_state,
                    telemetry_context,
                )
                .await;
        }

        let mut last_error: Option<anyhow::Error> = None;
        let mut steer_rx = steer_rx;
        let mut emitter = emitter;

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
                .execute_agent_with_client(
                    agent_node,
                    model,
                    llm_client,
                    background_task_id,
                    input,
                    memory_config,
                    resource_limits,
                    steer_rx.take(),
                    emitter.take(),
                    factory,
                    agent_id,
                    initial_state.clone(),
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
}

#[async_trait]
impl AgentExecutor for AgentRuntimeExecutor {
    /// Execute an agent with the given input.
    ///
    /// This method:
    /// 1. Loads the agent configuration from storage
    /// 2. Resolves the API key for the model
    /// 3. Creates the appropriate LLM client
    /// 4. Builds the system prompt (from agent config or skill)
    /// 5. Creates the tool registry
    /// 6. Executes the agent via restflow_ai::AgentExecutor
    /// 7. Returns the execution result with output and messages
    async fn execute(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<ExecutionResult> {
        self.execute_internal(
            agent_id,
            background_task_id,
            input,
            memory_config,
            steer_rx,
            None,
            None,
        )
        .await
    }

    async fn execute_with_emitter(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        self.execute_with_emitter_and_telemetry(
            agent_id,
            background_task_id,
            input,
            memory_config,
            steer_rx,
            emitter,
            None,
        )
        .await
    }

    async fn execute_with_emitter_and_telemetry(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        telemetry_context: Option<restflow_telemetry::TelemetryContext>,
    ) -> Result<ExecutionResult> {
        self.execute_internal(
            agent_id,
            background_task_id,
            input,
            memory_config,
            steer_rx,
            emitter,
            telemetry_context,
        )
        .await
    }

    async fn execute_from_state(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: restflow_ai::AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        self.execute_from_state_with_emitter_and_telemetry(
            agent_id,
            background_task_id,
            state,
            memory_config,
            steer_rx,
            emitter,
            None,
        )
        .await
    }

    async fn execute_from_state_with_emitter_and_telemetry(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: restflow_ai::AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        telemetry_context: Option<restflow_telemetry::TelemetryContext>,
    ) -> Result<ExecutionResult> {
        self.execute_internal_from_state(
            agent_id,
            background_task_id,
            state,
            memory_config,
            steer_rx,
            emitter,
            telemetry_context,
        )
        .await
    }
}

impl AgentRuntimeExecutor {
    #[allow(clippy::too_many_arguments)]
    async fn execute_internal(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        telemetry_context: Option<restflow_telemetry::TelemetryContext>,
    ) -> Result<ExecutionResult> {
        let stored_agent = self
            .storage
            .agents
            .get_agent(agent_id.to_string())?
            .ok_or_else(|| anyhow!("Agent '{}' not found", agent_id))?;
        // Fail closed on storage errors - do not silently swallow DB failures.
        let background_task = match background_task_id {
            Some(task_id) => match self.storage.background_agents.get_task(task_id) {
                Ok(task_opt) => task_opt,
                Err(e) => {
                    warn!(task_id, error = %e, "Failed to load background task");
                    return Err(e);
                }
            },
            None => None,
        };
        if let Some(task) = background_task.as_ref() {
            self.validate_prerequisites(&task.prerequisites)?;
        }
        let resolved_resource_limits = background_task
            .as_ref()
            .map(|task| task.resource_limits.clone())
            .unwrap_or_default();

        let agent_node = stored_agent.agent.clone();
        let primary_model = self.resolve_primary_model(&agent_node).await?;
        let primary_provider = primary_model.provider();
        self.run_preflight_check(&agent_node, primary_model, primary_provider, input)
            .await?;

        let failover_config = self
            .build_failover_config(primary_model, agent_node.api_key_config.as_ref())
            .await;
        let failover_manager = FailoverManager::new(failover_config);
        let retry_config = RetryConfig::default();
        let mut retry_state = RetryState::new();
        let input_owned = input.map(|value| value.to_string());
        let mut steer_rx = steer_rx;
        let shared_emitter = share_stream_emitter(emitter);
        let telemetry_sink = crate::telemetry::build_core_telemetry_sink(self.storage.as_ref());
        let base_telemetry_context = Self::background_telemetry_context(
            telemetry_context,
            background_task_id,
            background_task.as_ref(),
            Some(agent_id),
        )
            .with_requested_model(primary_model.as_serialized_str())
            .with_effective_model(primary_model.as_serialized_str())
            .with_provider(primary_provider.as_canonical_str());

        loop {
            let input_ref = input_owned.as_deref();
            let agent_node_clone = agent_node.clone();
            // Note: steer_rx is consumed on first execution attempt only.
            // Retries after this point won't have steering support.
            let mut previous_attempt_model: Option<ModelId> = None;
            let telemetry_sink = telemetry_sink.clone();
            let base_telemetry_context = base_telemetry_context.clone();
            let result = execute_with_failover(&failover_manager, |model| {
                let node = agent_node_clone.clone();
                let steer_rx = steer_rx.take();
                let previous_model = previous_attempt_model.replace(model);
                let emitter = clone_shared_emitter(&shared_emitter);
                let limits = resolved_resource_limits.clone();
                let telemetry_sink = telemetry_sink.clone();
                let telemetry_context = base_telemetry_context
                    .clone()
                    .with_effective_model(model.as_serialized_str());
                async move {
                    if let Some(previous_model) = previous_model
                        && previous_model != model
                    {
                        telemetry_sink
                            .emit(
                                restflow_telemetry::ExecutionEventEnvelope::from_telemetry_context(
                                    &telemetry_context,
                                    restflow_telemetry::ExecutionEvent::ModelSwitch {
                                        from_model: previous_model.as_serialized_str().to_string(),
                                        to_model: model.as_serialized_str().to_string(),
                                        reason: Some("failover".to_string()),
                                        success: true,
                                    },
                                ),
                            )
                            .await;
                    }
                    self.execute_with_profiles(
                        &node,
                        model,
                        background_task_id,
                        input_ref,
                        memory_config,
                        &limits,
                        primary_provider,
                        steer_rx,
                        emitter,
                        Some(agent_id),
                        None,
                        telemetry_context,
                    )
                    .await
                }
            })
            .await;

            match result {
                Ok((mut exec_result, final_model)) => {
                    exec_result.metrics.final_model = Some(final_model);
                    self.persist_deliverable_if_needed(
                        background_task_id,
                        agent_id,
                        &exec_result.output,
                    )?;
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

    #[allow(clippy::too_many_arguments)]
    async fn execute_internal_from_state(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: restflow_ai::AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        telemetry_context: Option<restflow_telemetry::TelemetryContext>,
    ) -> Result<ExecutionResult> {
        let stored_agent = self
            .storage
            .agents
            .get_agent(agent_id.to_string())?
            .ok_or_else(|| anyhow!("Agent '{}' not found", agent_id))?;
        // Fail closed on storage errors - do not silently swallow DB failures.
        let resolved_resource_limits = match background_task_id {
            Some(task_id) => match self.storage.background_agents.get_task(task_id) {
                Ok(Some(task)) => task.resource_limits,
                Ok(None) => {
                    warn!(task_id, "Background task not found, using default limits");
                    Default::default()
                }
                Err(e) => return Err(e),
            },
            None => Default::default(),
        };

        let agent_node = stored_agent.agent.clone();
        let primary_model = self.resolve_primary_model(&agent_node).await?;
        let primary_provider = primary_model.provider();
        self.run_preflight_check(&agent_node, primary_model, primary_provider, None)
            .await?;
        let background_task_snapshot = match background_task_id {
            Some(task_id) => self.storage.background_agents.get_task(task_id)?,
            None => None,
        };
        let base_telemetry_context = Self::background_telemetry_context(
            telemetry_context,
            background_task_id,
            background_task_snapshot.as_ref(),
            Some(agent_id),
        )
            .with_requested_model(primary_model.as_serialized_str())
            .with_effective_model(primary_model.as_serialized_str())
            .with_provider(primary_provider.as_canonical_str());

        self.execute_with_profiles(
            &agent_node,
            primary_model,
            background_task_id,
            None,
            memory_config,
            &resolved_resource_limits,
            primary_provider,
            steer_rx,
            emitter,
            Some(agent_id),
            Some(state),
            base_telemetry_context,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        BackgroundAgentSchedule, BackgroundAgentSpec, ExecutionMode, NotificationConfig,
    };
    use crate::storage::Storage;
    use tempfile::tempdir;

    #[test]
    fn should_force_non_stream_for_all_cli_models() {
        assert!(should_force_non_stream(ModelId::ClaudeCodeSonnet));
        assert!(should_force_non_stream(ModelId::CodexCli));
        assert!(should_force_non_stream(ModelId::GeminiCli));
        assert!(should_force_non_stream(ModelId::OpenCodeCli));
        assert!(!should_force_non_stream(ModelId::Gpt5));
    }

    #[test]
    fn background_telemetry_context_uses_runner_supplied_run_id() {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("background-telemetry-context.db");
        let storage = Arc::new(Storage::new(db_path.to_str().expect("db path")).expect("storage"));
        let task = storage
            .background_agents
            .create_background_agent(BackgroundAgentSpec {
                name: "Digest".to_string(),
                description: None,
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                input: Some("digest".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::default(),
                notification: Some(NotificationConfig::default()),
                execution_mode: Some(ExecutionMode::default()),
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("task");
        let stale_trace = restflow_telemetry::RestflowTrace::new(
            "run-stale",
            task.chat_session_id.clone(),
            task.id.clone(),
            task.agent_id.clone(),
        );
        futures::executor::block_on(crate::telemetry::emit_run_started(
            &crate::telemetry::build_execution_trace_sink(&storage.execution_traces),
            stale_trace,
        ));
        let explicit_context = restflow_telemetry::TelemetryContext::new(
            restflow_telemetry::RestflowTrace::new(
                "run-current",
                task.chat_session_id.clone(),
                task.id.clone(),
                "agent-stale",
            ),
        );
        let context = AgentRuntimeExecutor::background_telemetry_context(
            Some(explicit_context),
            Some(&task.id),
            Some(&task),
            Some("agent-1"),
        );

        assert_eq!(context.trace.run_id, "run-current");
        assert_eq!(context.trace.session_id, task.chat_session_id);
        assert_eq!(context.trace.scope_id, task.id);
        assert_eq!(context.trace.actor_id, "agent-1");
    }
}
