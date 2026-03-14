use super::*;

pub(super) struct CoreBackend {
    pub(super) core: Arc<AppCore>,
    pub(super) registry: std::sync::OnceLock<restflow_traits::registry::ToolRegistry>,
}

impl CoreBackend {
    fn parse_binding_channel_source(channel: &str) -> Option<ChatSessionSource> {
        match channel.trim().to_ascii_lowercase().as_str() {
            "telegram" => Some(ChatSessionSource::Telegram),
            "discord" => Some(ChatSessionSource::Discord),
            "slack" => Some(ChatSessionSource::Slack),
            _ => None,
        }
    }

    fn channel_key_from_source(source: ChatSessionSource) -> Option<&'static str> {
        match source {
            ChatSessionSource::Telegram => Some("telegram"),
            ChatSessionSource::Discord => Some("discord"),
            ChatSessionSource::Slack => Some("slack"),
            ChatSessionSource::Workspace | ChatSessionSource::ExternalLegacy => None,
        }
    }

    fn resolve_legacy_external_route(session: &ChatSession) -> Option<(ChatSessionSource, String)> {
        let source = match session.source_channel {
            Some(ChatSessionSource::Workspace) | None => return None,
            Some(source) => source,
        };
        let conversation_id = session
            .source_conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())?
            .to_string();
        Some((source, conversation_id))
    }

    fn ensure_binding_from_legacy_source(
        &self,
        session: &ChatSession,
    ) -> Result<Option<(ChatSessionSource, String)>, String> {
        let Some((source, conversation_id)) = Self::resolve_legacy_external_route(session) else {
            return Ok(None);
        };

        if let Some(channel_key) = Self::channel_key_from_source(source) {
            let binding = crate::models::ChannelSessionBinding::new(
                channel_key,
                None,
                &conversation_id,
                &session.id,
            );
            self.core
                .storage
                .channel_session_bindings
                .upsert(&binding)
                .map_err(|e| e.to_string())?;
        }

        Ok(Some((source, conversation_id)))
    }

    fn apply_effective_session_source(&self, session: &mut ChatSession) -> Result<(), String> {
        let bindings = self
            .core
            .storage
            .channel_session_bindings
            .list_by_session(&session.id)
            .map_err(|e| e.to_string())?;
        if let Some(binding) = bindings.first() {
            let effective_source = Self::parse_binding_channel_source(&binding.channel)
                .unwrap_or(ChatSessionSource::ExternalLegacy);
            session.source_channel = Some(effective_source);
            session.source_conversation_id = Some(binding.conversation_id.clone());
            return Ok(());
        }

        if let Some((source, conversation_id)) = self.ensure_binding_from_legacy_source(session)? {
            session.source_channel = Some(source);
            session.source_conversation_id = Some(conversation_id);
            return Ok(());
        }

        session.source_channel = Some(ChatSessionSource::Workspace);
        session.source_conversation_id = None;
        Ok(())
    }

    fn get_registry(&self) -> Result<&restflow_traits::registry::ToolRegistry, String> {
        if let Some(r) = self.registry.get() {
            return Ok(r);
        }
        let r = create_runtime_tool_registry_for_core(&self.core).map_err(|e| e.to_string())?;
        // If another thread raced us, that's fine — return whichever won.
        let _ = self.registry.set(r);
        Ok(self.registry.get().unwrap())
    }
}

