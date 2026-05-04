use super::*;
use crate::boundary::task::{
    core_patch_to_contract, core_patch_to_update_request, core_spec_to_contract,
    core_spec_to_create_request,
};
use crate::daemon::tool_result_mapper::to_tool_execution_result;
use crate::services::operation_assessment::OperationAssessorAdapter;
use crate::services::task_command::{TaskCommandService, TaskExecutionMode};

fn resolve_task_id(
    storage: &crate::storage::TaskStorage,
    id_or_prefix: &str,
) -> Result<String, String> {
    storage
        .resolve_existing_task_id_typed(id_or_prefix)
        .map_err(|e| e.to_string())
}
use crate::daemon::request_mapper::to_contract;
use restflow_contracts::DeleteWithIdResponse;
use restflow_traits::{TaskCommandOutcome, store::TaskDeleteRequest};

pub(super) struct CoreBackend {
    pub(super) core: Arc<AppCore>,
    pub(super) registry: std::sync::OnceLock<restflow_traits::registry::ToolRegistry>,
}

impl CoreBackend {
    fn session_service(&self) -> crate::services::session::SessionService {
        crate::services::session::SessionService::from_storage(&self.core.storage)
    }

    fn task_command_service(&self) -> TaskCommandService {
        TaskCommandService::from_storage(
            self.core.storage.as_ref(),
            Some(Arc::new(OperationAssessorAdapter::new(self.core.clone()))),
        )
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
        let session_service = self.session_service();
        let sessions = session_service
            .list_session_views(None, None, false)
            .map_err(|e| e.to_string())?;
        Ok(sessions.iter().map(ChatSessionSummary::from).collect())
    }

