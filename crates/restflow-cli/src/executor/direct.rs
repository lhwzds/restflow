use anyhow::{Result, bail};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

use crate::executor::{CommandExecutor, CreateTaskInput};
use crate::setup;
use restflow_ai::{
    AgentConfig, AgentExecutor, AgentState, AgentStatus, DefaultLlmClientFactory, LlmProvider,
    LlmClientFactory, ModelSpec, Role, SwappableLlm, SwitchModelTool, ToolRegistry,
};
use restflow_core::auth::{AuthManagerConfig, AuthProfileManager, AuthProvider};
use restflow_core::memory::{ChatSessionMirror, ExportResult, MemoryExporter, MessageMirror};
use restflow_core::models::{
    AgentExecuteResponse, AgentNode, ApiKeyConfig, ExecutionDetails, ExecutionStep, Provider,
    TaskEvent,
};
use restflow_core::paths;
use restflow_core::services::{
    agent as agent_service, config as config_service, secrets as secrets_service,
    skills as skills_service, tool_registry::create_tool_registry,
};
use restflow_core::storage::SystemConfig;
use restflow_core::storage::agent::StoredAgent;
use restflow_core::{
    AIModel, AppCore,
    models::{
        AgentTask, AgentTaskStatus, ChatSession, ChatSessionSummary, MemoryChunk,
        MemorySearchResult, MemoryStats, Secret, Skill,
    },
};
use restflow_storage::AuthProfileStorage;

pub struct DirectExecutor {
    core: Arc<AppCore>,
}

impl DirectExecutor {
    pub async fn connect(db_path: Option<String>) -> Result<Self> {
        let core = setup::prepare_core(db_path).await?;
        Ok(Self { core })
    }
}

#[async_trait]
impl CommandExecutor for DirectExecutor {
    fn core(&self) -> Option<Arc<AppCore>> {
        Some(self.core.clone())
    }

