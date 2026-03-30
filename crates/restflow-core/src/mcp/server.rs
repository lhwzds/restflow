//! MCP server implementation for RestFlow
//!
//! This module provides an MCP server that exposes RestFlow's functionality
//! to AI assistants like Claude Code.

use crate::AppCore;
use crate::auth::build_runtime_api_keys;
use crate::daemon::{IpcClient, IpcRequest};
use crate::models::{
    BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSpec,
    BackgroundAgentStatus, BackgroundMessage, BackgroundMessageSource, BackgroundProgress,
    ChatSession, ChatSessionSummary, Deliverable, ExecutionContainerKind, ExecutionContainerRef,
    ExecutionSessionListQuery, ExecutionSessionSummary, ExecutionTraceCategory,
    ExecutionTraceEvent, ExecutionTraceQuery, ExecutionTraceSource, Hook, HookAction, HookEvent,
    HookFilter, MemoryChunk, MemorySearchQuery, MemorySearchResult, MemorySource, MemoryStats,
    ModelId, SearchMode, Skill, SkillStatus, ValidationError,
};
use crate::services::{
    operation_assessment::OperationAssessorAdapter,
    tool_registry::create_tool_registry_with_assessor,
};
use crate::storage::agent::StoredAgent;
use crate::storage::{SecretStorage, SystemConfig};
use restflow_ai::llm::{
    CodexClient, DefaultLlmClientFactory, LlmClient, LlmSwitcherImpl, SwappableLlm,
};
use restflow_ai::tools::Tool as RuntimeTool;
pub(crate) use restflow_contracts::ToolDefinition as RuntimeToolDefinition;
pub(crate) use restflow_contracts::ToolExecutionResult as RuntimeToolResult;
use restflow_storage::ApiDefaults;
use restflow_tools::SwitchModelTool;
use restflow_traits::store::{
    MANAGE_BACKGROUND_AGENT_OPERATIONS_CSV, MANAGE_BACKGROUND_AGENTS_TOOL_DESCRIPTION,
};
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
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use tokio::io::{stdin, stdout};
use tokio::sync::Mutex;