    async fn list_sessions_by_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<ChatSessionSummary>, String> {
        let session_service = self.session_service();
        let sessions = session_service
            .list_session_views(Some(agent_id), None, false)
            .map_err(|e| e.to_string())?;
        Ok(sessions.iter().map(ChatSessionSummary::from).collect())
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession, String> {
        self.session_service()
            .get_session_view(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session not found: {}", id))
    }

    async fn list_tasks(&self, status: Option<TaskStatus>) -> Result<Vec<Task>, String> {
        match status {
            Some(status) => self
                .core
                .storage
                .tasks
                .list_tasks_by_status(status)
                .map_err(|e| e.to_string()),
            None => self
                .core
                .storage
                .tasks
                .list_tasks()
                .map_err(|e| e.to_string()),
        }
    }

    async fn create_task(&self, spec: TaskSpec) -> Result<Task, String> {
        let request = core_spec_to_create_request(&spec).map_err(|e| e.to_string())?;
        let outcome = self
            .task_command_service()
            .create_from_request(request, TaskExecutionMode::Direct)
            .await
            .map_err(|e| e.to_string())?;
        TaskCommandService::into_direct_result(outcome).map_err(|e| e.to_string())
    }

    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task, String> {
        let request =
            core_patch_to_update_request(id.to_string(), &patch).map_err(|e| e.to_string())?;
        let outcome = self
            .task_command_service()
            .update_from_request(request, TaskExecutionMode::Direct)
            .await
            .map_err(|e| e.to_string())?;
        TaskCommandService::into_direct_result(outcome).map_err(|e| e.to_string())
    }

    async fn delete_task(
        &self,
        request: TaskDeleteRequest,
    ) -> Result<TaskCommandOutcome<DeleteWithIdResponse>, String> {
        self.task_command_service()
            .delete_from_request(request, TaskExecutionMode::Guarded)
            .await
            .map_err(|e| e.to_string())
    }

    async fn control_task(&self, id: &str, action: TaskControlAction) -> Result<Task, String> {
        let action = to_contract(action).map_err(|e| e.to_string())?;
        let outcome = self
            .task_command_service()
            .control_from_request(
                restflow_traits::store::TaskControlRequest {
                    id: id.to_string(),
                    action,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .map_err(|e| e.to_string())?;
        TaskCommandService::into_direct_result(outcome).map_err(|e| e.to_string())
    }

    async fn get_task_progress(
        &self,
        id: &str,
        event_limit: usize,
    ) -> Result<TaskProgress, String> {
        self.core
            .storage
            .tasks
            .get_task_progress(id, event_limit)
            .map_err(|e| e.to_string())
    }

    async fn send_task_message(
        &self,
        id: &str,
        message: String,
        source: TaskMessageSource,
    ) -> Result<TaskMessage, String> {
        self.core
            .storage
            .tasks
            .send_task_message(id, message, source)
            .map_err(|e| e.to_string())
    }

    async fn list_task_messages(&self, id: &str, limit: usize) -> Result<Vec<TaskMessage>, String> {
        self.core
            .storage
            .tasks
            .list_task_messages(id, limit)
            .map_err(|e| e.to_string())
    }

    async fn list_artifacts(&self, task_id: &str) -> Result<Vec<RunArtifact>, String> {
        resolve_task_id(&self.core.storage.tasks, task_id)?;
        Ok(Vec::new())
    }

    async fn list_runs(&self, query: RunListQuery) -> Result<Vec<RunSummary>, String> {
        crate::services::execution_console::ExecutionConsoleService::from_storage(
            &self.core.storage,
        )
        .list_runs(&query)
        .map_err(|e| e.to_string())
    }

    async fn query_execution_traces(
        &self,
        query: crate::models::ExecutionTraceQuery,
    ) -> Result<Vec<crate::models::ExecutionTraceEvent>, String> {
        self.core
            .storage
            .execution_traces
            .query(&query)
            .map_err(|e| e.to_string())
    }

    async fn query_execution_run_traces(
        &self,
        run_id: &str,
        limit: usize,
    ) -> Result<Vec<crate::models::ExecutionTraceEvent>, String> {
        self.core
            .storage
            .execution_traces
            .query(&crate::models::ExecutionTraceQuery {
                task_id: None,
                run_id: Some(run_id.to_string()),
                parent_run_id: None,
                session_id: None,
                turn_id: None,
                agent_id: None,
                category: None,
                source: None,
                from_timestamp: None,
                to_timestamp: None,
                limit: Some(limit),
                offset: Some(0),
            })
            .map_err(|e| e.to_string())
    }

    async fn get_task(&self, id: &str) -> Result<Task, String> {
        let resolved_id = resolve_task_id(&self.core.storage.tasks, id)?;
        self.core
            .storage
            .tasks
            .get_task(&resolved_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Task {} not found", resolved_id))
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
        Ok(to_tool_execution_result(output))
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
    async fn request_typed<T: DeserializeOwned>(&self, req: IpcRequest) -> Result<T, String> {
        let mut client = self.client.lock().await;
        client.request_typed(req).await.map_err(|e| e.to_string())
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

    async fn list_tasks(&self, status: Option<TaskStatus>) -> Result<Vec<Task>, String> {
        let mut client = self.client.lock().await;
        client
            .list_tasks(status.map(|value| value.as_str().to_string()))
            .await
            .map_err(|e| e.to_string())
    }

    async fn create_task(&self, spec: TaskSpec) -> Result<Task, String> {
        let spec = core_spec_to_contract(spec).map_err(|e| e.to_string())?;
        self.request_typed(IpcRequest::CreateTask { spec }).await
    }

    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task, String> {
        let patch = core_patch_to_contract(patch).map_err(|e| e.to_string())?;
        self.request_typed(IpcRequest::UpdateTask {
            id: id.to_string(),
            patch,
        })
        .await
    }

    async fn delete_task(
        &self,
        request: TaskDeleteRequest,
    ) -> Result<TaskCommandOutcome<DeleteWithIdResponse>, String> {
        if request.preview || request.approval_id.is_some() {
            return Err(
                "Preview and confirmation replay are no longer available for IPC task deletions."
                    .to_string(),
            );
        }
        let result = self
            .request_typed(IpcRequest::DeleteTask { id: request.id })
            .await?;
        Ok(TaskCommandOutcome::Executed { result })
    }

    async fn control_task(&self, id: &str, action: TaskControlAction) -> Result<Task, String> {
        let action = to_contract(action).map_err(|e| e.to_string())?;
        self.request_typed(IpcRequest::ControlTask {
            id: id.to_string(),
            action,
        })
        .await
    }

    async fn get_task_progress(
        &self,
        id: &str,
        event_limit: usize,
    ) -> Result<TaskProgress, String> {
        self.request_typed(IpcRequest::GetTaskProgress {
            id: id.to_string(),
            event_limit: Some(event_limit),
        })
        .await
    }

    async fn send_task_message(
        &self,
        id: &str,
        message: String,
        source: TaskMessageSource,
    ) -> Result<TaskMessage, String> {
        let source = to_contract(source).map_err(|e| e.to_string())?;
        self.request_typed(IpcRequest::SendTaskMessage {
            id: id.to_string(),
            message,
            source: Some(source),
        })
        .await
    }

    async fn list_task_messages(&self, id: &str, limit: usize) -> Result<Vec<TaskMessage>, String> {
        self.request_typed(IpcRequest::ListTaskMessages {
            id: id.to_string(),
            limit: Some(limit),
        })
        .await
    }

    async fn list_artifacts(&self, task_id: &str) -> Result<Vec<RunArtifact>, String> {
        self.request_typed(IpcRequest::ListRunArtifacts {
            run_id: None,
            task_id: Some(task_id.to_string()),
        })
        .await
    }

    async fn list_runs(&self, query: RunListQuery) -> Result<Vec<RunSummary>, String> {
        let query = to_contract(query).map_err(|e| e.to_string())?;
        self.request_typed(IpcRequest::ListRuns { query }).await
    }

    async fn query_execution_traces(
        &self,
        query: crate::models::ExecutionTraceQuery,
    ) -> Result<Vec<crate::models::ExecutionTraceEvent>, String> {
        let query = to_contract(query).map_err(|e| e.to_string())?;
        self.request_typed(IpcRequest::QueryExecutionTraces { query })
            .await
    }

    async fn query_execution_run_traces(
        &self,
        run_id: &str,
        limit: usize,
    ) -> Result<Vec<crate::models::ExecutionTraceEvent>, String> {
        let query = to_contract(crate::models::ExecutionTraceQuery {
            task_id: None,
            run_id: Some(run_id.to_string()),
            parent_run_id: None,
            session_id: None,
            turn_id: None,
            agent_id: None,
            category: None,
            source: None,
            from_timestamp: None,
            to_timestamp: None,
            limit: Some(limit),
            offset: Some(0),
        })
        .map_err(|e| e.to_string())?;
        self.request_typed(IpcRequest::QueryExecutionTraces { query })
            .await
    }

    async fn get_task(&self, id: &str) -> Result<Task, String> {
        let mut client = self.client.lock().await;
        client
            .get_task(id.to_string())
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Task {} not found", id))
    }

    async fn list_runtime_tools(&self) -> Result<Vec<RuntimeToolDefinition>, String> {
        let mut client = self.client.lock().await;
        let tools: Vec<RuntimeToolDefinition> = client
            .get_available_tool_definitions()
            .await
            .map_err(|e: anyhow::Error| e.to_string())?;
        Ok(tools.into_iter().collect())
    }

    async fn execute_runtime_tool(
        &self,
        name: &str,
        input: Value,
    ) -> Result<RuntimeToolResult, String> {
        let mut client = self.client.lock().await;
        let output: RuntimeToolResult = client
            .execute_tool(name.to_string(), input)
            .await
            .map_err(|e: anyhow::Error| e.to_string())?;
        Ok(output)
    }

    async fn get_api_defaults(&self) -> Result<ApiDefaults, String> {
        let config: SystemConfig = self.request_typed(IpcRequest::GetConfig).await?;
        Ok(config.api_defaults)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TaskSchedule;

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn core_backend_task_mutations_use_command_service_resolution() {
        let _env_lock = crate::prompt_files::agents_dir_env_lock();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let agents_dir = tempfile::tempdir().expect("agents dir");
        unsafe {
            std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_dir.path());
        }
        let db_path = temp_dir.path().join("mcp-core-backend.db");
        let core = Arc::new(
            AppCore::new(db_path.to_str().expect("db path"))
                .await
                .expect("core"),
        );
        let agent_id = core
            .storage
            .agents
            .resolve_default_agent_id()
            .expect("default agent");
        let backend = CoreBackend {
            core,
            registry: std::sync::OnceLock::new(),
        };

        let created = backend
            .create_task(TaskSpec {
                name: "MCP Core Task".to_string(),
                agent_id: agent_id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("Run from MCP core backend".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .await
            .expect("create task");

        assert_eq!(created.agent_id, agent_id);

        let prefix = created.id[..8].to_string();
        let updated = backend
            .update_task(
                &prefix,
                TaskPatch {
                    name: Some("Updated MCP Core Task".to_string()),
                    agent_id: Some(agent_id.clone()),
                    ..TaskPatch::default()
                },
            )
            .await
            .expect("update task by prefix");

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.agent_id, agent_id);
        assert_eq!(updated.name, "Updated MCP Core Task");

        let paused = backend
            .control_task(&prefix, TaskControlAction::Pause)
            .await
            .expect("control task by prefix");

        assert_eq!(paused.id, created.id);
        assert_eq!(paused.status, TaskStatus::Paused);
        unsafe {
            std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV);
        }
    }
}
