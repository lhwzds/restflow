//! MCP server implementation for RestFlow
//!
//! This module provides an MCP server that exposes RestFlow's functionality
//! to AI assistants like Claude Code.

use crate::AppCore;
use crate::daemon::{IpcClient, IpcRequest, IpcResponse};
use crate::models::{
    AIModel, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentPatch,
    BackgroundAgentSchedule, BackgroundAgentSpec, BackgroundAgentStatus, BackgroundMessage,
    BackgroundMessageSource, BackgroundProgress, ChatSession, ChatSessionSummary, Hook, HookAction,
    HookEvent, HookFilter, MemoryChunk, MemoryConfig, MemoryScope, MemorySearchQuery,
    MemorySearchResult, MemorySource, MemoryStats, Provider, SearchMode, Skill,
};
use crate::services::tool_registry::create_tool_registry;
use crate::storage::SecretStorage;
use crate::storage::agent::StoredAgent;
use restflow_ai::llm::{
    CodexClient, DefaultLlmClientFactory, LlmClient, LlmProvider, SwappableLlm,
};
use restflow_ai::tools::{SwitchModelTool, Tool as RuntimeTool};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::tool::schema_for_type,
    model::{
        CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
    },
    schemars::{self, JsonSchema},
    service::{RequestContext, RoleServer},
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::io::{stdin, stdout};
use tokio::sync::Mutex;

/// RestFlow MCP Server
///
/// Exposes skills, agents, and workflow functionality via MCP protocol.
#[derive(Clone)]
pub struct RestFlowMcpServer {
    backend: Arc<dyn McpBackend>,
    switch_model_tool: SwitchModelTool,
}

#[derive(Debug, Clone)]
pub struct RuntimeToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct RuntimeToolResult {
    pub success: bool,
    pub result: Value,
    pub error: Option<String>,
}

#[async_trait::async_trait]
pub trait McpBackend: Send + Sync {
    async fn list_skills(&self) -> Result<Vec<Skill>, String>;
    async fn get_skill(&self, id: &str) -> Result<Option<Skill>, String>;
    async fn create_skill(&self, skill: Skill) -> Result<(), String>;
    async fn update_skill(&self, skill: Skill) -> Result<(), String>;
    async fn delete_skill(&self, id: &str) -> Result<(), String>;

    async fn list_agents(&self) -> Result<Vec<StoredAgent>, String>;
    async fn get_agent(&self, id: &str) -> Result<StoredAgent, String>;

    async fn search_memory(&self, query: MemorySearchQuery) -> Result<MemorySearchResult, String>;
    async fn store_memory(&self, chunk: MemoryChunk) -> Result<String, String>;
    async fn get_memory_stats(&self, agent_id: &str) -> Result<MemoryStats, String>;

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>, String>;
    async fn list_sessions_by_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<ChatSessionSummary>, String>;
    async fn get_session(&self, id: &str) -> Result<ChatSession, String>;

    async fn list_tasks(
        &self,
        status: Option<BackgroundAgentStatus>,
    ) -> Result<Vec<BackgroundAgent>, String>;
    async fn create_background_agent(
        &self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent, String>;
    async fn update_background_agent(
        &self,
        id: &str,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent, String>;
    async fn delete_background_agent(&self, id: &str) -> Result<bool, String>;
    async fn control_background_agent(
        &self,
        id: &str,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent, String>;
    async fn get_background_agent_progress(
        &self,
        id: &str,
        event_limit: usize,
    ) -> Result<BackgroundProgress, String>;
    async fn send_background_agent_message(
        &self,
        id: &str,
        message: String,
        source: BackgroundMessageSource,
    ) -> Result<BackgroundMessage, String>;
    async fn list_background_agent_messages(
        &self,
        id: &str,
        limit: usize,
    ) -> Result<Vec<BackgroundMessage>, String>;

    async fn list_hooks(&self) -> Result<Vec<Hook>, String>;
    async fn create_hook(&self, hook: Hook) -> Result<Hook, String>;
    async fn update_hook(&self, id: &str, hook: Hook) -> Result<Hook, String>;
    async fn delete_hook(&self, id: &str) -> Result<bool, String>;

    async fn list_runtime_tools(&self) -> Result<Vec<RuntimeToolDefinition>, String>;
    async fn execute_runtime_tool(
        &self,
        name: &str,
        input: Value,
    ) -> Result<RuntimeToolResult, String>;
}

fn create_runtime_tool_registry_for_core(core: &Arc<AppCore>) -> restflow_ai::tools::ToolRegistry {
    create_tool_registry(
        core.storage.skills.clone(),
        core.storage.memory.clone(),
        core.storage.chat_sessions.clone(),
        core.storage.shared_space.clone(),
        core.storage.secrets.clone(),
        core.storage.config.clone(),
        core.storage.agents.clone(),
        core.storage.background_agents.clone(),
        core.storage.triggers.clone(),
        core.storage.terminal_sessions.clone(),
        None,
        None,
    )
}

fn build_api_keys(secret_storage: Option<&SecretStorage>) -> HashMap<LlmProvider, String> {
    let mut keys = HashMap::new();
    for provider in Provider::all() {
        let env_name = provider.api_key_env();
        if let Some(storage) = secret_storage
            && let Ok(Some(value)) = storage.get_secret(env_name)
            && !value.trim().is_empty()
        {
            keys.insert(provider.as_llm_provider(), value);
            continue;
        }

        if let Ok(value) = std::env::var(env_name)
            && !value.trim().is_empty()
        {
            keys.insert(provider.as_llm_provider(), value);
        }
    }
    keys
}

fn build_switch_model_tool(secret_storage: Option<&SecretStorage>) -> SwitchModelTool {
    let api_keys = build_api_keys(secret_storage);
    let factory = Arc::new(DefaultLlmClientFactory::new(
        api_keys,
        AIModel::build_model_specs(),
    ));
    let initial_client: Arc<dyn LlmClient> = Arc::new(CodexClient::new());
    let swappable = Arc::new(SwappableLlm::new(initial_client));
    SwitchModelTool::new(swappable, factory)
}

struct CoreBackend {
    core: Arc<AppCore>,
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
        self.core
            .storage
            .chat_sessions
            .list_summaries()
            .map_err(|e| e.to_string())
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
        Ok(sessions.iter().map(ChatSessionSummary::from).collect())
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession, String> {
        self.core
            .storage
            .chat_sessions
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session not found: {}", id))
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
        let registry = create_runtime_tool_registry_for_core(&self.core);
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
        let registry = create_runtime_tool_registry_for_core(&self.core);
        let output = registry
            .execute_safe(name, input)
            .await
            .map_err(|e| e.to_string())?;
        Ok(RuntimeToolResult {
            success: output.success,
            result: output.result,
            error: output.error,
        })
    }
}

struct IpcBackend {
    client: Arc<Mutex<IpcClient>>,
}

impl IpcBackend {
    async fn request_typed<T: DeserializeOwned>(&self, req: IpcRequest) -> Result<T, String> {
        let mut client = self.client.lock().await;
        match client.request(req).await.map_err(|e| e.to_string())? {
            IpcResponse::Success(value) => serde_json::from_value(value).map_err(|e| e.to_string()),
            IpcResponse::Error { code, message } => Err(format!("IPC error {}: {}", code, message)),
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
        })
    }
}

impl RestFlowMcpServer {
    /// Create a new MCP server with the given AppCore
    pub fn new(core: Arc<AppCore>) -> Self {
        Self {
            switch_model_tool: build_switch_model_tool(Some(&core.storage.secrets)),
            backend: Arc::new(CoreBackend { core }),
        }
    }

    /// Create a new MCP server using daemon IPC
    pub fn with_ipc(client: IpcClient) -> Self {
        Self {
            switch_model_tool: build_switch_model_tool(None),
            backend: Arc::new(IpcBackend {
                client: Arc::new(Mutex::new(client)),
            }),
        }
    }

    /// Create a new MCP server with a custom backend
    pub fn with_backend(backend: Arc<dyn McpBackend>) -> Self {
        Self {
            switch_model_tool: build_switch_model_tool(None),
            backend,
        }
    }

    /// Run the MCP server using stdio transport
    pub async fn run(self) -> anyhow::Result<()> {
        tracing::info!("Starting RestFlow MCP server...");
        let server = self.serve(stdio()).await?;
        tracing::info!("MCP server initialized, waiting for requests...");
        server.waiting().await?;
        Ok(())
    }
}

/// Create stdio transport for MCP communication
fn stdio() -> (tokio::io::Stdin, tokio::io::Stdout) {
    (stdin(), stdout())
}

// ============================================================================
// Tool Parameter Types
// ============================================================================

/// Parameters for get_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetSkillParams {
    /// The ID of the skill to retrieve
    pub id: String,
}

/// Parameters for create_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateSkillParams {
    /// Display name of the skill
    pub name: String,
    /// Optional description of what the skill does
    #[serde(default)]
    pub description: Option<String>,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// The markdown content of the skill (instructions for the AI)
    pub content: String,
}

