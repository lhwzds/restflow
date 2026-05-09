use super::*;
use crate::boundary::task::{core_patch_to_contract, core_spec_to_contract};
use crate::daemon::request_mapper::to_contract;
use crate::daemon::tool_result_mapper::to_tool_execution_result;
use ::types::DeleteWithIdResponse;
use ::types::{TaskCommandOutcome, store::TaskDeleteRequest};

fn task_storage_removed<T>() -> Result<T, String> {
    Err("legacy task storage has been removed".to_string())
}

pub(super) struct CoreBackend {
    pub(super) core: Arc<AppCore>,
    pub(super) registry: std::sync::OnceLock<::types::registry::ToolRegistry>,
}

impl CoreBackend {
    fn session_service(&self) -> crate::services::session::SessionService {
        crate::services::session::SessionService::from_storage(&self.core.storage)
    }

    fn get_registry(&self) -> Result<&::types::registry::ToolRegistry, String> {
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
        let _ = status;
        Ok(Vec::new())
    }

    async fn create_task(&self, spec: TaskSpec) -> Result<Task, String> {
        let _ = spec;
        task_storage_removed()
    }

    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task, String> {
        let _ = (id, patch);
        task_storage_removed()
    }

    async fn delete_task(
        &self,
        request: TaskDeleteRequest,
    ) -> Result<TaskCommandOutcome<DeleteWithIdResponse>, String> {
        let _ = request;
        task_storage_removed()
    }

    async fn control_task(&self, id: &str, action: TaskControlAction) -> Result<Task, String> {
        let _ = (id, action);
        task_storage_removed()
    }

    async fn get_task_progress(
        &self,
        id: &str,
        event_limit: usize,
    ) -> Result<TaskProgress, String> {
        let _ = (id, event_limit);
        task_storage_removed()
    }

    async fn send_task_message(
        &self,
        id: &str,
        message: String,
        source: TaskMessageSource,
    ) -> Result<TaskMessage, String> {
        let _ = (id, message, source);
        task_storage_removed()
    }

    async fn list_task_messages(&self, id: &str, limit: usize) -> Result<Vec<TaskMessage>, String> {
        let _ = (id, limit);
        Ok(Vec::new())
    }

    async fn list_artifacts(&self, task_id: &str) -> Result<Vec<RunArtifact>, String> {
        let _ = task_id;
        Ok(Vec::new())
    }

    async fn list_runs(&self, query: RunListQuery) -> Result<Vec<RunSummary>, String> {
        crate::services::execution_console::ExecutionConsoleService::from_storage(
            &self.core.storage,
        )
        .list_runs(&query)
        .map_err(|e| e.to_string())
    }

    async fn get_task(&self, id: &str) -> Result<Task, String> {
        let _ = id;
        task_storage_removed()
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
        if request.preview {
            return Err("Preview is no longer available for IPC task deletions.".to_string());
        }
        let result = self
            .request_typed(IpcRequest::DeleteTask {
                id: request.id,
                approval_id: request.approval_id,
            })
            .await?;
        Ok(TaskCommandOutcome::Executed { result })
    }

    async fn control_task(&self, id: &str, action: TaskControlAction) -> Result<Task, String> {
        let action = to_contract(action).map_err(|e| e.to_string())?;
        self.request_typed(IpcRequest::ControlTask {
            id: id.to_string(),
            action,
            approval_id: None,
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
        let env = crate::test_support::RestflowTestEnv::new();
        let db_path = env.db_path("mcp-core-backend.db");
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
                execution_mode: None,
                timeout_secs: None,
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