    async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        agent_service::list_agents(&self.core).await
    }

    async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
        agent_service::get_agent(&self.core, id).await
    }

    async fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
        agent_service::create_agent(&self.core, name, agent).await
    }

    async fn update_agent(
        &self,
        id: &str,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<StoredAgent> {
        agent_service::update_agent(&self.core, id, name, agent).await
    }

    async fn delete_agent(&self, id: &str) -> Result<()> {
        agent_service::delete_agent(&self.core, id).await
    }

    async fn execute_agent(
        &self,
        id: &str,
        input: String,
        session_id: Option<String>,
    ) -> Result<AgentExecuteResponse> {
        let agent = agent_service::get_agent(&self.core, id).await?;
        let response = run_agent_with_executor(
            &self.core,
            &agent.agent,
            &input,
            Some(&self.core.storage.secrets),
            self.core.storage.skills.clone(),
            self.core.storage.memory.clone(),
            self.core.storage.chat_sessions.clone(),
            self.core.storage.shared_space.clone(),
        )
        .await?;

        if let Some(ref session_id) = session_id {
            let mirror = ChatSessionMirror::new(Arc::new(self.core.storage.chat_sessions.clone()));

            if let Err(e) = mirror.mirror_user(session_id, &input).await {
                warn!(error = %e, "Failed to mirror user message");
            }

            let tokens = response
                .execution_details
                .as_ref()
                .map(|details| details.total_tokens);

            if let Err(e) = mirror
                .mirror_assistant(session_id, &response.response, tokens)
                .await
            {
                warn!(error = %e, "Failed to mirror assistant message");
            }
        }

        Ok(response)
    }

    async fn list_skills(&self) -> Result<Vec<Skill>> {
        skills_service::list_skills(&self.core).await
    }

    async fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
        skills_service::get_skill(&self.core, id).await
    }

    async fn create_skill(&self, skill: Skill) -> Result<()> {
        skills_service::create_skill(&self.core, skill).await
    }

    async fn update_skill(&self, id: &str, skill: Skill) -> Result<()> {
        skills_service::update_skill(&self.core, id, &skill).await
    }

    async fn delete_skill(&self, id: &str) -> Result<()> {
        skills_service::delete_skill(&self.core, id).await
    }

    async fn list_tasks(&self) -> Result<Vec<AgentTask>> {
        self.core.storage.agent_tasks.list_tasks()
    }

    async fn list_tasks_by_status(&self, status: AgentTaskStatus) -> Result<Vec<AgentTask>> {
        self.core.storage.agent_tasks.list_tasks_by_status(status)
    }

    async fn get_task(&self, id: &str) -> Result<Option<AgentTask>> {
        self.core.storage.agent_tasks.get_task(id)
    }

    async fn create_task(&self, input: CreateTaskInput) -> Result<AgentTask> {
        let mut task = self.core.storage.agent_tasks.create_task(
            input.name,
            input.agent_id,
            input.schedule,
        )?;

        if let Some(text) = input.input {
            task.input = Some(text);
            self.core.storage.agent_tasks.update_task(&task)?;
        }

        Ok(task)
    }

    async fn pause_task(&self, id: &str) -> Result<AgentTask> {
        self.core.storage.agent_tasks.pause_task(id)
    }

    async fn resume_task(&self, id: &str) -> Result<AgentTask> {
        self.core.storage.agent_tasks.resume_task(id)
    }

    async fn delete_task(&self, id: &str) -> Result<bool> {
        self.core.storage.agent_tasks.delete_task(id)
    }

    async fn get_task_history(&self, id: &str) -> Result<Vec<TaskEvent>> {
        self.core.storage.agent_tasks.list_events_for_task(id)
    }

    async fn search_memory(
        &self,
        query: String,
        agent_id: Option<String>,
        _limit: Option<u32>,
    ) -> Result<MemorySearchResult> {
        let agent_id = resolve_agent_id(&self.core, agent_id).await?;
        let search =
            restflow_core::models::memory::MemorySearchQuery::new(agent_id).with_query(query);
        let results = self.core.storage.memory.search(&search)?;
        Ok(results)
    }

    async fn list_memory(
        &self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        match (agent_id, tag) {
            (Some(agent_id), Some(tag)) => Ok(self
                .core
                .storage
                .memory
                .list_chunks(&agent_id)?
                .into_iter()
                .filter(|chunk| chunk.tags.iter().any(|value| value == &tag))
                .collect()),
            (Some(agent_id), None) => self.core.storage.memory.list_chunks(&agent_id),
            (None, Some(tag)) => self.core.storage.memory.list_chunks_by_tag(&tag),
            (None, None) => {
                let agent_id = resolve_agent_id(&self.core, None).await?;
                self.core.storage.memory.list_chunks(&agent_id)
            }
        }
    }

    async fn clear_memory(&self, agent_id: Option<String>) -> Result<u32> {
        let agent_id = resolve_agent_id(&self.core, agent_id).await?;
        self.core.storage.memory.delete_chunks_for_agent(&agent_id)
    }

    async fn get_memory_stats(&self, agent_id: Option<String>) -> Result<MemoryStats> {
        let agent_id = resolve_agent_id(&self.core, agent_id).await?;
        self.core.storage.memory.get_stats(&agent_id)
    }

    async fn export_memory(&self, agent_id: Option<String>) -> Result<ExportResult> {
        let agent_id = resolve_agent_id(&self.core, agent_id).await?;
        let exporter = MemoryExporter::new(self.core.storage.memory.clone());
        exporter.export_agent(&agent_id)
    }

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        self.core.storage.chat_sessions.list_summaries()
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession> {
        self.core
            .storage
            .chat_sessions
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
    }

    async fn create_session(&self, agent_id: String, model: String) -> Result<ChatSession> {
        let session = ChatSession::new(agent_id, model);
        self.core.storage.chat_sessions.create(&session)?;
        Ok(session)
    }

    async fn delete_session(&self, id: &str) -> Result<bool> {
        self.core.storage.chat_sessions.delete(id)
    }

    async fn search_sessions(&self, query: String) -> Result<Vec<ChatSessionSummary>> {
        let query = query.to_lowercase();
        let sessions = self.core.storage.chat_sessions.list()?;
        let matches: Vec<ChatSessionSummary> = sessions
            .into_iter()
            .filter(|session| {
                session.name.to_lowercase().contains(&query)
                    || session
                        .messages
                        .iter()
                        .any(|message| message.content.to_lowercase().contains(&query))
            })
            .map(|session| ChatSessionSummary::from(&session))
            .collect();
        Ok(matches)
    }

    async fn list_secrets(&self) -> Result<Vec<Secret>> {
        secrets_service::list_secrets(&self.core).await
    }

    async fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        secrets_service::set_secret(&self.core, key, value, description).await
    }

    async fn delete_secret(&self, key: &str) -> Result<()> {
        secrets_service::delete_secret(&self.core, key).await
    }

    async fn has_secret(&self, key: &str) -> Result<bool> {
        Ok(secrets_service::get_secret(&self.core, key)
            .await?
            .is_some())
    }

    async fn get_config(&self) -> Result<SystemConfig> {
        config_service::get_config(&self.core).await
    }

    async fn set_config(&self, config: SystemConfig) -> Result<()> {
        config_service::update_config(&self.core, config).await
    }
}