/// Parameters for update_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateSkillParams {
    /// The ID of the skill to update
    pub id: String,
    /// New display name (optional)
    #[serde(default)]
    pub name: Option<String>,
    /// New description (optional)
    #[serde(default)]
    pub description: Option<String>,
    /// New tags (optional)
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// New content (optional)
    #[serde(default)]
    pub content: Option<String>,
}

/// Parameters for delete_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteSkillParams {
    /// The ID of the skill to delete
    pub id: String,
}

/// Parameters for get_agent tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetAgentParams {
    /// The ID of the agent to retrieve
    pub id: String,
}

/// Parameters for memory_search tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemorySearchParams {
    /// Search query string
    pub query: String,
    /// Agent ID to scope the search
    pub agent_id: String,
    /// Maximum number of results to return
    #[serde(default = "default_memory_limit")]
    pub limit: u32,
}

/// Parameters for memory_store tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryStoreParams {
    /// Agent ID to store memory under
    pub agent_id: String,
    /// Memory content to store
    pub content: String,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Parameters for memory_stats tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryStatsParams {
    /// Agent ID to fetch stats for
    pub agent_id: String,
}

/// Parameters for skill_execute tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SkillExecuteParams {
    /// Skill ID to execute
    pub skill_id: String,
    /// Optional input provided to the skill
    #[serde(default)]
    pub input: Option<String>,
}

/// Parameters for chat_session_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChatSessionListParams {
    /// Optional agent ID filter
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Maximum number of sessions to return
    #[serde(default = "default_session_limit")]
    pub limit: u32,
}

/// Parameters for chat_session_get tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChatSessionGetParams {
    /// Session ID to retrieve
    pub session_id: String,
}

/// Parameters for manage_background_agents tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ManageBackgroundAgentsParams {
    /// Operation to perform
    pub operation: String,
    /// Task/background agent ID
    #[serde(default)]
    pub id: Option<String>,
    /// Task name
    #[serde(default)]
    pub name: Option<String>,
    /// Agent ID for execution
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Optional task input
    #[serde(default)]
    pub input: Option<String>,
    /// Optional task input template
    #[serde(default)]
    pub input_template: Option<String>,
    /// Optional schedule payload
    #[serde(default)]
    pub schedule: Option<Value>,
    /// Optional notification payload
    #[serde(default)]
    pub notification: Option<Value>,
    /// Optional execution mode payload
    #[serde(default)]
    pub execution_mode: Option<Value>,
    /// Optional memory payload
    #[serde(default)]
    pub memory: Option<Value>,
    /// Optional memory scope override
    #[serde(default)]
    pub memory_scope: Option<String>,
    /// Optional list status filter
    #[serde(default)]
    pub status: Option<String>,
    /// Optional control action
    #[serde(default)]
    pub action: Option<String>,
    /// Optional progress event limit
    #[serde(default)]
    pub event_limit: Option<usize>,
    /// Optional message body
    #[serde(default)]
    pub message: Option<String>,
    /// Optional message source
    #[serde(default)]
    pub source: Option<String>,
    /// Optional message list limit
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Parameters for manage_hooks tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ManageHooksParams {
    /// Operation to perform: list, create, update, delete
    pub operation: String,
    /// Hook ID (required for update/delete)
    #[serde(default)]
    pub id: Option<String>,
    /// Hook name (required for create)
    #[serde(default)]
    pub name: Option<String>,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Hook event trigger (required for create): task_started, task_completed, task_failed, task_cancelled, tool_executed, approval_required
    #[serde(default)]
    pub event: Option<String>,
    /// Hook action payload (required for create)
    #[serde(default)]
    pub action: Option<Value>,
    /// Optional filter to limit when the hook fires
    #[serde(default)]
    pub filter: Option<Value>,
    /// Whether the hook is enabled (default: true)
    #[serde(default)]
    pub enabled: Option<bool>,
}

// ============================================================================
// Response Types
// ============================================================================

/// Skill summary for list_skills response
#[derive(Debug, Serialize, Deserialize)]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Agent summary for list_agents response
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub model: String,
}

// ============================================================================
// Empty params for parameterless tools
// ============================================================================

/// Empty parameters (for tools with no parameters)
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EmptyParams {}

fn default_memory_limit() -> u32 {
    10
}

fn default_session_limit() -> u32 {
    20
}

// ============================================================================
// Tool Implementations
// ============================================================================