#[path = "server/agents.rs"]
mod agents;
#[path = "server/backends.rs"]
mod backends;
#[path = "server/background_agents.rs"]
mod background_agents;
#[path = "server/hooks.rs"]
mod hooks;
#[path = "server/memory.rs"]
mod memory;
#[path = "server/runtime_tools.rs"]
mod runtime_tools;
#[path = "server/sessions.rs"]
mod sessions;
#[path = "server/skills.rs"]
mod skills;
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
    async fn list_deliverables(&self, task_id: &str) -> Result<Vec<Deliverable>, String>;
    async fn list_execution_sessions(
        &self,
        query: ExecutionSessionListQuery,
    ) -> Result<Vec<ExecutionSessionSummary>, String>;

    async fn query_execution_traces(
        &self,
        query: ExecutionTraceQuery,
    ) -> Result<Vec<ExecutionTraceEvent>, String>;
    async fn query_execution_run_traces(
        &self,
        run_id: &str,
        limit: usize,
    ) -> Result<Vec<ExecutionTraceEvent>, String>;
    async fn get_background_agent(&self, id: &str) -> Result<BackgroundAgent, String>;

    async fn list_hooks(&self) -> Result<Vec<Hook>, String>;
    async fn create_hook(&self, hook: Hook) -> Result<Hook, String>;
    async fn update_hook(&self, id: &str, hook: Hook) -> Result<Hook, String>;
    async fn delete_hook(&self, id: &str) -> Result<bool, String>;
    async fn test_hook(&self, id: &str) -> Result<(), String>;

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
) -> anyhow::Result<restflow_ai::tools::ToolRegistry> {
    create_tool_registry_with_assessor(
        core.storage.skills.clone(),
        core.storage.memory.clone(),
        core.storage.chat_sessions.clone(),
        core.storage.channel_session_bindings.clone(),
        core.storage.execution_traces.clone(),
        core.storage.kv_store.clone(),
        core.storage.work_items.clone(),
        core.storage.secrets.clone(),
        core.storage.config.clone(),
        core.storage.agents.clone(),
        core.storage.background_agents.clone(),
        core.storage.triggers.clone(),
        core.storage.terminal_sessions.clone(),
        core.storage.deliverables.clone(),
        None,
        None,
        None,
        Some(Arc::new(OperationAssessorAdapter::new(core.clone()))),
    )
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
    fn execution_trace_event_name(event: &ExecutionTraceEvent) -> &'static str {
        match event.category {
            ExecutionTraceCategory::Lifecycle => match event
                .lifecycle
                .as_ref()
                .map(|lifecycle| lifecycle.status.as_str())
            {
                Some("started") => "turn_started",
                Some("completed") => "turn_completed",
                Some("failed") => "turn_failed",
                Some("interrupted") => "turn_interrupted",
                _ => "lifecycle",
            },
            ExecutionTraceCategory::ToolCall => match event.tool_call.as_ref().map(|t| t.phase) {
                Some(crate::models::ToolCallPhase::Started) => "tool_call_started",
                Some(crate::models::ToolCallPhase::Completed) => "tool_call_completed",
                None => "tool_call",
            },
            ExecutionTraceCategory::LlmCall => "llm_call",
            ExecutionTraceCategory::ModelSwitch => "model_switch",
            ExecutionTraceCategory::Message => "message",
            ExecutionTraceCategory::MetricSample => "metric_sample",
            ExecutionTraceCategory::ProviderHealth => "provider_health",
            ExecutionTraceCategory::LogRecord => "log_record",
        }
    }

    fn execution_trace_category_name(event: &ExecutionTraceEvent) -> &'static str {
        match event.category {
            ExecutionTraceCategory::ToolCall => "tool",
            ExecutionTraceCategory::Lifecycle => "turn",
            ExecutionTraceCategory::LlmCall => "llm",
            ExecutionTraceCategory::ModelSwitch => "model",
            ExecutionTraceCategory::Message => "message",
            ExecutionTraceCategory::MetricSample => "metric",
            ExecutionTraceCategory::ProviderHealth => "provider_health",
            ExecutionTraceCategory::LogRecord => "log",
        }
    }

    fn parse_trace_category(value: Option<String>) -> Result<Option<String>, String> {
        match value.map(|s| s.trim().to_lowercase()) {
            None => Ok(None),
            Some(s) if s.is_empty() => Ok(None),
            Some(s)
                if matches!(
                    s.as_str(),
                    "turn"
                        | "tool"
                        | "llm"
                        | "model"
                        | "message"
                        | "metric"
                        | "provider_health"
                        | "log"
                        | "turn_started"
                        | "tool_call_started"
                        | "tool_call_completed"
                        | "turn_completed"
                        | "turn_failed"
                        | "turn_interrupted"
                        | "llm_call"
                        | "model_switch"
                        | "metric_sample"
                        | "log_record"
                ) =>
            {
                Ok(Some(s))
            }
            Some(s) => Err(format!(
                "Unknown trace category: {}. Supported: turn, tool, llm, model, message, metric, provider_health, log, turn_started, tool_call_started, tool_call_completed, turn_completed, turn_failed, turn_interrupted, llm_call, model_switch, metric_sample, log_record",
                s
            )),
        }
    }

    fn normalize_optional_filter(value: Option<String>) -> Option<String> {
        value
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
    }

    fn validate_trace_time_range(
        from_time_ms: Option<i64>,
        to_time_ms: Option<i64>,
    ) -> Result<(), String> {
        if let (Some(from), Some(to)) = (from_time_ms, to_time_ms)
            && from > to
        {
            return Err(
                "Invalid time range: from_time_ms must be less than or equal to to_time_ms"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn trace_matches_category(trace: &ExecutionTraceEvent, category: Option<&str>) -> bool {
        let Some(category) = category else {
            return true;
        };

        match category {
            "turn" => Self::execution_trace_category_name(trace) == "turn",
            "tool" => Self::execution_trace_category_name(trace) == "tool",
            "llm" => Self::execution_trace_category_name(trace) == "llm",
            "model" => Self::execution_trace_category_name(trace) == "model",
            "message" => Self::execution_trace_category_name(trace) == "message",
            "metric" => Self::execution_trace_category_name(trace) == "metric",
            "provider_health" => Self::execution_trace_category_name(trace) == "provider_health",
            "log" => Self::execution_trace_category_name(trace) == "log",
            event_type => Self::execution_trace_event_name(trace) == event_type,
        }
    }

    fn trace_matches_source(trace: &ExecutionTraceEvent, source: Option<&str>) -> bool {
        let Some(source) = source else {
            return true;
        };

        match source {
            "agent_executor" => trace.source == ExecutionTraceSource::AgentExecutor,
            "runtime" => trace.source == ExecutionTraceSource::Runtime,
            "mcp_server" => trace.source == ExecutionTraceSource::McpServer,
            "cli" => trace.source == ExecutionTraceSource::Cli,
            "telemetry" => trace.source == ExecutionTraceSource::Telemetry,
            other => trace
                .tool_call
                .as_ref()
                .map(|tool_call| tool_call.tool_name.trim().eq_ignore_ascii_case(other))
                .unwrap_or(false),
        }
    }

    fn trace_matches_time_range(
        trace: &ExecutionTraceEvent,
        from_time_ms: Option<i64>,
        to_time_ms: Option<i64>,
    ) -> bool {
        if let Some(from) = from_time_ms
            && trace.timestamp < from
        {
            return false;
        }
        if let Some(to) = to_time_ms
            && trace.timestamp > to
        {
            return false;
        }
        true
    }

    fn build_trace_stats(
        traces: &[ExecutionTraceEvent],
        limit: usize,
        offset: usize,
        tasks_scanned: usize,
        sessions_scanned: usize,
        from_time_ms: Option<i64>,
        to_time_ms: Option<i64>,
    ) -> Value {
        let mut by_event_type: BTreeMap<String, u64> = BTreeMap::new();
        let mut by_category: BTreeMap<String, u64> = BTreeMap::new();
        let mut by_tool: BTreeMap<String, u64> = BTreeMap::new();
        let mut success_true = 0u64;
        let mut success_false = 0u64;
        let mut success_unknown = 0u64;
        let mut duration_total = 0u64;
        let mut duration_count = 0u64;
        let mut duration_min: Option<u64> = None;
        let mut duration_max: Option<u64> = None;
        let mut created_at_min: Option<i64> = None;
        let mut created_at_max: Option<i64> = None;

        for trace in traces {
            let event_name = Self::execution_trace_event_name(trace).to_string();
            *by_event_type.entry(event_name).or_insert(0) += 1;

            let category_name = Self::execution_trace_category_name(trace).to_string();
            *by_category.entry(category_name).or_insert(0) += 1;

            if let Some(tool_name) = trace
                .tool_call
                .as_ref()
                .map(|tool_call| tool_call.tool_name.trim())
                .filter(|name| !name.is_empty())
            {
                *by_tool.entry(tool_name.to_string()).or_insert(0) += 1;
            }

            match trace
                .tool_call
                .as_ref()
                .and_then(|tool_call| tool_call.success)
            {
                Some(true) => success_true += 1,
                Some(false) => success_false += 1,
                None => success_unknown += 1,
            }

            let duration_ms = trace
                .tool_call
                .as_ref()
                .and_then(|tool_call| tool_call.duration_ms)
                .or_else(|| {
                    trace
                        .llm_call
                        .as_ref()
                        .and_then(|llm_call| llm_call.duration_ms)
                })
                .and_then(|value| u64::try_from(value).ok());
            if let Some(duration_ms) = duration_ms {
                duration_total = duration_total.saturating_add(duration_ms);
                duration_count += 1;
                duration_min = Some(duration_min.map_or(duration_ms, |v| v.min(duration_ms)));
                duration_max = Some(duration_max.map_or(duration_ms, |v| v.max(duration_ms)));
            }

            created_at_min =
                Some(created_at_min.map_or(trace.timestamp, |v| v.min(trace.timestamp)));
            created_at_max =
                Some(created_at_max.map_or(trace.timestamp, |v| v.max(trace.timestamp)));
        }

        let duration_avg = if duration_count > 0 {
            Some(duration_total as f64 / duration_count as f64)
        } else {
            None
        };

        serde_json::json!({
            "total": traces.len(),
            "limit": limit,
            "offset": offset,
            "tasks_scanned": tasks_scanned,
            "sessions_scanned": sessions_scanned,
            "time_range": {
                "from_time_ms": from_time_ms,
                "to_time_ms": to_time_ms,
                "matched_min_created_at": created_at_min,
                "matched_max_created_at": created_at_max,
            },
            "by_event_type": by_event_type,
            "by_category": by_category,
            "by_tool": by_tool,
            "success": {
                "true": success_true,
                "false": success_false,
                "unknown": success_unknown,
            },
            "duration_ms": {
                "count": duration_count,
                "min": duration_min,
                "max": duration_max,
                "avg": duration_avg,
            }
        })
    }

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
            Some(s) if s == "interrupted" => Ok(Some(BackgroundAgentStatus::Interrupted)),
            Some(s) => Err(format!("Unknown status: {}", s)),
        }
    }

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

    fn runtime_alias_target(name: &str) -> Option<&'static str> {
        match name {
            "http" => Some("http_request"),
            "email" => Some("send_email"),
            "telegram" => Some("telegram_send"),
            "discord" => Some("discord_send"),
            "slack" => Some("slack_send"),
            "use_skill" => Some("skill"),
            "python" => Some("run_python"),
            _ => None,
        }
    }

    fn runtime_alias_description(alias_name: &str, target_name: &str) -> String {
        match (alias_name, target_name) {
            ("http", "http_request") => {
                "Alias of 'http_request' for convenience. Prefer using 'http_request' directly."
                    .to_string()
            }
            ("email", "send_email") => {
                "Alias of 'send_email' for convenience. Prefer using 'send_email' directly."
                    .to_string()
            }
            ("telegram", "telegram_send") => {
                "Alias of 'telegram_send' for convenience. Prefer using 'telegram_send' directly."
                    .to_string()
            }
            ("discord", "discord_send") => {
                "Alias of 'discord_send' for convenience. Prefer using 'discord_send' directly."
                    .to_string()
            }
            ("slack", "slack_send") => {
                "Alias of 'slack_send' for convenience. Prefer using 'slack_send' directly."
                    .to_string()
            }
            ("use_skill", "skill") => {
                "Alias of 'skill' for backward compatibility (load-only: list/read). Prefer using 'skill' directly."
                    .to_string()
            }
            ("python", "run_python") => {
                "Alias of 'run_python' for backward compatibility. Prefer using 'run_python' directly."
                    .to_string()
            }
            _ => format!("Alias of '{}' for backward compatibility.", target_name),
        }
    }

    fn convert_use_skill_input(input: Value) -> Value {
        let Value::Object(mut map) = input else {
            return serde_json::json!({ "action": "list" });
        };

        if let Some(action) = map.get("action").and_then(|v| v.as_str()) {
            let action = action.trim();
            if action.eq_ignore_ascii_case("execute") || action.eq_ignore_ascii_case("run") {
                return serde_json::json!({ "action": "__unsupported_execute" });
            }
            if action.eq_ignore_ascii_case("list") {
                return serde_json::json!({ "action": "list" });
            }
            if action.eq_ignore_ascii_case("read") || action.eq_ignore_ascii_case("load") {
                if let Some(skill_id) = map.remove("id").or_else(|| map.remove("skill_id")) {
                    return serde_json::json!({
                        "action": "read",
                        "id": skill_id
                    });
                }
                return serde_json::json!({ "action": "read" });
            }

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

    fn use_skill_alias_parameters() -> serde_json::Map<String, Value> {
        let schema = serde_json::json!({
            "type": "object",
            "description": "Load-only alias for skill access. Supports only list/read. Skill execution is not supported in this tool.",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "read"],
                    "description": "Load-only action."
                },
                "id": {
                    "type": "string",
                    "description": "Skill ID for read."
                },
                "skill_id": {
                    "type": "string",
                    "description": "Legacy compatibility field for read."
                },
                "list": {
                    "type": "boolean",
                    "default": false,
                    "description": "Legacy compatibility field for list."
                }
            },
            "additionalProperties": false
        });
        schema
            .as_object()
            .cloned()
            .unwrap_or_else(serde_json::Map::new)
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
                and manage_background_agents for background agent lifecycle, session conversion, progress, and messaging operations."
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
                "List all available agents in RestFlow. Returns a summary of each agent including ID, name, model, and provider.",
                schema_for_type::<EmptyParams>(),
            ),
            Tool::new(
                "get_agent",
                "Get the full configuration of an agent by its ID. Returns the complete agent including model, prompt, temperature, and tools.",
                schema_for_type::<GetAgentParams>(),
            ),
            Tool::new(
                "memory_search",
                "Search memory chunks using keyword matching. Returns raw chunks. Use this for broad keyword searches across memory.",
                schema_for_type::<MemorySearchParams>(),
            ),
            Tool::new(
                "memory_store",
                "Store a new memory chunk for an agent. Use this for raw memory storage without title/tags structure.",
                schema_for_type::<MemoryStoreParams>(),
            ),
            Tool::new(
                "memory_stats",
                "Get memory statistics for an agent.",
                schema_for_type::<MemoryStatsParams>(),
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
                "manage_background_agents",
                MANAGE_BACKGROUND_AGENTS_TOOL_DESCRIPTION,
                schema_map_from_value(
                    restflow_tools::impls::background_agent::tool_parameters_schema(),
                ),
            ),
            Tool::new(
                "manage_hooks",
                "Create, list, update, delete, and test lifecycle hooks. Hooks trigger actions (webhook, script, send_message, run_task) when events occur (task_started, task_completed, task_failed, task_interrupted, tool_executed, approval_required).",
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
                ("discord", "discord_send"),
                ("slack", "slack_send"),
                ("use_skill", "skill"),
                ("python", "run_python"),
            ] {
                if !known_names.contains(alias_name)
                    && let Some(target) = runtime_by_name.get(target_name)
                {
                    let parameters = if alias_name == "use_skill" {
                        Self::use_skill_alias_parameters()
                    } else {
                        match target.parameters.clone() {
                            Value::Object(map) => map,
                            _ => serde_json::Map::new(),
                        }
                    };
                    tools.push(Tool::new(
                        alias_name,
                        Self::runtime_alias_description(alias_name, target_name),
                        parameters,
                    ));
                    known_names.insert(alias_name.to_string());
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
            "use_skill" => {
                let converted = Self::convert_use_skill_input(Value::Object(
                    request.arguments.unwrap_or_default(),
                ));
                if converted
                    .get("action")
                    .and_then(|value| value.as_str())
                    .map(|action| action == "__unsupported_execute")
                    .unwrap_or(false)
                {
                    Err("skill execution not supported in this tool. use_skill is load-only; use action=list/read.".to_string())
                } else {
                    self.handle_runtime_tool("skill", converted).await
                }
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

        Ok(Self::to_call_tool_result(result))
    }
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;
