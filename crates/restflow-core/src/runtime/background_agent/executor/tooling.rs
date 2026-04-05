use super::*;
use restflow_ai::agent::SubagentManagerImpl;
use restflow_traits::SubagentManager;

impl AgentRuntimeExecutor {
    pub(super) fn to_agent_resource_limits(
        limits: &crate::models::ResourceLimits,
    ) -> AgentResourceLimits {
        AgentResourceLimits {
            max_tool_calls: limits.max_tool_calls,
            max_wall_clock: Duration::from_secs(limits.max_duration_secs),
            max_depth: AgentResourceLimits::default().max_depth,
            max_cost_usd: limits.max_cost_usd,
        }
    }

    pub(super) fn chat_resource_limits(
        max_tool_calls: usize,
        max_wall_clock_secs: Option<u64>,
    ) -> AgentResourceLimits {
        AgentResourceLimits {
            max_tool_calls,
            max_wall_clock: max_wall_clock_secs
                .map(Duration::from_secs)
                .unwrap_or(Duration::ZERO),
            max_depth: AgentResourceLimits::default().max_depth,
            max_cost_usd: None,
        }
    }

    pub(super) fn apply_llm_timeout(
        mut config: ReActAgentConfig,
        llm_timeout_secs: Option<u64>,
    ) -> ReActAgentConfig {
        if let Some(timeout_secs) = llm_timeout_secs {
            config = config.with_llm_timeout(Duration::from_secs(timeout_secs));
        } else {
            config = config.without_llm_timeout();
        }
        config
    }

    pub(super) fn apply_execution_context(
        mut config: ReActAgentConfig,
        context: &ExecutionContext,
    ) -> ReActAgentConfig {
        config = config.with_context("execution_context", context.to_value());
        config = config.with_context(
            "execution_role",
            serde_json::Value::String(context.role.as_str().to_string()),
        );
        if let Some(session_id) = &context.chat_session_id {
            config = config.with_context(
                "chat_session_id",
                serde_json::Value::String(session_id.clone()),
            );
        }
        if let Some(task_id) = &context.background_task_id {
            config = config.with_context(
                "background_task_id",
                serde_json::Value::String(task_id.clone()),
            );
        }
        if let Some(parent_run_id) = &context.parent_run_id {
            config = config.with_context(
                "parent_run_id",
                serde_json::Value::String(parent_run_id.clone()),
            );
        }
        config
    }

    pub(super) fn non_main_agent_prompt_flags() -> PromptFlags {
        PromptFlags::new().without_workspace_context()
    }

    pub(super) fn effective_max_tool_result_length(
        requested_max_output_bytes: usize,
        context_window: usize,
    ) -> usize {
        let requested = requested_max_output_bytes.max(1);
        let context_token_budget =
            ((context_window as f64) * TOOL_RESULT_CONTEXT_RATIO).round() as usize;
        let context_char_budget =
            context_token_budget.saturating_mul(TOOL_RESULT_CHARS_PER_TOKEN_ESTIMATE);
        let context_cap = context_char_budget.clamp(TOOL_RESULT_MIN_CHARS, TOOL_RESULT_MAX_CHARS);
        requested.min(context_cap)
    }

    pub(super) fn build_subagent_manager(
        &self,
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<ToolRegistry>,
        llm_client_factory: Arc<dyn LlmClientFactory>,
    ) -> SubagentManagerImpl {
        SubagentManagerImpl::new(
            self.subagent_tracker.clone(),
            self.subagent_definitions.clone(),
            llm_client,
            tool_registry,
            self.subagent_config.clone(),
        )
        .with_llm_client_factory(llm_client_factory)
        .with_orchestrator(Arc::new(AgentOrchestratorImpl::from_runtime_executor(
            self.clone(),
        )))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_tool_registry(
        &self,
        tool_names: Option<&[String]>,
        llm_client: Arc<dyn LlmClient>,
        swappable: Arc<SwappableLlm>,
        factory: Arc<dyn LlmClientFactory>,
        agent_id: Option<&str>,
        bash_config: Option<BashConfig>,
        reply_sender: Option<Arc<dyn ReplySender>>,
        workspace_root: Option<&std::path::Path>,
    ) -> anyhow::Result<Arc<ToolRegistry>> {
        let has_reply_sender = reply_sender.is_some();
        let filtered_tool_names = self.filter_requested_tool_names(tool_names, has_reply_sender);
        let filtered_tool_names_ref = filtered_tool_names.as_deref();
        let secret_resolver = Some(secret_resolver_from_storage(&self.storage));
        let subagent_tool_registry = Arc::new(registry_from_allowlist(
            filtered_tool_names_ref,
            None,
            secret_resolver.clone(),
            Some(self.storage.as_ref()),
            agent_id,
            bash_config.clone(),
            workspace_root,
        )?);
        let subagent_manager: Arc<dyn SubagentManager> = Arc::new(self.build_subagent_manager(
            llm_client,
            subagent_tool_registry,
            factory.clone(),
        ));
        let mut registry = registry_from_allowlist(
            filtered_tool_names_ref,
            Some(subagent_manager),
            secret_resolver,
            Some(self.storage.as_ref()),
            agent_id,
            bash_config,
            workspace_root,
        )?;

        let requested = |name: &str| {
            filtered_tool_names_ref
                .map(|names| names.iter().any(|n| n == name))
                .unwrap_or(false)
        };

        if requested("switch_model") {
            let switcher = Arc::new(LlmSwitcherImpl::new(swappable, factory));
            registry.register(SwitchModelTool::new(switcher));
        }

        if requested("process") {
            registry.register(ProcessTool::new(self.process_registry.clone()));
        }

        if requested("reply")
            && let Some(sender) = reply_sender
        {
            registry.register(ReplyTool::new(sender));
        }

        Ok(Arc::new(registry))
    }

    pub(super) fn filter_requested_tool_names(
        &self,
        tool_names: Option<&[String]>,
        has_reply_sender: bool,
    ) -> Option<Vec<String>> {
        let names = tool_names?;

        Some(
            names
                .iter()
                .filter_map(|name| {
                    if name == "reply" && !has_reply_sender {
                        debug!(
                            tool_name = "reply",
                            "Reply sender missing in this execution context; skipping tool"
                        );
                        return None;
                    }
                    Some(name.clone())
                })
                .collect(),
        )
    }

    pub(super) fn resolve_reply_sender(
        &self,
        background_task_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> Option<Arc<dyn ReplySender>> {
        if let Some(task_id) = background_task_id
            && let Some(factory) = &self.reply_sender_factory
        {
            let current_agent_id = agent_id.unwrap_or_default();
            if let Some(sender) = factory.for_background_task(task_id, current_agent_id) {
                return Some(sender);
            }
        }

        self.reply_sender.clone()
    }
}