impl RestFlowMcpServer {
    fn required_string(value: Option<String>, field: &str) -> Result<String, String> {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("Missing required field: {}", field))
    }

    fn parse_task_status(value: Option<String>) -> Result<Option<BackgroundAgentStatus>, String> {
        match value.map(|s| s.trim().to_lowercase()) {
            None => Ok(None),
            Some(s) if s.is_empty() => Ok(None),
            Some(s) if s == "active" => Ok(Some(BackgroundAgentStatus::Active)),
            Some(s) if s == "paused" => Ok(Some(BackgroundAgentStatus::Paused)),
            Some(s) if s == "running" => Ok(Some(BackgroundAgentStatus::Running)),
            Some(s) if s == "completed" => Ok(Some(BackgroundAgentStatus::Completed)),
            Some(s) if s == "failed" => Ok(Some(BackgroundAgentStatus::Failed)),
            Some(s) => Err(format!("Unknown status: {}", s)),
        }
    }

    fn parse_control_action(value: Option<String>) -> Result<BackgroundAgentControlAction, String> {
        match value.map(|s| s.trim().to_lowercase()) {
            Some(s) if s == "start" => Ok(BackgroundAgentControlAction::Start),
            Some(s) if s == "pause" => Ok(BackgroundAgentControlAction::Pause),
            Some(s) if s == "resume" => Ok(BackgroundAgentControlAction::Resume),
            Some(s) if s == "stop" => Ok(BackgroundAgentControlAction::Stop),
            Some(s) if s == "run_now" || s == "run-now" || s == "runnow" => {
                Ok(BackgroundAgentControlAction::RunNow)
            }
            Some(s) => Err(format!("Unknown control action: {}", s)),
            None => Err("Missing required field: action".to_string()),
        }
    }

    fn parse_message_source(value: Option<String>) -> Result<BackgroundMessageSource, String> {
        match value.map(|s| s.trim().to_lowercase()) {
            None => Ok(BackgroundMessageSource::User),
            Some(s) if s.is_empty() => Ok(BackgroundMessageSource::User),
            Some(s) if s == "user" => Ok(BackgroundMessageSource::User),
            Some(s) if s == "agent" => Ok(BackgroundMessageSource::Agent),
            Some(s) if s == "system" => Ok(BackgroundMessageSource::System),
            Some(s) => Err(format!("Unknown message source: {}", s)),
        }
    }

    fn parse_memory_scope(value: Option<String>) -> Result<Option<MemoryScope>, String> {
        match value.map(|s| s.trim().to_lowercase()) {
            None => Ok(None),
            Some(s) if s.is_empty() => Ok(None),
            Some(s) if s == "shared_agent" => Ok(Some(MemoryScope::SharedAgent)),
            Some(s) if s == "per_background_agent" => Ok(Some(MemoryScope::PerBackgroundAgent)),
            Some(s) => Err(format!("Unknown memory_scope: {}", s)),
        }
    }

    fn parse_optional_value<T: DeserializeOwned>(
        field: &str,
        value: Option<Value>,
    ) -> Result<Option<T>, String> {
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| format!("Invalid {}: {}", field, e)),
            None => Ok(None),
        }
    }

    fn merge_memory_scope(
        memory: Option<MemoryConfig>,
        memory_scope: Option<String>,
    ) -> Result<Option<MemoryConfig>, String> {
        let parsed_scope = Self::parse_memory_scope(memory_scope)?;
        match (memory, parsed_scope) {
            (Some(mut memory), Some(scope)) => {
                memory.memory_scope = scope;
                Ok(Some(memory))
            }
            (Some(memory), None) => Ok(Some(memory)),
            (None, Some(scope)) => Ok(Some(MemoryConfig {
                memory_scope: scope,
                ..MemoryConfig::default()
            })),
            (None, None) => Ok(None),
        }
    }

    fn runtime_alias_target(name: &str) -> Option<&'static str> {
        match name {
            "http" => Some("http_request"),
            "email" => Some("send_email"),
            "telegram" => Some("telegram_send"),
            "use_skill" => Some("skill"),
            _ => None,
        }
    }

    fn convert_use_skill_input(input: Value) -> Value {
        let Value::Object(mut map) = input else {
            return serde_json::json!({ "action": "list" });
        };

        if map.contains_key("action") {
            return Value::Object(map);
        }

        if map.get("list").and_then(|v| v.as_bool()).unwrap_or(false) {
            return serde_json::json!({ "action": "list" });
        }

        if let Some(skill_id) = map.remove("skill_id").or_else(|| map.remove("id")) {
            return serde_json::json!({
                "action": "read",
                "id": skill_id
            });
        }

        serde_json::json!({ "action": "list" })
    }

    fn session_scoped_runtime_tools() -> Vec<RuntimeToolDefinition> {
        vec![
            RuntimeToolDefinition {
                name: "spawn_agent".to_string(),
                description: "Spawn a specialized agent to work on a task in parallel. Requires active main-agent runtime context.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent": {
                            "type": "string",
                            "enum": ["researcher", "coder", "reviewer", "writer", "analyst"],
                            "description": "The specialized agent to spawn"
                        },
                        "task": {
                            "type": "string",
                            "description": "Detailed task description for the agent"
                        },
                        "wait": {
                            "type": "boolean",
                            "default": false,
                            "description": "If true, wait for completion. If false, run in background."
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "default": 300,
                            "description": "Timeout in seconds"
                        }
                    },
                    "required": ["agent", "task"]
                }),
            },
            RuntimeToolDefinition {
                name: "wait_agents".to_string(),
                description: "Wait for one or more sub-agents to finish. Requires active main-agent runtime context.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of sub-agent task IDs to wait for"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "default": 300,
                            "description": "Timeout in seconds"
                        }
                    },
                    "required": ["task_ids"]
                }),
            },
            RuntimeToolDefinition {
                name: "switch_model".to_string(),
                description: "Switch the active LLM model for the current MCP server session.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "description": "Both 'provider' and 'model' are required.",
                    "properties": {
                        "provider": {
                            "type": "string",
                            "description": "Provider selector (e.g. openai, anthropic, claude-code, openai-codex, gemini-cli)"
                        },
                        "model": {
                            "type": "string",
                            "description": "Model name to switch to. Supports provider-qualified format like openai-codex:gpt-5.3-codex."
                        },
                        "reason": {
                            "type": "string",
                            "description": "Optional reason for switching models"
                        }
                    },
                    "required": ["provider", "model"]
                }),
            },
        ]
    }

    async fn handle_list_skills(&self) -> Result<String, String> {
        let skills = self
            .backend
            .list_skills()
            .await
            .map_err(|e| format!("Failed to list skills: {}", e))?;

        let summaries: Vec<SkillSummary> = skills
            .into_iter()
            .map(|s| SkillSummary {
                id: s.id,
                name: s.name,
                description: s.description,
                tags: s.tags,
            })
            .collect();

        serde_json::to_string_pretty(&summaries)
            .map_err(|e| format!("Failed to serialize skills: {}", e))
    }

    async fn handle_get_skill(&self, params: GetSkillParams) -> Result<String, String> {
        let skill = self
            .backend
            .get_skill(&params.id)
            .await
            .map_err(|e| format!("Failed to get skill: {}", e))?
            .ok_or_else(|| format!("Skill not found: {}", params.id))?;

        serde_json::to_string_pretty(&skill)
            .map_err(|e| format!("Failed to serialize skill: {}", e))
    }

    async fn handle_create_skill(&self, params: CreateSkillParams) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let skill = crate::models::Skill::new(
            id.clone(),
            params.name,
            params.description,
            params.tags,
            params.content,
        );

        self.backend
            .create_skill(skill)
            .await
            .map_err(|e| format!("Failed to create skill: {}", e))?;

        Ok(format!("Skill created successfully with ID: {}", id))
    }

    async fn handle_update_skill(&self, params: UpdateSkillParams) -> Result<String, String> {
        let mut skill = self
            .backend
            .get_skill(&params.id)
            .await
            .map_err(|e| format!("Failed to get skill: {}", e))?
            .ok_or_else(|| format!("Skill not found: {}", params.id))?;

        // Update fields
        skill.update(
            params.name,
            params.description.map(Some),
            params.tags.map(Some),
            params.content,
        );

        self.backend
            .update_skill(skill)
            .await
            .map_err(|e| format!("Failed to update skill: {}", e))?;

        Ok(format!("Skill {} updated successfully", params.id))
    }

    async fn handle_delete_skill(&self, params: DeleteSkillParams) -> Result<String, String> {
        self.backend
            .delete_skill(&params.id)
            .await
            .map_err(|e| format!("Failed to delete skill: {}", e))?;

        Ok(format!("Skill {} deleted successfully", params.id))
    }

    async fn handle_list_agents(&self) -> Result<String, String> {
        let agents = self
            .backend
            .list_agents()
            .await
            .map_err(|e| format!("Failed to list agents: {}", e))?;

        let summaries: Vec<AgentSummary> = agents
            .into_iter()
            .map(|a| AgentSummary {
                id: a.id,
                name: a.name,
                // Use serde_json to get the proper serialized model name
                model: serde_json::to_value(a.agent.model)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("{:?}", a.agent.model)),
            })
            .collect();

        serde_json::to_string_pretty(&summaries)
            .map_err(|e| format!("Failed to serialize agents: {}", e))
    }

    async fn handle_get_agent(&self, params: GetAgentParams) -> Result<String, String> {
        let agent = self
            .backend
            .get_agent(&params.id)
            .await
            .map_err(|e| format!("Failed to get agent: {}", e))?;

        serde_json::to_string_pretty(&agent)
            .map_err(|e| format!("Failed to serialize agent: {}", e))
    }

    async fn handle_memory_search(&self, params: MemorySearchParams) -> Result<String, String> {
        let query = MemorySearchQuery::new(params.agent_id)
            .with_query(params.query)
            .with_mode(SearchMode::Keyword)
            .paginate(params.limit, 0);

        let results = self
            .backend
            .search_memory(query)
            .await
            .map_err(|e| format!("Failed to search memory: {}", e))?;

        serde_json::to_string_pretty(&results)
            .map_err(|e| format!("Failed to serialize search results: {}", e))
    }

    async fn handle_memory_store(&self, params: MemoryStoreParams) -> Result<String, String> {
        let mut chunk =
            MemoryChunk::new(params.agent_id, params.content).with_source(MemorySource::ManualNote);

        if !params.tags.is_empty() {
            chunk = chunk.with_tags(params.tags);
        }

        let id = self
            .backend
            .store_memory(chunk)
            .await
            .map_err(|e| format!("Failed to store memory: {}", e))?;

        Ok(format!("Stored memory chunk: {}", id))
    }

    async fn handle_memory_stats(&self, params: MemoryStatsParams) -> Result<String, String> {
        let stats = self
            .backend
            .get_memory_stats(&params.agent_id)
            .await
            .map_err(|e| format!("Failed to load memory stats: {}", e))?;

        serde_json::to_string_pretty(&stats)
            .map_err(|e| format!("Failed to serialize memory stats: {}", e))
    }

    async fn handle_skill_execute(&self, params: SkillExecuteParams) -> Result<String, String> {
        let skill = self
            .backend
            .get_skill(&params.skill_id)
            .await
            .map_err(|e| format!("Failed to get skill: {}", e))?
            .ok_or_else(|| format!("Skill not found: {}", params.skill_id))?;

        let response = serde_json::json!({
            "skill_id": skill.id,
            "name": skill.name,
            "content": skill.content,
            "input": params.input,
            "note": "Skill execution is not supported via MCP. Use the content with the input as context."
        });

        serde_json::to_string_pretty(&response)
            .map_err(|e| format!("Failed to serialize skill response: {}", e))
    }

    async fn handle_chat_session_list(
        &self,
        params: ChatSessionListParams,
    ) -> Result<String, String> {
        let limit = params.limit as usize;
        let summaries: Vec<ChatSessionSummary> = if let Some(agent_id) = params.agent_id {
            self.backend
                .list_sessions_by_agent(&agent_id)
                .await
                .map_err(|e| format!("Failed to list sessions: {}", e))?
                .into_iter()
                .take(limit)
                .collect()
        } else {
            self.backend
                .list_sessions()
                .await
                .map_err(|e| format!("Failed to list sessions: {}", e))?
                .into_iter()
                .take(limit)
                .collect()
        };

        serde_json::to_string_pretty(&summaries)
            .map_err(|e| format!("Failed to serialize sessions: {}", e))
    }

    async fn handle_chat_session_get(
        &self,
        params: ChatSessionGetParams,
    ) -> Result<String, String> {
        let session = self
            .backend
            .get_session(&params.session_id)
            .await
            .map_err(|e| format!("Failed to get session: {}", e))?;

        serde_json::to_string_pretty(&session)
            .map_err(|e| format!("Failed to serialize session: {}", e))
    }

    async fn handle_manage_background_agents(
        &self,
        params: ManageBackgroundAgentsParams,
    ) -> Result<String, String> {
        let operation = params.operation.trim().to_lowercase();

        let value = match operation.as_str() {
            "list" => {
                let status = Self::parse_task_status(params.status)?;
                serde_json::to_value(self.backend.list_tasks(status).await?)
                    .map_err(|e| e.to_string())?
            }
            "create" => {
                let name = Self::required_string(params.name, "name")?;
                let agent_id = Self::required_string(params.agent_id, "agent_id")?;
                let schedule = Self::parse_optional_value::<BackgroundAgentSchedule>(
                    "schedule",
                    params.schedule,
                )?
                .unwrap_or_default();
                let memory = Self::parse_optional_value::<MemoryConfig>("memory", params.memory)?;
                let memory = Self::merge_memory_scope(memory, params.memory_scope)?;
                let spec = BackgroundAgentSpec {
                    name,
                    agent_id,
                    description: params.description,
                    input: params.input,
                    input_template: params.input_template,
                    schedule,
                    notification: Self::parse_optional_value("notification", params.notification)?,
                    execution_mode: Self::parse_optional_value(
                        "execution_mode",
                        params.execution_mode,
                    )?,
                    memory,
                };
                serde_json::to_value(self.backend.create_background_agent(spec).await?)
                    .map_err(|e| e.to_string())?
            }
            "update" => {
                let id = Self::required_string(params.id, "id")?;
                let memory = Self::parse_optional_value::<MemoryConfig>("memory", params.memory)?;
                let memory = Self::merge_memory_scope(memory, params.memory_scope)?;
                let patch = BackgroundAgentPatch {
                    name: params.name,
                    description: params.description,
                    agent_id: params.agent_id,
                    input: params.input,
                    input_template: params.input_template,
                    schedule: Self::parse_optional_value("schedule", params.schedule)?,
                    notification: Self::parse_optional_value("notification", params.notification)?,
                    execution_mode: Self::parse_optional_value(
                        "execution_mode",
                        params.execution_mode,
                    )?,
                    memory,
                };
                serde_json::to_value(self.backend.update_background_agent(&id, patch).await?)
                    .map_err(|e| e.to_string())?
            }
            "delete" => {
                let id = Self::required_string(params.id, "id")?;
                let deleted = self.backend.delete_background_agent(&id).await?;
                serde_json::json!({ "id": id, "deleted": deleted })
            }
            "pause" => {
                let id = Self::required_string(params.id, "id")?;
                serde_json::to_value(
                    self.backend
                        .control_background_agent(&id, BackgroundAgentControlAction::Pause)
                        .await?,
                )
                .map_err(|e| e.to_string())?
            }
            "resume" => {
                let id = Self::required_string(params.id, "id")?;
                serde_json::to_value(
                    self.backend
                        .control_background_agent(&id, BackgroundAgentControlAction::Resume)
                        .await?,
                )
                .map_err(|e| e.to_string())?
            }
            "run" => {
                let id = Self::required_string(params.id, "id")?;
                serde_json::to_value(
                    self.backend
                        .control_background_agent(&id, BackgroundAgentControlAction::RunNow)
                        .await?,
                )
                .map_err(|e| e.to_string())?
            }
            "cancel" => {
                let id = Self::required_string(params.id, "id")?;
                let deleted = self.backend.delete_background_agent(&id).await?;
                serde_json::json!({ "id": id, "deleted": deleted })
            }
            "control" => {
                let id = Self::required_string(params.id, "id")?;
                let action = Self::parse_control_action(params.action)?;
                serde_json::to_value(self.backend.control_background_agent(&id, action).await?)
                    .map_err(|e| e.to_string())?
            }
            "progress" => {
                let id = Self::required_string(params.id, "id")?;
                let event_limit = params.event_limit.unwrap_or(10).max(1);
                serde_json::to_value(
                    self.backend
                        .get_background_agent_progress(&id, event_limit)
                        .await?,
                )
                .map_err(|e| e.to_string())?
            }
            "send_message" => {
                let id = Self::required_string(params.id, "id")?;
                let message = Self::required_string(params.message, "message")?;
                let source = Self::parse_message_source(params.source)?;
                serde_json::to_value(
                    self.backend
                        .send_background_agent_message(&id, message, source)
                        .await?,
                )
                .map_err(|e| e.to_string())?
            }
            "list_messages" => {
                let id = Self::required_string(params.id, "id")?;
                let limit = params.limit.unwrap_or(50).max(1);
                serde_json::to_value(
                    self.backend
                        .list_background_agent_messages(&id, limit)
                        .await?,
                )
                .map_err(|e| e.to_string())?
            }
            _ => {
                return Err(format!(
                    "Unknown operation: {}. Supported: create, update, delete, list, control, progress, send_message, list_messages, pause, resume, cancel, run",
                    operation
                ));
            }
        };

        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    }

    async fn handle_manage_hooks(&self, params: ManageHooksParams) -> Result<String, String> {
        let operation = params.operation.trim().to_lowercase();

        let value = match operation.as_str() {
            "list" => {
                serde_json::to_value(self.backend.list_hooks().await?).map_err(|e| e.to_string())?
            }
            "create" => {
                let name = Self::required_string(params.name, "name")?;
                let event_str = Self::required_string(params.event, "event")?;
                let event: HookEvent = serde_json::from_value(
                    Value::String(event_str.clone()),
                )
                .map_err(|_| {
                    format!(
                        "Invalid event: {}. Supported: task_started, task_completed, task_failed, task_cancelled, tool_executed, approval_required",
                        event_str
                    )
                })?;
                let action_value = params.action.ok_or("Missing required field: action")?;
                let action: HookAction = serde_json::from_value(action_value)
                    .map_err(|e| format!("Invalid action: {}", e))?;
                let mut hook = Hook::new(name, event, action);
                hook.description = params.description;
                if let Some(filter_value) = params.filter {
                    hook.filter = Some(
                        serde_json::from_value::<HookFilter>(filter_value)
                            .map_err(|e| format!("Invalid filter: {}", e))?,
                    );
                }
                if let Some(enabled) = params.enabled {
                    hook.enabled = enabled;
                }
                serde_json::to_value(self.backend.create_hook(hook).await?)
                    .map_err(|e| e.to_string())?
            }
            "update" => {
                let id = Self::required_string(params.id, "id")?;
                let hooks = self.backend.list_hooks().await?;
                let mut hook = hooks
                    .into_iter()
                    .find(|h| h.id == id)
                    .ok_or_else(|| format!("Hook not found: {}", id))?;
                if let Some(name) = params.name {
                    hook.name = name;
                }
                if let Some(desc) = params.description {
                    hook.description = Some(desc);
                }
                if let Some(event_str) = params.event {
                    hook.event = serde_json::from_value(Value::String(event_str.clone()))
                        .map_err(|_| format!("Invalid event: {}", event_str))?;
                }
                if let Some(action_value) = params.action {
                    hook.action = serde_json::from_value(action_value)
                        .map_err(|e| format!("Invalid action: {}", e))?;
                }
                if let Some(filter_value) = params.filter {
                    hook.filter = Some(
                        serde_json::from_value::<HookFilter>(filter_value)
                            .map_err(|e| format!("Invalid filter: {}", e))?,
                    );
                }
                if let Some(enabled) = params.enabled {
                    hook.enabled = enabled;
                }
                hook.touch();
                serde_json::to_value(self.backend.update_hook(&id, hook).await?)
                    .map_err(|e| e.to_string())?
            }
            "delete" => {
                let id = Self::required_string(params.id, "id")?;
                let deleted = self.backend.delete_hook(&id).await?;
                serde_json::json!({ "id": id, "deleted": deleted })
            }
            _ => {
                return Err(format!(
                    "Unknown operation: {}. Supported: list, create, update, delete",
                    operation
                ));
            }
        };

        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    }

    async fn handle_runtime_tool(&self, name: &str, input: Value) -> Result<String, String> {
        let output = self.backend.execute_runtime_tool(name, input).await?;
        if output.success {
            serde_json::to_string_pretty(&output.result).map_err(|e| e.to_string())
        } else {
            Err(output
                .error
                .unwrap_or_else(|| format!("Tool '{}' execution failed", name)))
        }
    }

    async fn handle_switch_model_for_mcp(&self, input: Value) -> Result<String, String> {
        let output = RuntimeTool::execute(&self.switch_model_tool, input)
            .await
            .map_err(|e| e.to_string())?;
        if output.success {
            serde_json::to_string_pretty(&output.result).map_err(|e| e.to_string())
        } else {
            Err(output
                .error
                .unwrap_or_else(|| "switch_model execution failed".to_string()))
        }
    }
}