async fn resolve_agent_id(core: &Arc<AppCore>, agent_id: Option<String>) -> Result<String> {
    if let Some(agent_id) = agent_id {
        return Ok(agent_id);
    }

    let agents = agent_service::list_agents(core).await?;
    if agents.is_empty() {
        bail!("No agents available");
    }

    Ok(agents[0].id.clone())
}

async fn resolve_api_key(
    api_key_config: Option<&ApiKeyConfig>,
    secret_storage: Option<&restflow_core::storage::SecretStorage>,
    provider: Provider,
    core: &Arc<AppCore>,
) -> Result<String> {
    if let Some(config) = api_key_config {
        match config {
            ApiKeyConfig::Direct(key) => {
                if !key.is_empty() {
                    return Ok(key.clone());
                }
            }
            ApiKeyConfig::Secret(secret_name) => {
                if let Some(storage) = secret_storage {
                    return storage
                        .get_secret(secret_name)?
                        .ok_or_else(|| anyhow::anyhow!("Secret '{}' not found", secret_name));
                }
                bail!("Secret storage not available");
            }
        }
    }

    if let Some(key) = resolve_api_key_from_profiles(provider, core).await? {
        return Ok(key);
    }

    bail!("No API key configured");
}

async fn resolve_api_key_from_profiles(
    provider: Provider,
    core: &Arc<AppCore>,
) -> Result<Option<String>> {
    let config = AuthManagerConfig::default();
    let secrets = Arc::new(core.storage.secrets.clone());
    let storage = AuthProfileStorage::new(core.storage.get_db())?;
    let manager = AuthProfileManager::with_storage(config, secrets, Some(storage));

    let old_json = paths::ensure_restflow_dir()?.join("auth_profiles.json");
    if let Err(e) = manager.migrate_from_json(&old_json).await {
        warn!(error = %e, "Failed to migrate auth profiles from JSON");
    }
    manager.initialize().await?;

    let selection = match provider {
        Provider::Anthropic => {
            if let Some(selection) = manager.select_profile(AuthProvider::Anthropic).await {
                Some(selection)
            } else {
                manager.select_profile(AuthProvider::ClaudeCode).await
            }
        }
        Provider::OpenAI => manager.select_profile(AuthProvider::OpenAI).await,
        Provider::DeepSeek => None,
    };

    match selection {
        Some(sel) => Ok(Some(sel.profile.get_api_key(manager.resolver())?)),
        None => Ok(None),
    }
}

fn build_model_specs() -> Vec<ModelSpec> {
    let mut specs = Vec::new();

    for model in AIModel::all() {
        let provider = to_llm_provider(model.provider());
        let spec = if model.is_codex_cli() {
            ModelSpec::codex(model.as_serialized_str(), model.as_str())
        } else {
            ModelSpec::new(model.as_serialized_str(), provider, model.as_str())
        };
        specs.push(spec);

        if model.is_claude_code() {
            specs.push(ModelSpec::new(model.as_str(), provider, model.as_str()));
        }
    }

    for codex_model in [
        "gpt-5.3-codex",
        "gpt-5.2-codex",
        "gpt-5.1-codex-max",
        "gpt-5.1-codex",
        "gpt-5-codex",
    ] {
        specs.push(ModelSpec::codex(codex_model, codex_model));
    }

    specs
}

fn to_llm_provider(provider: Provider) -> LlmProvider {
    match provider {
        Provider::OpenAI => LlmProvider::OpenAI,
        Provider::Anthropic => LlmProvider::Anthropic,
        Provider::DeepSeek => LlmProvider::DeepSeek,
    }
}

