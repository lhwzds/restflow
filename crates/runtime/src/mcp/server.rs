//! MCP server implementation for RestFlow
//!
//! This module provides an MCP server that exposes RestFlow's functionality
//! to AI assistants like Claude Code.

use crate::ApiDefaults;
use crate::AppCore;
use crate::auth::provider_access::build_runtime_api_keys;
use crate::daemon::{IpcClient, IpcRequest};
use crate::models::{
    ChatSession, ChatSessionSummary, ModelId, RunArtifact, RunListQuery, RunSummary, Skill,
    SkillStatus, Task, TaskControlAction, TaskMessage, TaskMessageSource, TaskPatch, TaskProgress,
    TaskSpec, TaskStatus, ValidationError,
};
use crate::services::{
    operation_assessment::OperationAssessorAdapter,
    tool_registry::create_tool_registry_with_assessor,
};
use crate::storage::agent::StoredAgent;
use crate::storage::{SecretStorage, SystemConfig};
use ::types::DeleteWithIdResponse;
use ::types::TaskCommandOutcome;
pub(crate) use ::types::ToolDefinition as RuntimeToolDefinition;
pub(crate) use ::types::ToolExecutionResult as RuntimeToolResult;
use ::types::store::TaskDeleteRequest;
use ai::llm::{CodexClient, DefaultLlmClientFactory, LlmClient, LlmSwitcherImpl, SwappableLlm};
use ai::tools::Tool as RuntimeTool;
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
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::io::{stdin, stdout};
use tokio::sync::Mutex;
use tools::SwitchModelTool;

#[path = "server/agents.rs"]
mod agents;
#[path = "server/backends.rs"]
mod backends;
#[path = "server/runtime_tools.rs"]
mod runtime_tools;
#[path = "server/sessions.rs"]
mod sessions;
#[path = "server/skills.rs"]
mod skills;
#[path = "server/tasks.rs"]
mod tasks;
#[path = "server/types.rs"]
mod types;

use self::backends::{CoreBackend, IpcBackend};
use self::types::*;

fn schema_map_from_value(schema: Value) -> Map<String, Value> {
    schema
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new)
}

/// RestFlow MCP Server
///
/// Exposes skills, agents, and workflow functionality via MCP protocol.
#[derive(Clone)]
pub struct RestFlowMcpServer {
    backend: Arc<dyn McpBackend>,
    switch_model_tool: SwitchModelTool,
}

#[async_trait::async_trait]
pub trait McpBackend: Send + Sync {
    async fn list_skills(&self) -> Result<Vec<Skill>, String>;
    async fn get_skill(&self, id: &str) -> Result<Option<Skill>, String>;
    async fn get_skill_reference(
        &self,
        skill_id: &str,
        ref_id: &str,
    ) -> Result<Option<String>, String>;

    async fn list_agents(&self) -> Result<Vec<StoredAgent>, String>;
    async fn get_agent(&self, id: &str) -> Result<StoredAgent, String>;

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>, String>;
    async fn list_sessions_by_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<ChatSessionSummary>, String>;
    async fn get_session(&self, id: &str) -> Result<ChatSession, String>;

    async fn list_tasks(&self, status: Option<TaskStatus>) -> Result<Vec<Task>, String>;
    async fn create_task(&self, spec: TaskSpec) -> Result<Task, String>;
    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task, String>;
    async fn delete_task(
        &self,
        request: TaskDeleteRequest,
    ) -> Result<TaskCommandOutcome<DeleteWithIdResponse>, String>;
    async fn control_task(&self, id: &str, action: TaskControlAction) -> Result<Task, String>;
    async fn get_task_progress(&self, id: &str, event_limit: usize)
    -> Result<TaskProgress, String>;
    async fn send_task_message(
        &self,
        id: &str,
        message: String,
        source: TaskMessageSource,
    ) -> Result<TaskMessage, String>;
    async fn list_task_messages(&self, id: &str, limit: usize) -> Result<Vec<TaskMessage>, String>;
    async fn list_artifacts(&self, task_id: &str) -> Result<Vec<RunArtifact>, String>;
    async fn list_runs(&self, query: RunListQuery) -> Result<Vec<RunSummary>, String>;
    async fn get_task(&self, id: &str) -> Result<Task, String>;