// ============================================================================
// Server Handler Implementation
// ============================================================================

impl ServerHandler for RestFlowMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "restflow".to_string(),
                title: Some("RestFlow MCP Server".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "RestFlow MCP Server - Manage skills, agents, memory, chat sessions, and hooks. \
                Use list_skills/get_skill to access skills, list_agents/get_agent for agents, \
                memory_search/memory_store for memory, chat_session_list/chat_session_get for sessions, \
                manage_hooks for lifecycle hook automation, \
                and manage_background_agents for background agent lifecycle, progress, and messaging operations."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = vec![
            Tool::new(
                "list_skills",
                "List all available skills in RestFlow. Returns a summary of each skill including ID, name, description, and tags.",
                schema_for_type::<EmptyParams>(),
            ),
            Tool::new(
                "get_skill",
                "Get the full content of a skill by its ID. Returns the complete skill including its markdown content.",
                schema_for_type::<GetSkillParams>(),
            ),
            Tool::new(
                "create_skill",
                "Create a new skill in RestFlow. Provide a name, optional description, optional tags, and the markdown content.",
                schema_for_type::<CreateSkillParams>(),
            ),
            Tool::new(
                "update_skill",
                "Update an existing skill in RestFlow. Provide the skill ID and the fields to update.",
                schema_for_type::<UpdateSkillParams>(),
            ),
            Tool::new(
                "delete_skill",
                "Delete a skill from RestFlow by its ID.",
                schema_for_type::<DeleteSkillParams>(),
            ),
            Tool::new(
                "list_agents",
                "List all available agents in RestFlow. Returns a summary of each agent including ID, name, and model.",
                schema_for_type::<EmptyParams>(),
            ),
            Tool::new(
                "get_agent",
                "Get the full configuration of an agent by its ID. Returns the complete agent including model, prompt, temperature, and tools.",
                schema_for_type::<GetAgentParams>(),
            ),
            Tool::new(
                "memory_search",
                "Search memory chunks for an agent using keyword matching.",
                schema_for_type::<MemorySearchParams>(),
            ),
            Tool::new(
                "memory_store",
                "Store a new memory chunk for an agent.",
                schema_for_type::<MemoryStoreParams>(),
            ),
            Tool::new(
                "memory_stats",
                "Get memory statistics for an agent.",
                schema_for_type::<MemoryStatsParams>(),
            ),
            Tool::new(
                "skill_execute",
                "Fetch a skill's content for execution context.",
                schema_for_type::<SkillExecuteParams>(),
            ),
            Tool::new(
                "chat_session_list",
                "List chat sessions (optionally filtered by agent).",
                schema_for_type::<ChatSessionListParams>(),
            ),
            Tool::new(
                "chat_session_get",
                "Get a chat session by ID, including its message history.",
                schema_for_type::<ChatSessionGetParams>(),
            ),
            Tool::new(
                "manage_background_agents",
                "Manage background agents with explicit operations: create, update, delete, list, control, progress, send_message, list_messages, pause, resume, cancel, and run.",
                schema_for_type::<ManageBackgroundAgentsParams>(),
            ),
            Tool::new(
                "manage_hooks",
                "Create, list, update, and delete lifecycle hooks. Hooks trigger actions (webhook, script, send_message, run_task) when events occur (task_started, task_completed, task_failed, task_cancelled, tool_executed, approval_required).",
                schema_for_type::<ManageHooksParams>(),
            ),
        ];

        if let Ok(runtime_tools) = self.backend.list_runtime_tools().await {
            let mut known_names: HashSet<String> =
                tools.iter().map(|tool| tool.name.to_string()).collect();
            let mut runtime_by_name: HashMap<String, RuntimeToolDefinition> = HashMap::new();

            for runtime_tool in runtime_tools {
                runtime_by_name.insert(runtime_tool.name.clone(), runtime_tool.clone());
                if known_names.insert(runtime_tool.name.clone()) {
                    let parameters = match runtime_tool.parameters {
                        Value::Object(map) => map,
                        _ => serde_json::Map::new(),
                    };
                    tools.push(Tool::new(
                        runtime_tool.name,
                        runtime_tool.description,
                        parameters,
                    ));
                }
            }

            for (alias_name, target_name) in [
                ("http", "http_request"),
                ("email", "send_email"),
                ("telegram", "telegram_send"),
                ("use_skill", "skill"),
            ] {
                if !known_names.contains(alias_name)
                    && let Some(target) = runtime_by_name.get(target_name)
                {
                    let parameters = match target.parameters.clone() {
                        Value::Object(map) => map,
                        _ => serde_json::Map::new(),
                    };
                    tools.push(Tool::new(
                        alias_name,
                        format!("Alias of '{}' for main-agent compatibility.", target_name),
                        parameters,
                    ));
                    known_names.insert(alias_name.to_string());
                }
            }

            for runtime_tool in Self::session_scoped_runtime_tools() {
                if known_names.insert(runtime_tool.name.clone()) {
                    let parameters = match runtime_tool.parameters {
                        Value::Object(map) => map,
                        _ => serde_json::Map::new(),
                    };
                    tools.push(Tool::new(
                        runtime_tool.name,
                        runtime_tool.description,
                        parameters,
                    ));
                }
            }
        }

        Ok(ListToolsResult {
            meta: None,
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let result = match request.name.as_ref() {
            "list_skills" => self.handle_list_skills().await,
            "get_skill" => {
                let params: GetSkillParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_get_skill(params).await
            }
            "create_skill" => {
                let params: CreateSkillParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_create_skill(params).await
            }
            "update_skill" => {
                let params: UpdateSkillParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_update_skill(params).await
            }
            "delete_skill" => {
                let params: DeleteSkillParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_delete_skill(params).await
            }
            "list_agents" => self.handle_list_agents().await,
            "get_agent" => {
                let params: GetAgentParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_get_agent(params).await
            }
            "memory_search" => {
                let params: MemorySearchParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_memory_search(params).await
            }
            "memory_store" => {
                let params: MemoryStoreParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_memory_store(params).await
            }
            "memory_stats" => {
                let params: MemoryStatsParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_memory_stats(params).await
            }
            "skill_execute" => {
                let params: SkillExecuteParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_skill_execute(params).await
            }
            "chat_session_list" => {
                let params: ChatSessionListParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_chat_session_list(params).await
            }
            "chat_session_get" => {
                let params: ChatSessionGetParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_chat_session_get(params).await
            }
            "manage_background_agents" => {
                let params: ManageBackgroundAgentsParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_manage_background_agents(params).await
            }
            "manage_hooks" => {
                let params: ManageHooksParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_manage_hooks(params).await
            }
            "switch_model" => {
                self.handle_switch_model_for_mcp(Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .await
            }
            "spawn_agent" | "wait_agents" => Err(format!(
                "Tool '{}' requires active main-agent runtime context and is not executable from standalone MCP yet.",
                request.name
            )),
            "use_skill" => {
                let converted = Self::convert_use_skill_input(Value::Object(
                    request.arguments.unwrap_or_default(),
                ));
                self.handle_runtime_tool("skill", converted).await
            }
            _ => {
                if let Some(target) = Self::runtime_alias_target(request.name.as_ref()) {
                    self.handle_runtime_tool(
                        target,
                        Value::Object(request.arguments.unwrap_or_default()),
                    )
                    .await
                } else {
                    self.handle_runtime_tool(
                        request.name.as_ref(),
                        Value::Object(request.arguments.unwrap_or_default()),
                    )
                    .await
                }
            }
        };

        match result {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(error)])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::{IpcClient, IpcServer};
    use crate::models::{AIModel, AgentNode, ApiKeyConfig, Skill};
    use crate::storage::agent::StoredAgent;
    use tempfile::TempDir;
    use tokio::time::{Duration, sleep};

    // =========================================================================
    // Test Utilities
    // =========================================================================

    /// Create a test server with a temporary database
    async fn create_test_server() -> (RestFlowMcpServer, Arc<AppCore>, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (RestFlowMcpServer::new(core.clone()), core, temp_dir)
    }

    /// Create a test skill with given id and name
    fn create_test_skill(id: &str, name: &str) -> Skill {
        Skill::new(
            id.to_string(),
            name.to_string(),
            Some(format!("Description for {}", name)),
            Some(vec!["test".to_string()]),
            format!("# {}\n\nContent here.", name),
        )
    }

    /// Create a test agent node
    fn create_test_agent_node(prompt: &str) -> AgentNode {
        AgentNode {
            model: Some(AIModel::ClaudeSonnet4_5),
            prompt: Some(prompt.to_string()),
            temperature: Some(0.7),
            codex_cli_reasoning_effort: None,
            codex_cli_execution_mode: None,
            api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
            tools: Some(vec!["add".to_string()]),
            skills: None,
            skill_variables: None,
            python_runtime_policy: None,
        }
    }

    // =========================================================================
    // Serialization Tests
    // =========================================================================

    #[test]
    fn test_skill_summary_serialization() {
        let summary = SkillSummary {
            id: "test-id".to_string(),
            name: "Test Skill".to_string(),
            description: Some("A test skill".to_string()),
            tags: Some(vec!["test".to_string()]),
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("Test Skill"));
    }

    #[test]
    fn test_agent_summary_serialization() {
        let summary = AgentSummary {
            id: "test-id".to_string(),
            name: "Test Agent".to_string(),
            model: "gpt-5".to_string(),
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("gpt-5"));
    }

    // =========================================================================
    // Skill Tool Tests
    // =========================================================================

    #[tokio::test]
    async fn test_list_skills_empty() {
        let (server, _core, _temp_dir) = create_test_server().await;

        let result = server.handle_list_skills().await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_list_skills_multiple() {
        let (server, core, _temp_dir) = create_test_server().await;

        // Create skills using the service layer
        let skill1 = create_test_skill("skill-1", "Skill One");
        let skill2 = create_test_skill("skill-2", "Skill Two");

        crate::services::skills::create_skill(&core, skill1)
            .await
            .unwrap();
        crate::services::skills::create_skill(&core, skill2)
            .await
            .unwrap();

        let result = server.handle_list_skills().await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
        assert_eq!(skills.len(), 2);
    }

    #[tokio::test]
    async fn test_get_skill_success() {
        let (server, core, _temp_dir) = create_test_server().await;

        let skill = create_test_skill("test-skill", "Test Skill");
        crate::services::skills::create_skill(&core, skill.clone())
            .await
            .unwrap();

        let params = GetSkillParams {
            id: "test-skill".to_string(),
        };
        let result = server.handle_get_skill(params).await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let retrieved: Skill = serde_json::from_str(&json).unwrap();
        assert_eq!(retrieved.id, "test-skill");
        assert_eq!(retrieved.name, "Test Skill");
        assert_eq!(retrieved.content, skill.content);
    }

    #[tokio::test]
    async fn test_get_skill_not_found() {
        let (server, _core, _temp_dir) = create_test_server().await;

        let params = GetSkillParams {
            id: "nonexistent".to_string(),
        };
        let result = server.handle_get_skill(params).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("not found"));
    }

    #[tokio::test]
    async fn test_create_skill_success() {
        let (server, _core, _temp_dir) = create_test_server().await;

        let params = CreateSkillParams {
            name: "New Skill".to_string(),
            description: Some("A new skill".to_string()),
            tags: Some(vec!["new".to_string()]),
            content: "# New Skill\n\nContent".to_string(),
        };
        let result = server.handle_create_skill(params).await;

        assert!(result.is_ok());
        let message = result.unwrap();
        assert!(message.contains("created successfully"));

        // Verify it was persisted
        let skills = server.handle_list_skills().await.unwrap();
        let skill_list: Vec<SkillSummary> = serde_json::from_str(&skills).unwrap();
        assert_eq!(skill_list.len(), 1);
        assert_eq!(skill_list[0].name, "New Skill");
    }

    #[tokio::test]
    async fn test_update_skill_success() {
        let (server, core, _temp_dir) = create_test_server().await;

        let skill = create_test_skill("test-skill", "Original Name");
        crate::services::skills::create_skill(&core, skill)
            .await
            .unwrap();

        let params = UpdateSkillParams {
            id: "test-skill".to_string(),
            name: Some("Updated Name".to_string()),
            description: Some("Updated description".to_string()),
            tags: None,
            content: Some("# Updated content".to_string()),
        };
        let result = server.handle_update_skill(params).await;

        assert!(result.is_ok());

        // Verify changes
        let get_params = GetSkillParams {
            id: "test-skill".to_string(),
        };
        let json = server.handle_get_skill(get_params).await.unwrap();
        let updated: Skill = serde_json::from_str(&json).unwrap();
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.description, Some("Updated description".to_string()));
        assert_eq!(updated.content, "# Updated content");
    }

    #[tokio::test]
    async fn test_update_skill_not_found() {
        let (server, _core, _temp_dir) = create_test_server().await;

        let params = UpdateSkillParams {
            id: "nonexistent".to_string(),
            name: Some("New Name".to_string()),
            description: None,
            tags: None,
            content: None,
        };
        let result = server.handle_update_skill(params).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_update_skill_partial() {
        let (server, core, _temp_dir) = create_test_server().await;

        let skill = create_test_skill("test-skill", "Original Name");
        crate::services::skills::create_skill(&core, skill)
            .await
            .unwrap();

        // Only update name, keep other fields
        let params = UpdateSkillParams {
            id: "test-skill".to_string(),
            name: Some("New Name".to_string()),
            description: None,
            tags: None,
            content: None,
        };
        server.handle_update_skill(params).await.unwrap();

        let get_params = GetSkillParams {
            id: "test-skill".to_string(),
        };
        let json = server.handle_get_skill(get_params).await.unwrap();
        let updated: Skill = serde_json::from_str(&json).unwrap();

        assert_eq!(updated.name, "New Name");
        // Original description should be preserved
        assert_eq!(
            updated.description,
            Some("Description for Original Name".to_string())
        );
    }

    #[tokio::test]
    async fn test_delete_skill_success() {
        let (server, core, _temp_dir) = create_test_server().await;

        let skill = create_test_skill("test-skill", "Test Skill");
        crate::services::skills::create_skill(&core, skill)
            .await
            .unwrap();

        let params = DeleteSkillParams {
            id: "test-skill".to_string(),
        };
        let result = server.handle_delete_skill(params).await;

        assert!(result.is_ok());

        // Verify deletion
        let get_params = GetSkillParams {
            id: "test-skill".to_string(),
        };
        let get_result = server.handle_get_skill(get_params).await;
        assert!(get_result.is_err());
    }

    // =========================================================================
    // Agent Tool Tests
    // =========================================================================

    #[tokio::test]
    async fn test_list_agents_default() {
        // AppCore creates a default agent on initialization
        let (server, _core, _temp_dir) = create_test_server().await;

        let result = server.handle_list_agents().await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let agents: Vec<AgentSummary> = serde_json::from_str(&json).unwrap();
        // Expect exactly one default agent created by AppCore
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "Default Assistant");
    }

    #[tokio::test]
    async fn test_list_agents_multiple() {
        // AppCore creates a default agent, so we start with 1
        let (server, core, _temp_dir) = create_test_server().await;

        let agent1 = create_test_agent_node("Prompt 1");
        let agent2 = create_test_agent_node("Prompt 2");

        crate::services::agent::create_agent(&core, "Agent 1".to_string(), agent1)
            .await
            .unwrap();
        crate::services::agent::create_agent(&core, "Agent 2".to_string(), agent2)
            .await
            .unwrap();

        let result = server.handle_list_agents().await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let agents: Vec<AgentSummary> = serde_json::from_str(&json).unwrap();
        // 1 default + 2 created = 3 agents
        assert_eq!(agents.len(), 3);
    }

    #[tokio::test]
    async fn test_get_agent_success() {
        let (server, core, _temp_dir) = create_test_server().await;

        let agent_node = create_test_agent_node("Test prompt");
        let stored =
            crate::services::agent::create_agent(&core, "Test Agent".to_string(), agent_node)
                .await
                .unwrap();

        let params = GetAgentParams {
            id: stored.id.clone(),
        };
        let result = server.handle_get_agent(params).await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let retrieved: StoredAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(retrieved.id, stored.id);
        assert_eq!(retrieved.name, "Test Agent");
        assert_eq!(retrieved.agent.prompt, Some("Test prompt".to_string()));
    }

    #[tokio::test]
    async fn test_get_agent_not_found() {
        let (server, _core, _temp_dir) = create_test_server().await;

        let params = GetAgentParams {
            id: "nonexistent".to_string(),
        };
        let result = server.handle_get_agent(params).await;

        assert!(result.is_err());
    }

    // =========================================================================
    // ServerHandler Trait Tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_info() {
        let (server, _core, _temp_dir) = create_test_server().await;

        let info = server.get_info();

        assert_eq!(info.server_info.name, "restflow");
        assert!(info.capabilities.tools.is_some());
        assert!(info.instructions.is_some());
    }

    #[test]
    fn test_tool_definitions() {
        // Verify tool definitions are correct without needing RequestContext
        // The actual list_tools method would be called by the MCP framework
        let expected_tools = [
            "list_skills",
            "get_skill",
            "create_skill",
            "update_skill",
            "delete_skill",
            "list_agents",
            "get_agent",
            "memory_search",
            "memory_store",
            "memory_stats",
            "skill_execute",
            "chat_session_list",
            "chat_session_get",
            "manage_background_agents",
        ];

        // Verify we have definitions for all expected tools
        assert_eq!(expected_tools.len(), 14);
    }

    #[tokio::test]
    async fn test_handle_unknown_tool() {
        let (server, _core, _temp_dir) = create_test_server().await;

        // Test unknown tool handling by simulating what call_tool does internally
        let result = match "unknown_tool" {
            "list_skills" => server.handle_list_skills().await,
            _ => Err(format!("Unknown tool: {}", "unknown_tool")),
        };

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_handle_invalid_skill_params() {
        // Create test server to ensure setup works (also keeps pattern consistent)
        let (_server, _core, _temp_dir) = create_test_server().await;

        // Test with invalid params - missing required id field
        let args = serde_json::json!({"wrong_field": "value"});
        let result: Result<GetSkillParams, _> = serde_json::from_value(args);

        // Should fail to parse
        assert!(result.is_err());
    }

    // =========================================================================
    // Integration Tests (Full Workflow)
    // =========================================================================

    #[tokio::test]
    async fn test_skill_crud_workflow() {
        let (server, _core, _temp_dir) = create_test_server().await;

        // 1. Create
        let create_params = CreateSkillParams {
            name: "Workflow Skill".to_string(),
            description: Some("Test workflow".to_string()),
            tags: Some(vec!["workflow".to_string()]),
            content: "# Workflow\n\nInitial content".to_string(),
        };
        let create_result = server.handle_create_skill(create_params).await.unwrap();
        assert!(create_result.contains("created successfully"));

        // 2. List to get ID
        let list_json = server.handle_list_skills().await.unwrap();
        let skills: Vec<SkillSummary> = serde_json::from_str(&list_json).unwrap();
        assert_eq!(skills.len(), 1);
        let skill_id = skills[0].id.clone();

        // 3. Get
        let get_params = GetSkillParams {
            id: skill_id.clone(),
        };
        let get_json = server.handle_get_skill(get_params).await.unwrap();
        let skill: Skill = serde_json::from_str(&get_json).unwrap();
        assert_eq!(skill.name, "Workflow Skill");

        // 4. Update
        let update_params = UpdateSkillParams {
            id: skill_id.clone(),
            name: Some("Updated Workflow Skill".to_string()),
            description: None,
            tags: None,
            content: Some("# Updated\n\nNew content".to_string()),
        };
        server.handle_update_skill(update_params).await.unwrap();

        // 5. Verify update
        let get_params2 = GetSkillParams {
            id: skill_id.clone(),
        };
        let get_json2 = server.handle_get_skill(get_params2).await.unwrap();
        let updated_skill: Skill = serde_json::from_str(&get_json2).unwrap();
        assert_eq!(updated_skill.name, "Updated Workflow Skill");
        assert_eq!(updated_skill.content, "# Updated\n\nNew content");

        // 6. Delete
        let delete_params = DeleteSkillParams {
            id: skill_id.clone(),
        };
        server.handle_delete_skill(delete_params).await.unwrap();

        // 7. Verify deletion
        let final_list = server.handle_list_skills().await.unwrap();
        let final_skills: Vec<SkillSummary> = serde_json::from_str(&final_list).unwrap();
        assert!(final_skills.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_ipc_backend_list_skills() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("mcp-ipc.db");
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());

        let socket_path = std::env::temp_dir().join(format!(
            "restflow-mcp-{}-{}.sock",
            std::process::id(),
            chrono::Utc::now().timestamp_millis()
        ));
        let _ = std::fs::remove_file(&socket_path);
        let ipc_server = IpcServer::new(core.clone(), socket_path.clone());
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        let shutdown_rx = shutdown_tx.subscribe();
        let mut server_handle = Some(tokio::spawn(
            async move { ipc_server.run(shutdown_rx).await },
        ));

        let mut client = None;
        let mut last_connect_error = None;
        for _ in 0..100 {
            match IpcClient::connect(&socket_path).await {
                Ok(connected) => {
                    client = Some(connected);
                    break;
                }
                Err(err) => {
                    last_connect_error = Some(err.to_string());
                }
            }
            sleep(Duration::from_millis(50)).await;
        }

        let server_hint = if client.is_none()
            && server_handle
                .as_ref()
                .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            let handle = server_handle.take().unwrap();
            match handle.await {
                Ok(Ok(())) => "ipc server exited before client connection".to_string(),
                Ok(Err(err)) => format!("ipc server startup failed: {}", err),
                Err(err) => format!("ipc server task join failed: {}", err),
            }
        } else {
            "ipc server still running".to_string()
        };

        if client.is_none() && server_hint.contains("Operation not permitted") {
            eprintln!(
                "Skipping IPC backend test in restricted environment: {}",
                server_hint
            );
            let _ = shutdown_tx.send(());
            if let Some(handle) = server_handle.take() {
                let _ = handle.await;
            }
            let _ = std::fs::remove_file(&socket_path);
            return;
        }

        let client = client.unwrap_or_else(|| {
            panic!(
                "Failed to connect to IPC server: {} ({})",
                last_connect_error.unwrap_or_else(|| "unknown error".to_string()),
                server_hint
            )
        });
        let mcp_server = RestFlowMcpServer::with_ipc(client);

        let skill = create_test_skill("ipc-skill", "IPC Skill");
        crate::services::skills::create_skill(&core, skill)
            .await
            .unwrap();

        let json = mcp_server.handle_list_skills().await.unwrap();
        let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "IPC Skill");

        let _ = shutdown_tx.send(());
        if let Some(handle) = server_handle.take() {
            let _ = handle.await;
        }
        let _ = std::fs::remove_file(&socket_path);
    }

    struct MockBackend {
        skills: Vec<Skill>,
        session: ChatSession,
    }

    impl MockBackend {
        fn new() -> Self {
            let skill = Skill::new(
                "mock-skill".to_string(),
                "Mock Skill".to_string(),
                Some("Mock description".to_string()),
                None,
                "# Mock".to_string(),
            );
            let session = ChatSession::new("mock-agent".to_string(), "mock-model".to_string())
                .with_name("Mock Session");
            Self {
                skills: vec![skill],
                session,
            }
        }

        fn agent_summary(&self) -> StoredAgent {
            StoredAgent {
                id: "mock-agent".to_string(),
                name: "Mock Agent".to_string(),
                agent: AgentNode {
                    model: Some(AIModel::ClaudeSonnet4_5),
                    prompt: Some("Mock prompt".to_string()),
                    temperature: Some(0.5),
                    codex_cli_reasoning_effort: None,
                    codex_cli_execution_mode: None,
                    api_key_config: Some(ApiKeyConfig::Direct("mock_key".to_string())),
                    tools: None,
                    skills: None,
                    skill_variables: None,
                    python_runtime_policy: None,
                },
                created_at: None,
                updated_at: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl McpBackend for MockBackend {
        async fn list_skills(&self) -> Result<Vec<Skill>, String> {
            Ok(self.skills.clone())
        }

        async fn get_skill(&self, id: &str) -> Result<Option<Skill>, String> {
            Ok(self.skills.iter().find(|s| s.id == id).cloned())
        }

        async fn create_skill(&self, _skill: Skill) -> Result<(), String> {
            Ok(())
        }

        async fn update_skill(&self, _skill: Skill) -> Result<(), String> {
            Ok(())
        }

        async fn delete_skill(&self, _id: &str) -> Result<(), String> {
            Ok(())
        }

        async fn list_agents(&self) -> Result<Vec<StoredAgent>, String> {
            Ok(vec![self.agent_summary()])
        }

        async fn get_agent(&self, _id: &str) -> Result<StoredAgent, String> {
            Ok(self.agent_summary())
        }

        async fn search_memory(
            &self,
            _query: MemorySearchQuery,
        ) -> Result<MemorySearchResult, String> {
            Ok(MemorySearchResult {
                chunks: Vec::new(),
                total_count: 0,
                has_more: false,
            })
        }

        async fn store_memory(&self, _chunk: MemoryChunk) -> Result<String, String> {
            Ok("mock-chunk".to_string())
        }

        async fn get_memory_stats(&self, agent_id: &str) -> Result<MemoryStats, String> {
            Ok(MemoryStats {
                agent_id: agent_id.to_string(),
                ..MemoryStats::default()
            })
        }

        async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>, String> {
            Ok(vec![ChatSessionSummary::from(&self.session)])
        }

        async fn list_sessions_by_agent(
            &self,
            agent_id: &str,
        ) -> Result<Vec<ChatSessionSummary>, String> {
            let summary = ChatSessionSummary::from(&self.session);
            if summary.agent_id == agent_id {
                Ok(vec![summary])
            } else {
                Ok(Vec::new())
            }
        }

        async fn get_session(&self, id: &str) -> Result<ChatSession, String> {
            if self.session.id == id {
                Ok(self.session.clone())
            } else {
                Err(format!("Session not found: {}", id))
            }
        }

        async fn list_tasks(
            &self,
            _status: Option<BackgroundAgentStatus>,
        ) -> Result<Vec<BackgroundAgent>, String> {
            Ok(Vec::new())
        }

        async fn create_background_agent(
            &self,
            _spec: BackgroundAgentSpec,
        ) -> Result<BackgroundAgent, String> {
            Err("not implemented in mock backend".to_string())
        }

        async fn update_background_agent(
            &self,
            _id: &str,
            _patch: BackgroundAgentPatch,
        ) -> Result<BackgroundAgent, String> {
            Err("not implemented in mock backend".to_string())
        }

        async fn delete_background_agent(&self, _id: &str) -> Result<bool, String> {
            Ok(true)
        }

        async fn control_background_agent(
            &self,
            _id: &str,
            _action: BackgroundAgentControlAction,
        ) -> Result<BackgroundAgent, String> {
            Err("not implemented in mock backend".to_string())
        }

        async fn get_background_agent_progress(
            &self,
            _id: &str,
            _event_limit: usize,
        ) -> Result<BackgroundProgress, String> {
            Err("not implemented in mock backend".to_string())
        }

        async fn send_background_agent_message(
            &self,
            _id: &str,
            _message: String,
            _source: BackgroundMessageSource,
        ) -> Result<BackgroundMessage, String> {
            Err("not implemented in mock backend".to_string())
        }

        async fn list_background_agent_messages(
            &self,
            _id: &str,
            _limit: usize,
        ) -> Result<Vec<BackgroundMessage>, String> {
            Ok(Vec::new())
        }

        async fn list_hooks(&self) -> Result<Vec<Hook>, String> {
            Ok(Vec::new())
        }

        async fn create_hook(&self, hook: Hook) -> Result<Hook, String> {
            Ok(hook)
        }

        async fn update_hook(&self, _id: &str, hook: Hook) -> Result<Hook, String> {
            Ok(hook)
        }

        async fn delete_hook(&self, _id: &str) -> Result<bool, String> {
            Ok(true)
        }

        async fn list_runtime_tools(&self) -> Result<Vec<RuntimeToolDefinition>, String> {
            Ok(vec![RuntimeToolDefinition {
                name: "echo_runtime".to_string(),
                description: "Echo the input payload.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" }
                    }
                }),
            }])
        }

        async fn execute_runtime_tool(
            &self,
            name: &str,
            input: Value,
        ) -> Result<RuntimeToolResult, String> {
            if name == "echo_runtime" {
                Ok(RuntimeToolResult {
                    success: true,
                    result: input,
                    error: None,
                })
            } else {
                Err(format!("Unknown runtime tool: {}", name))
            }
        }
    }

    #[tokio::test]
    async fn test_mock_backend_list_skills() {
        let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
        let json = server.handle_list_skills().await.unwrap();
        let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "Mock Skill");
    }

    #[tokio::test]
    async fn test_mock_backend_session_filter() {
        let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
        let params = ChatSessionListParams {
            agent_id: Some("mock-agent".to_string()),
            limit: 10,
        };
        let json = server.handle_chat_session_list(params).await.unwrap();
        let sessions: Vec<ChatSessionSummary> = serde_json::from_str(&json).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].agent_id, "mock-agent");
    }

    #[tokio::test]
    async fn test_manage_background_agents_list_operation() {
        let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
        let params = ManageBackgroundAgentsParams {
            operation: "list".to_string(),
            id: None,
            name: None,
            agent_id: None,
            description: None,
            input: None,
            input_template: None,
            schedule: None,
            notification: None,
            execution_mode: None,
            memory: None,
            memory_scope: None,
            status: None,
            action: None,
            event_limit: None,
            message: None,
            source: None,
            limit: None,
        };

        let json = server
            .handle_manage_background_agents(params)
            .await
            .unwrap();
        let tasks: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_manage_hooks_list_operation() {
        let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
        let params = ManageHooksParams {
            operation: "list".to_string(),
            id: None,
            name: None,
            description: None,
            event: None,
            action: None,
            filter: None,
            enabled: None,
        };

        let json = server.handle_manage_hooks(params).await.unwrap();
        let hooks: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert!(hooks.is_empty());
    }

    #[tokio::test]
    async fn test_manage_hooks_create_operation() {
        let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
        let params = ManageHooksParams {
            operation: "create".to_string(),
            id: None,
            name: Some("Test Hook".to_string()),
            description: Some("A test hook".to_string()),
            event: Some("task_completed".to_string()),
            action: Some(serde_json::json!({
                "type": "webhook",
                "url": "https://example.com/hook"
            })),
            filter: None,
            enabled: None,
        };

        let json = server.handle_manage_hooks(params).await.unwrap();
        let hook: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(hook["name"], "Test Hook");
        assert_eq!(hook["event"], "task_completed");
        assert_eq!(hook["enabled"], true);
    }

    #[tokio::test]
    async fn test_manage_hooks_invalid_operation() {
        let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
        let params = ManageHooksParams {
            operation: "invalid".to_string(),
            id: None,
            name: None,
            description: None,
            event: None,
            action: None,
            filter: None,
            enabled: None,
        };

        let result = server.handle_manage_hooks(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown operation"));
    }

    #[tokio::test]
    async fn test_runtime_tool_fallback() {
        let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
        let json = server
            .handle_runtime_tool(
                "echo_runtime",
                serde_json::json!({ "value": "hello-runtime" }),
            )
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["value"], "hello-runtime");
    }

    #[tokio::test]
    async fn test_runtime_tools_include_manage_agents() {
        let (server, _core, _temp_dir) = create_test_server().await;
        let runtime_tools = server.backend.list_runtime_tools().await.unwrap();
        assert!(
            runtime_tools
                .iter()
                .any(|tool| tool.name == "manage_agents")
        );
    }

    #[tokio::test]
    async fn test_manage_agents_runtime_tool_list_operation() {
        let (server, _core, _temp_dir) = create_test_server().await;
        let json = server
            .handle_runtime_tool("manage_agents", serde_json::json!({ "operation": "list" }))
            .await
            .unwrap();
        let agents: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert!(!agents.is_empty());
    }

    #[test]
    fn test_convert_use_skill_input_maps_to_skill_read() {
        let input = serde_json::json!({
            "skill_id": "my-skill"
        });
        let output = RestFlowMcpServer::convert_use_skill_input(input);
        assert_eq!(output["action"], "read");
        assert_eq!(output["id"], "my-skill");
    }

    #[test]
    fn test_session_scoped_runtime_tools_include_switch_model() {
        let tools = RestFlowMcpServer::session_scoped_runtime_tools();
        assert!(tools.iter().any(|tool| tool.name == "switch_model"));
        assert!(tools.iter().any(|tool| tool.name == "spawn_agent"));
        assert!(tools.iter().any(|tool| tool.name == "wait_agents"));

        let switch_model = tools
            .iter()
            .find(|tool| tool.name == "switch_model")
            .expect("switch_model tool should exist");
        assert!(switch_model.parameters.get("anyOf").is_none());
        assert!(switch_model.parameters.get("oneOf").is_none());
        assert!(switch_model.parameters.get("allOf").is_none());
        assert_eq!(
            switch_model.parameters["required"],
            serde_json::json!(["provider", "model"])
        );
    }

    #[tokio::test]
    async fn test_switch_model_works_in_standalone_mcp_mode() {
        let (server, _core, _temp_dir) = create_test_server().await;

        let result = server
            .handle_switch_model_for_mcp(serde_json::json!({
                "provider": "openai-codex",
                "model": "gpt-5.3-codex",
                "reason": "MCP standalone test"
            }))
            .await
            .expect("switch_model should succeed in standalone MCP mode");

        let value: serde_json::Value =
            serde_json::from_str(&result).expect("switch_model result should be valid JSON");
        assert_eq!(value["switched"], true);
        assert_eq!(value["to"]["model"], "gpt-5.3-codex");
        assert_eq!(value["to"]["provider"], "codex-cli");
    }
}