async fn build_api_keys(
    agent_node: &AgentNode,
    secret_storage: Option<&restflow_core::storage::SecretStorage>,
    core: &Arc<AppCore>,
    primary_provider: Provider,
) -> HashMap<LlmProvider, String> {
    let mut keys = HashMap::new();

    for provider in [Provider::OpenAI, Provider::Anthropic, Provider::DeepSeek] {
        let api_key_config = if provider == primary_provider {
            agent_node.api_key_config.as_ref()
        } else {
            None
        };

        if let Ok(key) =
            resolve_api_key(api_key_config, secret_storage, provider, core).await
        {
            keys.insert(to_llm_provider(provider), key);
        }
    }

    keys
}

#[allow(clippy::too_many_arguments)]
async fn run_agent_with_executor(
    core: &Arc<AppCore>,
    agent_node: &AgentNode,
    input: &str,
    secret_storage: Option<&restflow_core::storage::SecretStorage>,
    skill_storage: restflow_core::storage::skill::SkillStorage,
    memory_storage: restflow_core::storage::memory::MemoryStorage,
    chat_storage: restflow_core::storage::chat_session::ChatSessionStorage,
    shared_space_storage: restflow_core::storage::SharedSpaceStorage,
) -> Result<AgentExecuteResponse> {
    let model = agent_node.require_model().map_err(|e| anyhow::anyhow!(e))?;

    let api_keys = build_api_keys(agent_node, secret_storage, core, model.provider()).await;
    let factory = Arc::new(DefaultLlmClientFactory::new(api_keys, build_model_specs()));

    let api_key = if model.is_codex_cli() {
        None
    } else {
        Some(
            resolve_api_key(
                agent_node.api_key_config.as_ref(),
                secret_storage,
                model.provider(),
                core,
            )
            .await?,
        )
    };

    let llm_client = factory.create_client(model.as_serialized_str(), api_key.as_deref())?;
    let swappable = Arc::new(SwappableLlm::new(llm_client));

    let full_registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        shared_space_storage,
        None,
    );

    let mut tools = if let Some(ref tool_names) = agent_node.tools {
        if tool_names.is_empty() {
            ToolRegistry::new()
        } else {
            let mut filtered_registry = ToolRegistry::new();
            for name in tool_names {
                if let Some(tool) = full_registry.get(name) {
                    filtered_registry.register_arc(tool);
                } else {
                    warn!(tool_name = %name, "Configured tool not found in registry, skipping");
                }
            }
            filtered_registry
        }
    } else {
        ToolRegistry::new()
    };

    if agent_node
        .tools
        .as_deref()
        .map(|names| names.iter().any(|name| name == "switch_model"))
        .unwrap_or(false)
    {
        tools.register(SwitchModelTool::new(swappable.clone(), factory.clone()));
    }

    let tools = Arc::new(tools);

    let mut config = AgentConfig::new(input);

    if let Some(ref prompt) = agent_node.prompt {
        config = config.with_system_prompt(prompt);
    }

    if model.supports_temperature()
        && let Some(temp) = agent_node.temperature
    {
        config = config.with_temperature(temp as f32);
    }

    let executor = AgentExecutor::new(swappable, tools);
    let result = executor.run(config).await?;

    let response = result.answer.unwrap_or_else(|| {
        if let Some(ref err) = result.error {
            format!("Error: {}", err)
        } else {
            "No response generated".to_string()
        }
    });

    let execution_details = ExecutionDetails {
        iterations: result.iterations,
        total_tokens: result.total_tokens,
        steps: convert_to_execution_steps(&result.state),
        status: status_to_string(&result.state.status),
    };

    Ok(AgentExecuteResponse {
        response,
        execution_details: Some(execution_details),
    })
}

fn convert_to_execution_steps(state: &AgentState) -> Vec<ExecutionStep> {
    state
        .messages
        .iter()
        .map(|msg| {
            let step_type = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => {
                    if msg.tool_calls.is_some() {
                        "tool_call"
                    } else {
                        "assistant"
                    }
                }
                Role::Tool => "tool_result",
            };

            let tool_calls = msg.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|tc| restflow_core::models::ToolCallInfo {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    })
                    .collect()
            });

            ExecutionStep {
                step_type: step_type.to_string(),
                content: msg.content.clone(),
                tool_calls,
            }
        })
        .collect()
}

fn status_to_string(status: &AgentStatus) -> String {
    match status {
        AgentStatus::Running => "running".to_string(),
        AgentStatus::Completed => "completed".to_string(),
        AgentStatus::Failed { error } => format!("failed: {}", error),
        AgentStatus::MaxIterations => "max_iterations".to_string(),
    }
}