    async fn list_runtime_tools(&self) -> Result<Vec<RuntimeToolDefinition>, String>;
    async fn execute_runtime_tool(
        &self,
        name: &str,
        input: Value,
    ) -> Result<RuntimeToolResult, String>;
    async fn get_api_defaults(&self) -> Result<ApiDefaults, String>;
}

fn create_runtime_tool_registry_for_core(
    core: &Arc<AppCore>,
) -> anyhow::Result<ai::tools::ToolRegistry> {
    let mut registry = create_tool_registry_with_assessor(
        core.storage.config.clone(),
        None,
        None,
        Some(Arc::new(OperationAssessorAdapter::new(core.clone()))),
    )?;

    let manage_tasks = ["manage_tasks".to_string()];
    let task_registry = crate::runtime::agent::tools::registry_from_allowlist(
        Some(&manage_tasks),
        None,
        None,
        Some(core.storage.as_ref()),
        None,
        None,
        None,
    )?;
    for name in task_registry.list() {
        if let Some(tool) = task_registry.get(name) {
            registry.register_arc(tool);
        }
    }

    Ok(registry)
}

fn build_switch_model_tool(secret_storage: Option<&SecretStorage>) -> SwitchModelTool {
    let api_keys = build_runtime_api_keys(secret_storage);
    let factory = Arc::new(DefaultLlmClientFactory::new(
        api_keys,
        ModelId::build_model_specs(),
    ));
    let initial_client: Arc<dyn LlmClient> = Arc::new(CodexClient::new());
    let swappable = Arc::new(SwappableLlm::new(initial_client));
    let switcher = Arc::new(LlmSwitcherImpl::new(swappable, factory));
    SwitchModelTool::new(switcher)
}