#[async_trait::async_trait]
impl McpBackend for CoreBackend {
    async fn list_skills(&self) -> Result<Vec<Skill>, String> {
        crate::services::skills::list_skills(&self.core)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_skill(&self, id: &str) -> Result<Option<Skill>, String> {
        crate::services::skills::get_skill(&self.core, id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_skill_reference(
        &self,
        skill_id: &str,
        ref_id: &str,
    ) -> Result<Option<String>, String> {
        crate::services::skills::get_skill_reference(&self.core, skill_id, ref_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn create_skill(&self, skill: Skill) -> Result<(), String> {
        crate::services::skills::create_skill(&self.core, skill)
            .await
            .map_err(|e| e.to_string())
    }

    async fn update_skill(&self, skill: Skill) -> Result<(), String> {
        crate::services::skills::update_skill(&self.core, &skill.id, &skill)
            .await
            .map_err(|e| e.to_string())
    }

    async fn delete_skill(&self, id: &str) -> Result<(), String> {
        crate::services::skills::delete_skill(&self.core, id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn list_agents(&self) -> Result<Vec<StoredAgent>, String> {
        crate::services::agent::list_agents(&self.core)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_agent(&self, id: &str) -> Result<StoredAgent, String> {
        crate::services::agent::get_agent(&self.core, id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn search_memory(&self, query: MemorySearchQuery) -> Result<MemorySearchResult, String> {
        self.core
            .storage
            .memory
            .search(&query)
            .map_err(|e| e.to_string())
    }

    async fn store_memory(&self, chunk: MemoryChunk) -> Result<String, String> {
        self.core
            .storage
            .memory
            .store_chunk(&chunk)
            .map_err(|e| e.to_string())
    }

    async fn get_memory_stats(&self, agent_id: &str) -> Result<MemoryStats, String> {
        self.core
            .storage
            .memory
            .get_stats(agent_id)
            .map_err(|e| e.to_string())
    }

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>, String> {
        let mut sessions = self
            .core
            .storage
            .chat_sessions
            .list()
            .map_err(|e| e.to_string())?;
        for session in &mut sessions {
            self.apply_effective_session_source(session)?;
        }
        Ok(sessions.iter().map(ChatSessionSummary::from).collect())
    }

    async fn list_sessions_by_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<ChatSessionSummary>, String> {
        let sessions = self
            .core
            .storage
            .chat_sessions
            .list_by_agent(agent_id)
            .map_err(|e| e.to_string())?;
        let mut sessions = sessions;
        for session in &mut sessions {
            self.apply_effective_session_source(session)?;
        }
        Ok(sessions.iter().map(ChatSessionSummary::from).collect())
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession, String> {
        let mut session = self
            .core
            .storage
            .chat_sessions
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session not found: {}", id))?;
        self.apply_effective_session_source(&mut session)?;
        Ok(session)
    }

    async fn list_tasks(
        &self,
        status: Option<BackgroundAgentStatus>,
    ) -> Result<Vec<BackgroundAgent>, String> {
        match status {
            Some(status) => self
                .core
                .storage
                .background_agents
                .list_tasks_by_status(status)
                .map_err(|e| e.to_string()),
            None => self
                .core
                .storage
                .background_agents
                .list_tasks()
                .map_err(|e| e.to_string()),
        }
    }

    async fn create_background_agent(
        &self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent, String> {
        self.core
            .storage
            .background_agents
            .create_background_agent(spec)
            .map_err(|e| e.to_string())
    }

    async fn update_background_agent(
        &self,
        id: &str,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent, String> {
        self.core
            .storage
            .background_agents
            .update_background_agent(id, patch)
            .map_err(|e| e.to_string())
    }

    async fn delete_background_agent(&self, id: &str) -> Result<bool, String> {
        self.core
            .storage
            .background_agents
            .delete_task(id)
            .map_err(|e| e.to_string())
    }

    async fn control_background_agent(
        &self,
        id: &str,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent, String> {
        self.core
            .storage
            .background_agents
            .control_background_agent(id, action)
            .map_err(|e| e.to_string())
    }

    async fn get_background_agent_progress(
        &self,
        id: &str,
        event_limit: usize,
    ) -> Result<BackgroundProgress, String> {
        self.core
            .storage
            .background_agents
            .get_background_agent_progress(id, event_limit)
            .map_err(|e| e.to_string())
    }

    async fn send_background_agent_message(
        &self,
        id: &str,
        message: String,
        source: BackgroundMessageSource,
    ) -> Result<BackgroundMessage, String> {
        self.core
            .storage
            .background_agents
            .send_background_agent_message(id, message, source)
            .map_err(|e| e.to_string())
    }

    async fn list_background_agent_messages(
        &self,
        id: &str,
        limit: usize,
    ) -> Result<Vec<BackgroundMessage>, String> {
        self.core
            .storage
            .background_agents
            .list_background_agent_messages(id, limit)
            .map_err(|e| e.to_string())
    }

    async fn list_deliverables(&self, task_id: &str) -> Result<Vec<Deliverable>, String> {
        self.core
            .storage
            .deliverables
            .list_by_task(task_id)
            .map_err(|e| e.to_string())
    }

    async fn list_tool_traces(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<crate::models::ToolTrace>, String> {
        self.core
            .storage
            .tool_traces
            .list_by_session(session_id, Some(limit))
            .map_err(|e| e.to_string())
    }

    async fn list_tool_traces_by_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        limit: usize,
    ) -> Result<Vec<crate::models::ToolTrace>, String> {
        self.core
            .storage
            .tool_traces
            .list_by_session_turn(session_id, turn_id, Some(limit))
            .map_err(|e| e.to_string())
    }

    async fn get_background_agent(&self, id: &str) -> Result<Value, String> {
        let task = self
            .core
            .storage
            .background_agents
            .get_task(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Task {} not found", id))?;
        serde_json::to_value(task).map_err(|e| e.to_string())
    }

    async fn list_hooks(&self) -> Result<Vec<Hook>, String> {
        self.core.storage.hooks.list().map_err(|e| e.to_string())
    }

    async fn create_hook(&self, hook: Hook) -> Result<Hook, String> {
        self.core
            .storage
            .hooks
            .create(&hook)
            .map_err(|e| e.to_string())?;
        Ok(hook)
    }

    async fn update_hook(&self, id: &str, hook: Hook) -> Result<Hook, String> {
        self.core
            .storage
            .hooks
            .update(id, &hook)
            .map_err(|e| e.to_string())?;
        Ok(hook)
    }

    async fn delete_hook(&self, id: &str) -> Result<bool, String> {
        self.core
            .storage
            .hooks
            .delete(id)
            .map_err(|e| e.to_string())
    }

    async fn list_runtime_tools(&self) -> Result<Vec<RuntimeToolDefinition>, String> {
        let registry = self.get_registry()?;
        Ok(registry
            .schemas()
            .into_iter()
            .map(|schema| RuntimeToolDefinition {
                name: schema.name,
                description: schema.description,
                parameters: schema.parameters,
            })
            .collect())
    }

    async fn execute_runtime_tool(
        &self,
        name: &str,
        input: Value,
    ) -> Result<RuntimeToolResult, String> {
        let registry = self.get_registry()?;
        let output = registry
            .execute_safe(name, input)
            .await
            .map_err(|e| e.to_string())?;
        Ok(RuntimeToolResult {
            success: output.success,
            result: output.result,
            error: output.error,
            error_category: output.error_category,
            retryable: output.retryable,
            retry_after_ms: output.retry_after_ms,
        })
    }

    async fn get_api_defaults(&self) -> Result<ApiDefaults, String> {
        let config = match self.core.storage.config.get_effective_config() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(
                    %error,
                    "Failed to load effective config overrides for API defaults; using stored values"
                );
                self.core
                    .storage
                    .config
                    .get_config()
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default()
            }
        };
        Ok(config.api_defaults)
    }
}

pub(super) struct IpcBackend {
    pub(super) client: Arc<Mutex<IpcClient>>,
}

impl IpcBackend {
    fn format_ipc_error(code: i32, message: &str, details: Option<Value>) -> String {
        match details {
            Some(details) => serde_json::json!({
                "code": code,
                "message": message,
                "details": details
            })
            .to_string(),
            None => format!("IPC error {}: {}", code, message),
        }
    }

    async fn request_typed<T: DeserializeOwned>(&self, req: IpcRequest) -> Result<T, String> {
        let mut client = self.client.lock().await;
        match client.request(req).await.map_err(|e| e.to_string())? {
            IpcResponse::Success(value) => serde_json::from_value(value).map_err(|e| e.to_string()),
            IpcResponse::Error {
                code,
                message,
                details,
            } => Err(Self::format_ipc_error(code, &message, details)),
            IpcResponse::Pong => Err("Unexpected IPC pong response".to_string()),
        }
    }
}

#[async_trait::async_trait]
impl McpBackend for IpcBackend {
    async fn list_skills(&self) -> Result<Vec<Skill>, String> {
        let mut client = self.client.lock().await;
        client.list_skills().await.map_err(|e| e.to_string())
    }

    async fn get_skill(&self, id: &str) -> Result<Option<Skill>, String> {
        let mut client = self.client.lock().await;
        client
            .get_skill(id.to_string())
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_skill_reference(
        &self,
        skill_id: &str,
        ref_id: &str,
    ) -> Result<Option<String>, String> {
        let mut client = self.client.lock().await;
        client
            .get_skill_reference(skill_id.to_string(), ref_id.to_string())
            .await
            .map_err(|e| e.to_string())
    }

    async fn create_skill(&self, skill: Skill) -> Result<(), String> {
        let mut client = self.client.lock().await;
        client.create_skill(skill).await.map_err(|e| e.to_string())
    }

    async fn update_skill(&self, skill: Skill) -> Result<(), String> {
        let mut client = self.client.lock().await;
        client
            .update_skill(skill.id.clone(), skill)
            .await
            .map_err(|e| e.to_string())
    }

    async fn delete_skill(&self, id: &str) -> Result<(), String> {
        let mut client = self.client.lock().await;
        client
            .delete_skill(id.to_string())
            .await
            .map_err(|e| e.to_string())
    }

    async fn list_agents(&self) -> Result<Vec<StoredAgent>, String> {
        let mut client = self.client.lock().await;
        client.list_agents().await.map_err(|e| e.to_string())
    }

    async fn get_agent(&self, id: &str) -> Result<StoredAgent, String> {
        let mut client = self.client.lock().await;
        client
            .get_agent(id.to_string())
            .await
            .map_err(|e| e.to_string())
    }

    async fn search_memory(&self, query: MemorySearchQuery) -> Result<MemorySearchResult, String> {
        let mut client = self.client.lock().await;
        let text = query.query.unwrap_or_default();
        client
            .search_memory(text, Some(query.agent_id), Some(query.limit))
            .await
            .map_err(|e| e.to_string())
    }

    async fn store_memory(&self, chunk: MemoryChunk) -> Result<String, String> {
        let mut client = self.client.lock().await;
        client
            .create_memory_chunk(chunk)
            .await
            .map(|stored| stored.id)
            .map_err(|e| e.to_string())
    }

    async fn get_memory_stats(&self, agent_id: &str) -> Result<MemoryStats, String> {
        let mut client = self.client.lock().await;
        client
            .get_memory_stats(Some(agent_id.to_string()))
            .await
            .map_err(|e| e.to_string())
    }

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>, String> {
        let mut client = self.client.lock().await;
        client.list_sessions().await.map_err(|e| e.to_string())
    }

    async fn list_sessions_by_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<ChatSessionSummary>, String> {
        let mut client = self.client.lock().await;
        let sessions = client
            .list_sessions_by_agent(agent_id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        Ok(sessions.iter().map(ChatSessionSummary::from).collect())
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession, String> {
        let mut client = self.client.lock().await;
        client
            .get_session(id.to_string())
            .await
            .map_err(|e| e.to_string())
    }

    async fn list_tasks(
        &self,
        status: Option<BackgroundAgentStatus>,
    ) -> Result<Vec<BackgroundAgent>, String> {
        let mut client = self.client.lock().await;
        client
            .list_background_agents(status.map(|value| value.as_str().to_string()))
            .await
            .map_err(|e| e.to_string())
    }

    async fn create_background_agent(
        &self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent, String> {
        self.request_typed(IpcRequest::CreateBackgroundAgent { spec })
            .await
    }

    async fn update_background_agent(
        &self,
        id: &str,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent, String> {
        self.request_typed(IpcRequest::UpdateBackgroundAgent {
            id: id.to_string(),
            patch,
        })
        .await
    }

    async fn delete_background_agent(&self, id: &str) -> Result<bool, String> {
        #[derive(Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }

        let response: DeleteResponse = self
            .request_typed(IpcRequest::DeleteBackgroundAgent { id: id.to_string() })
            .await?;
        Ok(response.deleted)
    }

    async fn control_background_agent(
        &self,
        id: &str,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent, String> {
        self.request_typed(IpcRequest::ControlBackgroundAgent {
            id: id.to_string(),
            action,
        })
        .await
    }

    async fn get_background_agent_progress(
        &self,
        id: &str,
        event_limit: usize,
    ) -> Result<BackgroundProgress, String> {
        self.request_typed(IpcRequest::GetBackgroundAgentProgress {
            id: id.to_string(),
            event_limit: Some(event_limit),
        })
        .await
    }

    async fn send_background_agent_message(
        &self,
        id: &str,
        message: String,
        source: BackgroundMessageSource,
    ) -> Result<BackgroundMessage, String> {
        self.request_typed(IpcRequest::SendBackgroundAgentMessage {
            id: id.to_string(),
            message,
            source: Some(source),
        })
        .await
    }

    async fn list_background_agent_messages(
        &self,
        id: &str,
        limit: usize,
    ) -> Result<Vec<BackgroundMessage>, String> {
        self.request_typed(IpcRequest::ListBackgroundAgentMessages {
            id: id.to_string(),
            limit: Some(limit),
        })
        .await
    }

    async fn list_deliverables(&self, task_id: &str) -> Result<Vec<Deliverable>, String> {
        let result = self
            .execute_runtime_tool(
                "manage_background_agents",
                serde_json::json!({
                    "operation": "list_deliverables",
                    "id": task_id,
                }),
            )
            .await?;
        if !result.success {
            return Err(result
                .error
                .unwrap_or_else(|| "Runtime tool execution failed".to_string()));
        }
        serde_json::from_value(result.result).map_err(|e| e.to_string())
    }

    async fn list_tool_traces(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<crate::models::ToolTrace>, String> {
        self.request_typed(IpcRequest::ListToolTraces {
            session_id: session_id.to_string(),
            turn_id: None,
            limit: Some(limit),
        })
        .await
    }

    async fn list_tool_traces_by_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        limit: usize,
    ) -> Result<Vec<crate::models::ToolTrace>, String> {
        self.request_typed(IpcRequest::ListToolTraces {
            session_id: session_id.to_string(),
            turn_id: Some(turn_id.to_string()),
            limit: Some(limit),
        })
        .await
    }

    async fn get_background_agent(&self, id: &str) -> Result<Value, String> {
        let mut client = self.client.lock().await;
        let task: Option<BackgroundAgent> = client
            .get_background_agent(id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        let task = task.ok_or_else(|| format!("Task {} not found", id))?;
        serde_json::to_value(task).map_err(|e| e.to_string())
    }

    async fn list_hooks(&self) -> Result<Vec<Hook>, String> {
        self.request_typed(IpcRequest::ListHooks).await
    }

    async fn create_hook(&self, hook: Hook) -> Result<Hook, String> {
        self.request_typed(IpcRequest::CreateHook { hook }).await
    }

    async fn update_hook(&self, id: &str, hook: Hook) -> Result<Hook, String> {
        self.request_typed(IpcRequest::UpdateHook {
            id: id.to_string(),
            hook,
        })
        .await
    }

    async fn delete_hook(&self, id: &str) -> Result<bool, String> {
        #[derive(Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let response: DeleteResponse = self
            .request_typed(IpcRequest::DeleteHook { id: id.to_string() })
            .await?;
        Ok(response.deleted)
    }

    async fn list_runtime_tools(&self) -> Result<Vec<RuntimeToolDefinition>, String> {
        let mut client = self.client.lock().await;
        let tools = client
            .get_available_tool_definitions()
            .await
            .map_err(|e| e.to_string())?;
        Ok(tools
            .into_iter()
            .map(|tool| RuntimeToolDefinition {
                name: tool.name,
                description: tool.description,
                parameters: tool.parameters,
            })
            .collect())
    }

    async fn execute_runtime_tool(
        &self,
        name: &str,
        input: Value,
    ) -> Result<RuntimeToolResult, String> {
        let mut client = self.client.lock().await;
        let output = client
            .execute_tool(name.to_string(), input)
            .await
            .map_err(|e| e.to_string())?;
        Ok(RuntimeToolResult {
            success: output.success,
            result: output.result,
            error: output.error,
            error_category: output.error_category,
            retryable: output.retryable,
            retry_after_ms: output.retry_after_ms,
        })
    }

    async fn get_api_defaults(&self) -> Result<ApiDefaults, String> {
        let config: SystemConfig = self.request_typed(IpcRequest::GetConfig).await?;
        Ok(config.api_defaults)
    }
}