impl RestFlowMcpServer {
    /// Create a new MCP server with the given AppCore
    pub fn new(core: Arc<AppCore>) -> Self {
        Self {
            switch_model_tool: build_switch_model_tool(Some(&core.storage.secrets)),
            backend: Arc::new(CoreBackend {
                core,
                registry: std::sync::OnceLock::new(),
            }),
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

impl RestFlowMcpServer {
    fn parse_skill_status(value: Option<String>) -> Result<Option<SkillStatus>, String> {
        match value.map(|s| s.trim().to_lowercase()) {
            None => Ok(None),
            Some(s) if s.is_empty() => Ok(None),
            Some(s) if s == "active" => Ok(Some(SkillStatus::Active)),
            Some(s) if s == "completed" => Ok(Some(SkillStatus::Completed)),
            Some(s) if s == "archived" => Ok(Some(SkillStatus::Archived)),
            Some(s) if s == "draft" => Ok(Some(SkillStatus::Draft)),
            Some(s) => Err(format!("Unknown skill status: {}", s)),
        }
    }

    async fn load_api_defaults(&self) -> Result<ApiDefaults, String> {
        self.backend
            .get_api_defaults()
            .await
            .map_err(|e| format!("Failed to load API defaults: {}", e))
    }

    /// Runtime tools that are surfaced as explicit MCP-only additions.
    /// Dynamic runtime tools are discovered from backend tool registry schemas.
    fn session_scoped_runtime_tools() -> Vec<RuntimeToolDefinition> {
        vec![RuntimeToolDefinition {
            name: "switch_model".to_string(),
            description: "Switch the active LLM model for the current MCP server session."
                .to_string(),
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
        }]
    }

    #[allow(dead_code)]
    async fn skill_validation_warnings(&self, skill: &Skill) -> Vec<ValidationError> {
        let tool_names = self
            .backend
            .list_runtime_tools()
            .await
            .map(|tools| tools.into_iter().map(|tool| tool.name).collect::<Vec<_>>())
            .unwrap_or_default();
        let skill_ids = self
            .backend
            .list_skills()
            .await
            .map(|skills| skills.into_iter().map(|entry| entry.id).collect::<Vec<_>>())
            .unwrap_or_default();

        crate::services::skills::validate_skill_complete(skill, &tool_names, &skill_ids)
    }

    #[allow(dead_code)]
    fn format_validation_warnings(errors: &[ValidationError]) -> Option<String> {
        if errors.is_empty() {
            return None;
        }

        let message = errors
            .iter()
            .map(|error| format!("{}: {}", error.field, error.message))
            .collect::<Vec<_>>()
            .join("; ");
        Some(format!("Warnings: {}", message))
    }

    fn wrap_backend_error(context: &str, error: String) -> String {
        if serde_json::from_str::<Value>(&error).is_ok() {
            return error;
        }
        format!("{}: {}", context, error)
    }

    fn to_call_tool_result(result: Result<String, String>) -> CallToolResult {
        match result {
            Ok(text) => CallToolResult::success(vec![Content::text(text)]),
            Err(error) => {
                let structured_content = serde_json::from_str::<Value>(&error).ok();
                let mut value = CallToolResult::error(vec![Content::text(error)]);
                value.structured_content = structured_content;
                value
            }
        }
    }
}

// ============================================================================
// Server Handler Implementation
// ============================================================================

impl ServerHandler for RestFlowMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = Default::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = Implementation::new("restflow", env!("CARGO_PKG_VERSION"))
            .with_title("RestFlow MCP Server");
        info.instructions = Some(
            "RestFlow MCP Server - Manage skills, agents, chat sessions, and tasks. \
            Use list_skills/get_skill to access skills, list_agents/get_agent for agents, \
            chat_session_list/chat_session_get for sessions, and manage_tasks for task lifecycle, \
            session conversion, progress, and messaging operations."
                .to_string(),
        );
        info
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
                schema_for_type::<ListSkillsParams>(),
            ),
            Tool::new(
                "get_skill",
                "Get the full content of a skill by its ID. Returns the complete skill including its markdown content.",
                schema_for_type::<GetSkillParams>(),
            ),
            // No CLI needed: Deep reference lookup, use `skill show` for basic viewing
            Tool::new(
                "get_skill_reference",
                "Load the full content of a specific skill reference by skill_id and ref_id.",
                schema_for_type::<GetSkillReferenceParams>(),
            ),
            Tool::new(
                "list_agents",
                "List all available agents in RestFlow. Returns a summary of each agent including ID, name, model, and provider.",
                schema_for_type::<EmptyParams>(),
            ),
            Tool::new(
                "get_agent",
                "Get the full configuration of an agent by its ID. Returns the complete agent including model, prompt, temperature, and tools.",
                schema_for_type::<GetAgentParams>(),
            ),
            // No CLI needed: AI execution context only, use `skill show` for viewing
            Tool::new(
                "get_skill_context",
                "Fetch a skill's content with execution context (input, references). Use this when preparing to execute a skill task.",
                schema_for_type::<GetSkillContextParams>(),
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
                "manage_tasks",
                tools::impls::task::tool_description(),
                schema_map_from_value(tools::impls::task::tool_parameters_schema()),
            ),
        ];

        if let Ok(runtime_tools) = self.backend.list_runtime_tools().await {
            let mut known_names: HashSet<String> =
                tools.iter().map(|tool| tool.name.to_string()).collect();
            for runtime_tool in runtime_tools {
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

            // Append session-scoped tools (e.g. switch_model) only when running inside
            // an active agent session, NOT in standalone MCP mode.
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
            "list_skills" => {
                let params: ListSkillsParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_list_skills(params).await
            }
            "get_skill" => {
                let params: GetSkillParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_get_skill(params).await
            }
            "get_skill_reference" => {
                let params: GetSkillReferenceParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_get_skill_reference(params).await
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
            "get_skill_context" => {
                let params: GetSkillContextParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_get_skill_context(params).await
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
            "manage_tasks" => {
                let params: ManageTasksParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_manage_tasks(params).await
            }
            "switch_model" => {
                self.handle_switch_model_for_mcp(Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .await
            }
            _ => {
                self.handle_runtime_tool(
                    request.name.as_ref(),
                    Value::Object(request.arguments.unwrap_or_default()),
                )
                .await
            }
        };

        Ok(Self::to_call_tool_result(result))
    }
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;
