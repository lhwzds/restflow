//! Tool registry service for creating tool registries with storage access.

use crate::lsp::LspManager;
use crate::memory::UnifiedSearchEngine;
use crate::models::{
    AgentTaskStatus, BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSpec,
    BackgroundMessageSource, MemoryConfig, MemoryScope, MemorySearchQuery, SearchMode, SharedEntry,
    Skill, TaskSchedule, TerminalSession, ToolAction, TriggerConfig, UnifiedSearchQuery,
    Visibility,
};
use crate::registry::{
    GitHubProvider, MarketplaceProvider, SkillProvider as MarketplaceSkillProvider,
    SkillSearchQuery,
};
use crate::security::SecurityChecker;
use crate::storage::skill::SkillStorage;
use crate::storage::{
    AgentStorage, AgentTaskStorage, ChatSessionStorage, ConfigStorage, MemoryStorage,
    SecretStorage, SharedSpaceStorage, TerminalSessionStorage, TriggerStorage,
};
use chrono::Utc;
use restflow_ai::error::AiError;
use restflow_ai::tools::{
    AgentCreateRequest, AgentCrudTool, AgentStore, AgentUpdateRequest, ConfigTool, SecretsTool,
    TaskControlRequest, TaskCreateRequest, TaskMessageListRequest, TaskMessageRequest,
    TaskProgressRequest, TaskStore, TaskTool, TaskUpdateRequest,
};
use restflow_ai::{
    SecretResolver, SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillTool, SkillUpdate,
    Tool, ToolOutput, ToolRegistry, TranscribeTool, VisionTool,
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// SkillProvider implementation that reads from SkillStorage
pub struct SkillStorageProvider {
    storage: SkillStorage,
}

impl SkillStorageProvider {
    /// Create a new SkillStorageProvider
    pub fn new(storage: SkillStorage) -> Self {
        Self { storage }
    }
}

impl SkillProvider for SkillStorageProvider {
    fn list_skills(&self) -> Vec<SkillInfo> {
        match self.storage.list() {
            Ok(skills) => skills
                .into_iter()
                .map(|s| SkillInfo {
                    id: s.id,
                    name: s.name,
                    description: s.description,
                    tags: s.tags,
                })
                .collect(),
            Err(e) => {
                tracing::error!(error = %e, "Failed to list skills");
                Vec::new()
            }
        }
    }

    fn get_skill(&self, id: &str) -> Option<SkillContent> {
        match self.storage.get(id) {
            Ok(Some(skill)) => Some(SkillContent {
                id: skill.id,
                name: skill.name,
                content: skill.content,
            }),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(error = %e, skill_id = %id, "Failed to get skill");
                None
            }
        }
    }

    fn create_skill(&self, skill: SkillRecord) -> Result<SkillRecord, String> {
        let model = crate::models::Skill::new(
            skill.id.clone(),
            skill.name.clone(),
            skill.description.clone(),
            skill.tags.clone(),
            skill.content.clone(),
        );
        self.storage.create(&model).map_err(|e| e.to_string())?;
        Ok(skill)
    }

    fn update_skill(&self, id: &str, update: SkillUpdate) -> Result<SkillRecord, String> {
        let mut skill = self
            .storage
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Skill {} not found", id))?;

        skill.update(update.name, update.description, update.tags, update.content);
        self.storage.update(id, &skill).map_err(|e| e.to_string())?;

        Ok(SkillRecord {
            id: skill.id,
            name: skill.name,
            description: skill.description,
            tags: skill.tags,
            content: skill.content,
        })
    }

    fn delete_skill(&self, id: &str) -> Result<bool, String> {
        if !self.storage.exists(id).map_err(|e| e.to_string())? {
            return Ok(false);
        }
        self.storage.delete(id).map_err(|e| e.to_string())?;
        Ok(true)
    }

    fn export_skill(&self, id: &str) -> Result<String, String> {
        let skill = self
            .storage
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Skill {} not found", id))?;
        Ok(skill.to_markdown())
    }

    fn import_skill(
        &self,
        id: &str,
        markdown: &str,
        overwrite: bool,
    ) -> Result<SkillRecord, String> {
        let exists = self.storage.exists(id).map_err(|e| e.to_string())?;
        if exists && !overwrite {
            return Err(format!("Skill {} already exists", id));
        }

        let skill = crate::models::Skill::from_markdown(id, markdown).map_err(|e| e.to_string())?;

        if exists {
            self.storage.update(id, &skill).map_err(|e| e.to_string())?;
        } else {
            self.storage.create(&skill).map_err(|e| e.to_string())?;
        }

        Ok(SkillRecord {
            id: skill.id,
            name: skill.name,
            description: skill.description,
            tags: skill.tags,
            content: skill.content,
        })
    }
}

#[derive(Clone)]
struct AgentStoreAdapter {
    storage: AgentStorage,
}

impl AgentStoreAdapter {
    fn new(storage: AgentStorage) -> Self {
        Self { storage }
    }

    fn parse_agent_node(value: serde_json::Value) -> Result<crate::models::AgentNode, AiError> {
        serde_json::from_value(value)
            .map_err(|e| AiError::Tool(format!("Invalid agent payload: {}", e)))
    }
}

impl AgentStore for AgentStoreAdapter {
    fn list_agents(&self) -> restflow_ai::error::Result<serde_json::Value> {
        let agents = self
            .storage
            .list_agents()
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(agents).map_err(AiError::from)
    }

    fn get_agent(&self, id: &str) -> restflow_ai::error::Result<serde_json::Value> {
        let agent = self
            .storage
            .get_agent(id.to_string())
            .map_err(|e| AiError::Tool(e.to_string()))?
            .ok_or_else(|| AiError::Tool(format!("Agent {} not found", id)))?;
        serde_json::to_value(agent).map_err(AiError::from)
    }

    fn create_agent(
        &self,
        request: AgentCreateRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let agent = Self::parse_agent_node(request.agent)?;
        let created = self
            .storage
            .create_agent(request.name, agent)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(created).map_err(AiError::from)
    }

    fn update_agent(
        &self,
        request: AgentUpdateRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let agent = match request.agent {
            Some(value) => Some(Self::parse_agent_node(value)?),
            None => None,
        };
        let updated = self
            .storage
            .update_agent(request.id, request.name, agent)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(updated).map_err(AiError::from)
    }

    fn delete_agent(&self, id: &str) -> restflow_ai::error::Result<serde_json::Value> {
        self.storage
            .delete_agent(id.to_string())
            .map_err(|e| AiError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": true }))
    }
}

#[derive(Clone)]
struct TaskStoreAdapter {
    storage: AgentTaskStorage,
}

impl TaskStoreAdapter {
    fn new(storage: AgentTaskStorage) -> Self {
        Self { storage }
    }

    fn parse_status(status: &str) -> Result<AgentTaskStatus, AiError> {
        match status.trim().to_lowercase().as_str() {
            "active" => Ok(AgentTaskStatus::Active),
            "paused" => Ok(AgentTaskStatus::Paused),
            "running" => Ok(AgentTaskStatus::Running),
            "completed" => Ok(AgentTaskStatus::Completed),
            "failed" => Ok(AgentTaskStatus::Failed),
            _ => Err(AiError::Tool(format!("Unknown status: {}", status))),
        }
    }

    fn parse_control_action(action: &str) -> Result<BackgroundAgentControlAction, AiError> {
        match action.trim().to_lowercase().as_str() {
            "start" => Ok(BackgroundAgentControlAction::Start),
            "pause" => Ok(BackgroundAgentControlAction::Pause),
            "resume" => Ok(BackgroundAgentControlAction::Resume),
            "stop" => Ok(BackgroundAgentControlAction::Stop),
            "run_now" | "run-now" | "runnow" => Ok(BackgroundAgentControlAction::RunNow),
            _ => Err(AiError::Tool(format!("Unknown control action: {}", action))),
        }
    }

    fn parse_message_source(source: Option<&str>) -> Result<BackgroundMessageSource, AiError> {
        match source.map(|value| value.trim().to_lowercase()) {
            None => Ok(BackgroundMessageSource::User),
            Some(value) if value.is_empty() => Ok(BackgroundMessageSource::User),
            Some(value) if value == "user" => Ok(BackgroundMessageSource::User),
            Some(value) if value == "agent" => Ok(BackgroundMessageSource::Agent),
            Some(value) if value == "system" => Ok(BackgroundMessageSource::System),
            Some(value) => Err(AiError::Tool(format!("Unknown message source: {}", value))),
        }
    }

    fn parse_optional_value<T: DeserializeOwned>(
        field: &str,
        value: Option<serde_json::Value>,
    ) -> Result<Option<T>, AiError> {
        match value {
            Some(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(|e| AiError::Tool(format!("Invalid {}: {}", field, e))),
            None => Ok(None),
        }
    }

    fn parse_memory_scope(value: Option<&str>) -> Result<Option<MemoryScope>, AiError> {
        match value.map(|scope| scope.trim().to_lowercase()) {
            None => Ok(None),
            Some(scope) if scope.is_empty() => Ok(None),
            Some(scope) if scope == "shared_agent" => Ok(Some(MemoryScope::SharedAgent)),
            Some(scope) if scope == "per_task" => Ok(Some(MemoryScope::PerTask)),
            Some(scope) => Err(AiError::Tool(format!("Unknown memory_scope: {}", scope))),
        }
    }

    fn merge_memory_scope(
        memory: Option<MemoryConfig>,
        memory_scope: Option<String>,
    ) -> Result<Option<MemoryConfig>, AiError> {
        let parsed_scope = Self::parse_memory_scope(memory_scope.as_deref())?;
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
}

impl TaskStore for TaskStoreAdapter {
    fn create_task(
        &self,
        request: TaskCreateRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let schedule = Self::parse_optional_value::<TaskSchedule>("schedule", request.schedule)?
            .unwrap_or_default();
        let memory = Self::merge_memory_scope(None, request.memory_scope)?;
        let task = self
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: request.name,
                agent_id: request.agent_id,
                description: None,
                input: request.input,
                input_template: request.input_template,
                schedule,
                notification: None,
                execution_mode: None,
                memory,
            })
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(AiError::from)
    }

    fn update_task(
        &self,
        request: TaskUpdateRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let memory = Self::parse_optional_value("memory", request.memory)?;
        let memory = Self::merge_memory_scope(memory, request.memory_scope)?;
        let patch = BackgroundAgentPatch {
            name: request.name,
            description: request.description,
            agent_id: request.agent_id,
            input: request.input,
            input_template: request.input_template,
            schedule: Self::parse_optional_value("schedule", request.schedule)?,
            notification: Self::parse_optional_value("notification", request.notification)?,
            execution_mode: Self::parse_optional_value("execution_mode", request.execution_mode)?,
            memory,
        };

        let task = self
            .storage
            .update_background_agent(&request.id, patch)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(AiError::from)
    }

    fn delete_task(&self, id: &str) -> restflow_ai::error::Result<serde_json::Value> {
        let deleted = self
            .storage
            .delete_task(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": deleted }))
    }

    fn list_tasks(&self, status: Option<String>) -> restflow_ai::error::Result<serde_json::Value> {
        let tasks = if let Some(status) = status {
            let status = Self::parse_status(&status)?;
            self.storage
                .list_tasks_by_status(status)
                .map_err(|e| AiError::Tool(e.to_string()))?
        } else {
            self.storage
                .list_tasks()
                .map_err(|e| AiError::Tool(e.to_string()))?
        };

        serde_json::to_value(tasks).map_err(AiError::from)
    }

    fn control_task(
        &self,
        request: TaskControlRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let action = Self::parse_control_action(&request.action)?;
        let task = self
            .storage
            .control_background_agent(&request.id, action)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(AiError::from)
    }

    fn get_progress(
        &self,
        request: TaskProgressRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let progress = self
            .storage
            .get_background_agent_progress(&request.id, request.event_limit.unwrap_or(10).max(1))
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(progress).map_err(AiError::from)
    }

    fn send_message(
        &self,
        request: TaskMessageRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let source = Self::parse_message_source(request.source.as_deref())?;
        let message = self
            .storage
            .send_background_agent_message(&request.id, request.message, source)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(message).map_err(AiError::from)
    }

    fn list_messages(
        &self,
        request: TaskMessageListRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let messages = self
            .storage
            .list_background_agent_messages(&request.id, request.limit.unwrap_or(50).max(1))
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(messages).map_err(AiError::from)
    }
}

/// Create a tool registry with all available tools including storage-backed tools.
///
/// This function creates a registry with:
/// - Default tools from restflow-ai (http_request, run_python, send_email)
/// - SkillTool that can access skills from storage
/// - Memory search tool for unified memory and session search
#[allow(clippy::too_many_arguments)]
pub fn create_tool_registry(
    skill_storage: SkillStorage,
    memory_storage: MemoryStorage,
    chat_storage: ChatSessionStorage,
    shared_space_storage: SharedSpaceStorage,
    secret_storage: SecretStorage,
    config_storage: ConfigStorage,
    agent_storage: AgentStorage,
    agent_task_storage: AgentTaskStorage,
    trigger_storage: TriggerStorage,
    terminal_storage: TerminalSessionStorage,
    accessor_id: Option<String>,
) -> ToolRegistry {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let lsp_manager = Arc::new(LspManager::new(root));
    let mut registry = restflow_ai::tools::default_registry_with_diagnostics(lsp_manager);

    let secret_resolver: SecretResolver = {
        let secrets = Arc::new(secret_storage.clone());
        Arc::new(move |key| secrets.get_secret(key).ok().flatten())
    };

    registry.register(TranscribeTool::new(secret_resolver.clone()));
    registry.register(VisionTool::new(secret_resolver));

    // Add SkillTool with storage access
    let skill_provider = Arc::new(SkillStorageProvider::new(skill_storage.clone()));
    registry.register(SkillTool::new(skill_provider));

    // Add unified memory search tool
    let search_engine = UnifiedSearchEngine::new(memory_storage, chat_storage);
    registry.register(MemorySearchTool::new(search_engine));

    // Add shared space tool
    registry.register(SharedSpaceTool::new(shared_space_storage, accessor_id));

    // Add system management tools (read-only by default)
    registry.register(SecretsTool::new(Arc::new(secret_storage)));
    registry.register(ConfigTool::new(Arc::new(config_storage)));
    let agent_store = Arc::new(AgentStoreAdapter::new(agent_storage));
    registry.register(AgentCrudTool::new(agent_store).with_write(true));
    let task_store = Arc::new(TaskStoreAdapter::new(agent_task_storage));
    registry.register(TaskTool::new(task_store).with_write(true));
    registry.register(MarketplaceTool::new(skill_storage));
    registry.register(TriggerTool::new(trigger_storage));
    registry.register(TerminalTool::new(terminal_storage));
    registry.register(SecurityQueryTool::new());

    registry
}

#[derive(Clone)]
struct MemorySearchTool {
    engine: UnifiedSearchEngine,
}

impl MemorySearchTool {
    fn new(engine: UnifiedSearchEngine) -> Self {
        Self { engine }
    }
}

#[async_trait::async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search through long-term memory and chat session history"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keywords or phrase to search for"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to search within"
                },
                "include_sessions": {
                    "type": "boolean",
                    "description": "Whether to search chat sessions",
                    "default": true
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return",
                    "default": 5,
                    "minimum": 1
                },
                "offset": {
                    "type": "integer",
                    "description": "Offset for pagination",
                    "default": 0,
                    "minimum": 0
                }
            },
            "required": ["query", "agent_id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> restflow_ai::error::Result<ToolOutput> {
        let query = input
            .get("query")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AiError::Tool("Missing query parameter".to_string()))?;
        let agent_id = input
            .get("agent_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AiError::Tool("Missing agent_id parameter".to_string()))?;
        let include_sessions = input
            .get("include_sessions")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let limit = input
            .get("limit")
            .and_then(|value| value.as_u64())
            .unwrap_or(5)
            .min(u32::MAX as u64) as u32;
        let offset = input
            .get("offset")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;

        let base = MemorySearchQuery::new(agent_id.to_string())
            .with_query(query.to_string())
            .with_mode(SearchMode::Keyword)
            .paginate(limit, offset);
        let unified_query = UnifiedSearchQuery::new(base).with_sessions(include_sessions);

        let results = self
            .engine
            .search(&unified_query)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        Ok(ToolOutput::success(serde_json::to_value(results)?))
    }
}

#[derive(Clone)]
struct SharedSpaceTool {
    storage: SharedSpaceStorage,
    accessor_id: Option<String>,
}

impl SharedSpaceTool {
    fn new(storage: SharedSpaceStorage, accessor_id: Option<String>) -> Self {
        Self {
            storage,
            accessor_id,
        }
    }

    fn parse_visibility(value: &str) -> Visibility {
        match value {
            "private" => Visibility::Private,
            "shared" => Visibility::Shared,
            _ => Visibility::Public,
        }
    }
}

#[async_trait::async_trait]
impl Tool for SharedSpaceTool {
    fn name(&self) -> &str {
        "shared_space"
    }

    fn description(&self) -> &str {
        "Read and write entries in the shared space storage. Use namespace:name keys."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "set", "delete", "list"],
                    "description": "The action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "The key (namespace:name). Required for get/set/delete."
                },
                "value": {
                    "type": "string",
                    "description": "The value to store. Required for set."
                },
                "visibility": {
                    "type": "string",
                    "enum": ["public", "shared", "private"],
                    "description": "Access level for the entry"
                },
                "content_type": {
                    "type": "string",
                    "description": "Optional content type hint"
                },
                "type_hint": {
                    "type": "string",
                    "description": "Optional type hint for categorization"
                },
                "tags": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional tags for filtering"
                },
                "namespace": {
                    "type": "string",
                    "description": "For list: filter by namespace prefix"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> restflow_ai::error::Result<ToolOutput> {
        let action = input
            .get("action")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AiError::Tool("Missing action parameter".to_string()))?;
        let accessor_id = self.accessor_id.as_deref();

        match action {
            "get" => {
                let key = input
                    .get("key")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| AiError::Tool("Missing key parameter".to_string()))?;
                let entry = self
                    .storage
                    .get(key, accessor_id)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                let payload = match entry {
                    Some(entry) => json!({
                        "found": true,
                        "key": entry.key,
                        "value": entry.value,
                        "content_type": entry.content_type,
                        "visibility": entry.visibility,
                        "tags": entry.tags,
                        "updated_at": entry.updated_at
                    }),
                    None => json!({
                        "found": false,
                        "key": key
                    }),
                };
                Ok(ToolOutput::success(payload))
            }
            "set" => {
                let key = input
                    .get("key")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| AiError::Tool("Missing key parameter".to_string()))?;
                let value = input
                    .get("value")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| AiError::Tool("Missing value parameter".to_string()))?;

                let existing = self
                    .storage
                    .get_unchecked(key)
                    .map_err(|e| AiError::Tool(e.to_string()))?;

                if let Some(ref entry) = existing
                    && !entry.can_write(accessor_id)
                {
                    return Ok(ToolOutput::error(
                        "Access denied: cannot write to this entry",
                    ));
                }

                let visibility = input
                    .get("visibility")
                    .and_then(|value| value.as_str())
                    .map(Self::parse_visibility)
                    .or(existing.as_ref().map(|e| e.visibility))
                    .unwrap_or_default();

                let entry = SharedEntry {
                    key: key.to_string(),
                    value: value.to_string(),
                    visibility,
                    owner: existing
                        .as_ref()
                        .and_then(|e| e.owner.clone())
                        .or_else(|| self.accessor_id.clone()),
                    content_type: input
                        .get("content_type")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                        .or_else(|| existing.as_ref().and_then(|e| e.content_type.clone())),
                    type_hint: input
                        .get("type_hint")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                        .or_else(|| existing.as_ref().and_then(|e| e.type_hint.clone())),
                    tags: input
                        .get("tags")
                        .and_then(|value| value.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                                .collect::<Vec<String>>()
                        })
                        .or_else(|| existing.as_ref().map(|e| e.tags.clone()))
                        .unwrap_or_default(),
                    created_at: existing
                        .as_ref()
                        .map(|e| e.created_at)
                        .unwrap_or_else(|| Utc::now().timestamp_millis()),
                    updated_at: Utc::now().timestamp_millis(),
                    last_modified_by: self.accessor_id.clone(),
                };

                self.storage
                    .set(&entry)
                    .map_err(|e| AiError::Tool(e.to_string()))?;

                Ok(ToolOutput::success(json!({
                    "success": true,
                    "key": key,
                    "created": existing.is_none()
                })))
            }
            "delete" => {
                let key = input
                    .get("key")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| AiError::Tool("Missing key parameter".to_string()))?;
                let deleted = self
                    .storage
                    .delete(key, accessor_id)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(json!({
                    "deleted": deleted,
                    "key": key
                })))
            }
            "list" => {
                let namespace = input.get("namespace").and_then(|value| value.as_str());
                let prefix = namespace.map(|ns| format!("{}:", ns));
                let entries = self
                    .storage
                    .list(prefix.as_deref(), accessor_id)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                let items: Vec<_> = entries
                    .iter()
                    .map(|entry| {
                        let preview = if entry.value.len() > 100 {
                            format!("{}...", &entry.value[..100])
                        } else {
                            entry.value.clone()
                        };
                        json!({
                            "key": entry.key,
                            "content_type": entry.content_type,
                            "visibility": entry.visibility,
                            "tags": entry.tags,
                            "updated_at": entry.updated_at,
                            "preview": preview
                        })
                    })
                    .collect();
                Ok(ToolOutput::success(json!({
                    "count": items.len(),
                    "entries": items
                })))
            }
            _ => Ok(ToolOutput::error(format!("Unknown action: {}", action))),
        }
    }
}

#[derive(Clone)]
struct MarketplaceTool {
    storage: SkillStorage,
}

impl MarketplaceTool {
    fn new(storage: SkillStorage) -> Self {
        Self { storage }
    }

    fn provider_name(source: Option<&str>) -> &str {
        match source {
            Some("github") => "github",
            _ => "marketplace",
        }
    }

    async fn search_source(
        source: &str,
        query: &SkillSearchQuery,
    ) -> Result<Vec<crate::registry::SkillSearchResult>, AiError> {
        match source {
            "github" => GitHubProvider::new()
                .search(query)
                .await
                .map_err(|e| AiError::Tool(e.to_string())),
            _ => MarketplaceProvider::new()
                .search(query)
                .await
                .map_err(|e| AiError::Tool(e.to_string())),
        }
    }

    async fn get_manifest(source: &str, id: &str) -> Result<crate::models::SkillManifest, AiError> {
        match source {
            "github" => GitHubProvider::new()
                .get_manifest(id)
                .await
                .map_err(|e| AiError::Tool(e.to_string())),
            _ => MarketplaceProvider::new()
                .get_manifest(id)
                .await
                .map_err(|e| AiError::Tool(e.to_string())),
        }
    }

    async fn get_content(
        source: &str,
        id: &str,
        version: &crate::models::SkillVersion,
    ) -> Result<String, AiError> {
        match source {
            "github" => GitHubProvider::new()
                .get_content(id, version)
                .await
                .map_err(|e| AiError::Tool(e.to_string())),
            _ => MarketplaceProvider::new()
                .get_content(id, version)
                .await
                .map_err(|e| AiError::Tool(e.to_string())),
        }
    }

    fn manifest_to_skill(manifest: crate::models::SkillManifest, content: String) -> Skill {
        let now = Utc::now().timestamp_millis();
        Skill {
            id: manifest.id,
            name: manifest.name,
            description: manifest.description,
            tags: Some(manifest.keywords),
            content,
            folder_path: None,
            suggested_tools: Vec::new(),
            scripts: Vec::new(),
            references: Vec::new(),
            gating: None,
            version: Some(manifest.version.to_string()),
            author: manifest.author.map(|a| a.name),
            license: manifest.license,
            content_hash: None,
            storage_mode: crate::models::StorageMode::DatabaseOnly,
            is_synced: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum MarketplaceOperation {
    Search {
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        category: Option<String>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        #[serde(default)]
        author: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        source: Option<String>,
    },
    Info {
        id: String,
        #[serde(default)]
        source: Option<String>,
    },
    Install {
        id: String,
        #[serde(default)]
        source: Option<String>,
        #[serde(default)]
        overwrite: bool,
    },
    Uninstall {
        id: String,
    },
    ListInstalled,
}

#[async_trait::async_trait]
impl Tool for MarketplaceTool {
    fn name(&self) -> &str {
        "manage_marketplace"
    }

    fn description(&self) -> &str {
        "Search marketplace skills and install/uninstall them into local skill storage."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["search", "info", "install", "uninstall", "list_installed"]
                },
                "id": { "type": "string" },
                "query": { "type": "string" },
                "category": { "type": "string" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "author": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1 },
                "offset": { "type": "integer", "minimum": 0 },
                "source": { "type": "string", "enum": ["marketplace", "github"] },
                "overwrite": { "type": "boolean", "default": false }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> restflow_ai::error::Result<ToolOutput> {
        let operation: MarketplaceOperation = serde_json::from_value(input)?;
        match operation {
            MarketplaceOperation::Search {
                query,
                category,
                tags,
                author,
                limit,
                offset,
                source,
            } => {
                let query = SkillSearchQuery {
                    query,
                    category,
                    tags: tags.unwrap_or_default(),
                    author,
                    limit,
                    offset,
                    sort: None,
                };
                let source_name = Self::provider_name(source.as_deref());
                let results = Self::search_source(source_name, &query).await?;
                Ok(ToolOutput::success(serde_json::to_value(results)?))
            }
            MarketplaceOperation::Info { id, source } => {
                let source_name = Self::provider_name(source.as_deref());
                let manifest = Self::get_manifest(source_name, &id).await?;
                Ok(ToolOutput::success(serde_json::to_value(manifest)?))
            }
            MarketplaceOperation::Install {
                id,
                source,
                overwrite,
            } => {
                let source_name = Self::provider_name(source.as_deref());
                let manifest = Self::get_manifest(source_name, &id).await?;
                let content = Self::get_content(source_name, &id, &manifest.version).await?;
                let skill = Self::manifest_to_skill(manifest.clone(), content);

                let exists = self
                    .storage
                    .exists(&id)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                if exists && !overwrite {
                    return Ok(ToolOutput::error(
                        "Skill already installed. Set overwrite=true to replace.",
                    ));
                }

                if exists {
                    self.storage
                        .update(&id, &skill)
                        .map_err(|e| AiError::Tool(e.to_string()))?;
                } else {
                    self.storage
                        .create(&skill)
                        .map_err(|e| AiError::Tool(e.to_string()))?;
                }

                Ok(ToolOutput::success(json!({
                    "id": id,
                    "name": skill.name,
                    "version": skill.version,
                    "installed": true,
                    "updated": exists
                })))
            }
            MarketplaceOperation::Uninstall { id } => {
                let exists = self
                    .storage
                    .exists(&id)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                if exists {
                    self.storage
                        .delete(&id)
                        .map_err(|e| AiError::Tool(e.to_string()))?;
                }
                Ok(ToolOutput::success(json!({
                    "id": id,
                    "deleted": exists
                })))
            }
            MarketplaceOperation::ListInstalled => {
                let skills = self
                    .storage
                    .list()
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(serde_json::to_value(skills)?))
            }
        }
    }
}

struct TriggerTool {
    storage: TriggerStorage,
}

impl TriggerTool {
    fn new(storage: TriggerStorage) -> Self {
        Self { storage }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum TriggerOperation {
    Create {
        workflow_id: String,
        trigger_config: TriggerConfig,
        #[serde(default)]
        id: Option<String>,
    },
    List,
    Delete {
        id: String,
    },
    Enable {
        workflow_id: String,
        trigger_config: TriggerConfig,
        #[serde(default)]
        id: Option<String>,
    },
    Disable {
        id: String,
    },
}

#[async_trait::async_trait]
impl Tool for TriggerTool {
    fn name(&self) -> &str {
        "manage_triggers"
    }

    fn description(&self) -> &str {
        "Create/list/enable/disable workflow triggers."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "list", "delete", "enable", "disable"]
                },
                "id": { "type": "string" },
                "workflow_id": { "type": "string" },
                "trigger_config": {
                    "type": "object",
                    "description": "TriggerConfig payload with a `type` discriminator (manual/webhook/schedule)."
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> restflow_ai::error::Result<ToolOutput> {
        let operation: TriggerOperation = serde_json::from_value(input)?;
        match operation {
            TriggerOperation::Create {
                workflow_id,
                trigger_config,
                id,
            }
            | TriggerOperation::Enable {
                workflow_id,
                trigger_config,
                id,
            } => {
                let mut trigger = crate::models::ActiveTrigger::new(workflow_id, trigger_config);
                if let Some(id) = id {
                    trigger.id = id;
                }
                self.storage
                    .activate_trigger(&trigger)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(serde_json::to_value(trigger)?))
            }
            TriggerOperation::List => {
                let triggers = self
                    .storage
                    .list_active_triggers()
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(serde_json::to_value(triggers)?))
            }
            TriggerOperation::Delete { id } | TriggerOperation::Disable { id } => {
                self.storage
                    .deactivate_trigger(&id)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(json!({
                    "id": id,
                    "deleted": true
                })))
            }
        }
    }
}

#[derive(Clone)]
struct TerminalTool {
    storage: TerminalSessionStorage,
}

impl TerminalTool {
    fn new(storage: TerminalSessionStorage) -> Self {
        Self { storage }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum TerminalOperation {
    Create {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        working_directory: Option<String>,
        #[serde(default)]
        startup_command: Option<String>,
    },
    List,
    SendInput {
        session_id: String,
        data: String,
    },
    ReadOutput {
        session_id: String,
    },
    Close {
        session_id: String,
    },
}

#[async_trait::async_trait]
impl Tool for TerminalTool {
    fn name(&self) -> &str {
        "manage_terminal"
    }

    fn description(&self) -> &str {
        "Manage persistent terminal session metadata. Interactive PTY streaming is not available in this runtime."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "list", "send_input", "read_output", "close"]
                },
                "session_id": { "type": "string" },
                "name": { "type": "string" },
                "working_directory": { "type": "string" },
                "startup_command": { "type": "string" },
                "data": { "type": "string" }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> restflow_ai::error::Result<ToolOutput> {
        let operation: TerminalOperation = serde_json::from_value(input)?;
        match operation {
            TerminalOperation::Create {
                name,
                working_directory,
                startup_command,
            } => {
                let id = format!("terminal-{}", Uuid::new_v4());
                let default_name = self
                    .storage
                    .get_next_name()
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                let mut session = TerminalSession::new(id, name.unwrap_or(default_name));
                session.set_config(working_directory, startup_command);
                self.storage
                    .create(&session)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(serde_json::to_value(session)?))
            }
            TerminalOperation::List => {
                let sessions = self
                    .storage
                    .list()
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(serde_json::to_value(sessions)?))
            }
            TerminalOperation::SendInput { session_id, data } => {
                let mut session = self
                    .storage
                    .get(&session_id)
                    .map_err(|e| AiError::Tool(e.to_string()))?
                    .ok_or_else(|| {
                        AiError::Tool(format!("Terminal session not found: {}", session_id))
                    })?;

                let mut history = session.history.clone().unwrap_or_default();
                history.push_str(&format!("\n$ {}", data));
                session.update_history(history);
                self.storage
                    .update(&session_id, &session)
                    .map_err(|e| AiError::Tool(e.to_string()))?;

                Ok(ToolOutput::success(json!({
                    "session_id": session_id,
                    "accepted": true,
                    "live_runtime": false
                })))
            }
            TerminalOperation::ReadOutput { session_id } => {
                let session = self
                    .storage
                    .get(&session_id)
                    .map_err(|e| AiError::Tool(e.to_string()))?
                    .ok_or_else(|| {
                        AiError::Tool(format!("Terminal session not found: {}", session_id))
                    })?;
                Ok(ToolOutput::success(json!({
                    "session_id": session_id,
                    "output": session.history.unwrap_or_default(),
                    "live_runtime": false
                })))
            }
            TerminalOperation::Close { session_id } => {
                let mut session = self
                    .storage
                    .get(&session_id)
                    .map_err(|e| AiError::Tool(e.to_string()))?
                    .ok_or_else(|| {
                        AiError::Tool(format!("Terminal session not found: {}", session_id))
                    })?;
                session.status = crate::models::TerminalStatus::Stopped;
                session.stopped_at = Some(Utc::now().timestamp_millis());
                self.storage
                    .update(&session_id, &session)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(json!({
                    "session_id": session_id,
                    "closed": true
                })))
            }
        }
    }
}

#[derive(Clone, Default)]
struct SecurityQueryTool;

impl SecurityQueryTool {
    fn new() -> Self {
        Self
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum SecurityQueryOperation {
    CheckPermission {
        tool_name: String,
        operation_name: String,
        #[serde(default)]
        target: Option<String>,
        #[serde(default)]
        summary: Option<String>,
    },
    ListPermissions,
    ShowPolicy,
    RequestElevation {
        reason: String,
    },
}

#[async_trait::async_trait]
impl Tool for SecurityQueryTool {
    fn name(&self) -> &str {
        "security_query"
    }

    fn description(&self) -> &str {
        "Inspect default security policy and evaluate whether an action would require approval."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["check_permission", "list_permissions", "show_policy", "request_elevation"]
                },
                "tool_name": { "type": "string" },
                "operation_name": { "type": "string" },
                "target": { "type": "string" },
                "summary": { "type": "string" },
                "reason": { "type": "string" }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> restflow_ai::error::Result<ToolOutput> {
        let operation: SecurityQueryOperation = serde_json::from_value(input)?;
        match operation {
            SecurityQueryOperation::ShowPolicy => {
                let policy = crate::models::SecurityPolicy::default();
                Ok(ToolOutput::success(serde_json::to_value(policy)?))
            }
            SecurityQueryOperation::ListPermissions => {
                let policy = crate::models::SecurityPolicy::default();
                Ok(ToolOutput::success(json!({
                    "default_action": policy.default_action,
                    "allowlist_count": policy.allowlist.len(),
                    "blocklist_count": policy.blocklist.len(),
                    "approval_required_count": policy.approval_required.len(),
                    "tool_rule_count": policy.tool_rules.len()
                })))
            }
            SecurityQueryOperation::CheckPermission {
                tool_name,
                operation_name,
                target,
                summary,
            } => {
                let checker = SecurityChecker::with_defaults();
                let action = ToolAction {
                    tool_name: tool_name.clone(),
                    operation: operation_name.clone(),
                    target: target.unwrap_or_else(|| "*".to_string()),
                    summary: summary.unwrap_or_else(|| {
                        format!("{}:{}", tool_name.as_str(), operation_name.as_str())
                    }),
                };
                let ai_action = restflow_ai::ToolAction {
                    tool_name: action.tool_name.clone(),
                    operation: action.operation.clone(),
                    target: action.target.clone(),
                    summary: action.summary.clone(),
                };
                let decision = checker
                    .check_tool_action(&ai_action, Some("runtime"), Some("runtime"))
                    .await
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(json!({
                    "allowed": decision.allowed,
                    "requires_approval": decision.requires_approval,
                    "approval_id": decision.approval_id,
                    "reason": decision.reason
                })))
            }
            SecurityQueryOperation::RequestElevation { reason } => Ok(ToolOutput::error(format!(
                "Elevation requires human approval outside runtime tools: {}",
                reason
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::tempdir;

    fn setup_storage() -> (
        SkillStorage,
        MemoryStorage,
        ChatSessionStorage,
        SharedSpaceStorage,
        SecretStorage,
        ConfigStorage,
        AgentStorage,
        AgentTaskStorage,
        TriggerStorage,
        TerminalSessionStorage,
        tempfile::TempDir,
    ) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());

        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let previous_master_key = std::env::var_os("RESTFLOW_MASTER_KEY");
        unsafe {
            std::env::set_var("RESTFLOW_DIR", &state_dir);
            std::env::remove_var("RESTFLOW_MASTER_KEY");
        }

        let skill_storage = SkillStorage::new(db.clone()).unwrap();
        let memory_storage = MemoryStorage::new(db.clone()).unwrap();
        let chat_storage = ChatSessionStorage::new(db.clone()).unwrap();
        let shared_space_storage =
            SharedSpaceStorage::new(restflow_storage::SharedSpaceStorage::new(db.clone()).unwrap());
        let secret_storage = SecretStorage::with_config(
            db.clone(),
            restflow_storage::SecretStorageConfig {
                allow_insecure_file_permissions: true,
            },
        )
        .unwrap();
        let config_storage = ConfigStorage::new(db.clone()).unwrap();
        let agent_storage = AgentStorage::new(db.clone()).unwrap();
        let agent_task_storage = AgentTaskStorage::new(db.clone()).unwrap();
        let trigger_storage = TriggerStorage::new(db.clone()).unwrap();
        let terminal_storage = TerminalSessionStorage::new(db).unwrap();

        unsafe {
            std::env::remove_var("RESTFLOW_DIR");
            if let Some(value) = previous_master_key {
                std::env::set_var("RESTFLOW_MASTER_KEY", value);
            } else {
                std::env::remove_var("RESTFLOW_MASTER_KEY");
            }
        }
        (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            temp_dir,
        )
    }

    #[test]
    fn test_create_tool_registry() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();
        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            None,
        );

        // Should have default tools + skill tool
        assert!(registry.has("http_request"));
        assert!(registry.has("run_python"));
        assert!(registry.has("send_email"));
        assert!(registry.has("skill"));
        assert!(registry.has("memory_search"));
        assert!(registry.has("shared_space"));
        // New system management tools
        assert!(registry.has("manage_secrets"));
        assert!(registry.has("manage_config"));
        assert!(registry.has("manage_agents"));
        assert!(registry.has("manage_tasks"));
        assert!(registry.has("manage_marketplace"));
        assert!(registry.has("manage_triggers"));
        assert!(registry.has("manage_terminal"));
        assert!(registry.has("security_query"));
    }

    #[test]
    fn test_skill_provider_list_empty() {
        let (
            storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _secret_storage,
            _config_storage,
            _agent_storage,
            _agent_task_storage,
            _trigger_storage,
            _terminal_storage,
            _temp_dir,
        ) = setup_storage();
        let provider = SkillStorageProvider::new(storage);

        let skills = provider.list_skills();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_skill_provider_with_data() {
        let (
            storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _secret_storage,
            _config_storage,
            _agent_storage,
            _agent_task_storage,
            _trigger_storage,
            _terminal_storage,
            _temp_dir,
        ) = setup_storage();

        // Add a skill
        let skill = crate::models::Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            Some("A test".to_string()),
            Some(vec!["http_request".to_string()]),
            "# Test Content".to_string(),
        );
        storage.create(&skill).unwrap();

        let provider = SkillStorageProvider::new(storage);

        // Test list
        let skills = provider.list_skills();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "test-skill");

        // Test get
        let content = provider.get_skill("test-skill").unwrap();
        assert_eq!(content.id, "test-skill");
        assert!(content.content.contains("Test Content"));

        // Test get nonexistent
        assert!(provider.get_skill("nonexistent").is_none());
    }

    #[test]
    fn test_agent_store_adapter_crud_flow() {
        let (
            _skill_storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _secret_storage,
            _config_storage,
            agent_storage,
            _agent_task_storage,
            _trigger_storage,
            _terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let adapter = AgentStoreAdapter::new(agent_storage);
        let base_node = crate::models::AgentNode {
            model: Some(crate::models::AIModel::ClaudeSonnet4_5),
            prompt: Some("You are a testing assistant".to_string()),
            temperature: Some(0.3),
            api_key_config: Some(crate::models::ApiKeyConfig::Direct("test-key".to_string())),
            tools: Some(vec!["manage_tasks".to_string()]),
            skills: Some(vec!["ops-skill".to_string()]),
            skill_variables: None,
        };

        let created = AgentStore::create_agent(
            &adapter,
            AgentCreateRequest {
                name: "Ops Agent".to_string(),
                agent: serde_json::to_value(base_node).unwrap(),
            },
        )
        .unwrap();
        let agent_id = created
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap()
            .to_string();

        let listed = AgentStore::list_agents(&adapter).unwrap();
        assert_eq!(listed.as_array().map(|items| items.len()), Some(1));

        let fetched = AgentStore::get_agent(&adapter, &agent_id).unwrap();
        assert_eq!(
            fetched.get("name").and_then(|value| value.as_str()),
            Some("Ops Agent")
        );

        let updated = AgentStore::update_agent(
            &adapter,
            AgentUpdateRequest {
                id: agent_id.clone(),
                name: Some("Ops Agent Updated".to_string()),
                agent: Some(serde_json::json!({
                    "model": "gpt-5-mini",
                    "prompt": "Updated prompt",
                    "tools": ["manage_tasks", "manage_agents"],
                    "skills": ["ops-skill", "audit-skill"]
                })),
            },
        )
        .unwrap();
        assert_eq!(
            updated.get("name").and_then(|value| value.as_str()),
            Some("Ops Agent Updated")
        );
        assert_eq!(
            updated
                .get("agent")
                .and_then(|value| value.get("prompt"))
                .and_then(|value| value.as_str()),
            Some("Updated prompt")
        );

        let deleted = AgentStore::delete_agent(&adapter, &agent_id).unwrap();
        assert_eq!(
            deleted.get("deleted").and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_task_store_adapter_background_agent_flow() {
        let (
            _skill_storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _secret_storage,
            _config_storage,
            _agent_storage,
            agent_task_storage,
            _trigger_storage,
            _terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let adapter = TaskStoreAdapter::new(agent_task_storage);

        let created = TaskStore::create_task(
            &adapter,
            TaskCreateRequest {
                name: "Background Agent".to_string(),
                agent_id: "agent-001".to_string(),
                schedule: None,
                input: Some("Run periodic checks".to_string()),
                input_template: Some("Template {{task.id}}".to_string()),
                memory_scope: Some("per_task".to_string()),
            },
        )
        .unwrap();
        assert_eq!(
            created
                .get("input_template")
                .and_then(|value| value.as_str()),
            Some("Template {{task.id}}")
        );
        assert_eq!(
            created
                .get("memory")
                .and_then(|value| value.get("memory_scope"))
                .and_then(|value| value.as_str()),
            Some("per_task")
        );
        let task_id = created
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap()
            .to_string();

        let updated = TaskStore::update_task(
            &adapter,
            TaskUpdateRequest {
                id: task_id.clone(),
                name: Some("Background Agent Updated".to_string()),
                description: Some("Updated description".to_string()),
                agent_id: None,
                input: Some("Run checks and summarize".to_string()),
                input_template: Some("Updated {{task.name}}".to_string()),
                schedule: None,
                notification: None,
                execution_mode: None,
                memory: None,
                memory_scope: Some("shared_agent".to_string()),
            },
        )
        .unwrap();
        assert_eq!(
            updated.get("name").and_then(|value| value.as_str()),
            Some("Background Agent Updated")
        );
        assert_eq!(
            updated
                .get("memory")
                .and_then(|value| value.get("memory_scope"))
                .and_then(|value| value.as_str()),
            Some("shared_agent")
        );

        let controlled = TaskStore::control_task(
            &adapter,
            TaskControlRequest {
                id: task_id.clone(),
                action: "run_now".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            controlled.get("status").and_then(|value| value.as_str()),
            Some("active")
        );

        let message = TaskStore::send_message(
            &adapter,
            TaskMessageRequest {
                id: task_id.clone(),
                message: "Also check deployment logs".to_string(),
                source: Some("user".to_string()),
            },
        )
        .unwrap();
        assert_eq!(
            message.get("status").and_then(|value| value.as_str()),
            Some("queued")
        );

        let progress = TaskStore::get_progress(
            &adapter,
            TaskProgressRequest {
                id: task_id.clone(),
                event_limit: Some(5),
            },
        )
        .unwrap();
        assert_eq!(
            progress
                .get("background_agent_id")
                .and_then(|value| value.as_str()),
            Some(task_id.as_str())
        );

        let messages = TaskStore::list_messages(
            &adapter,
            TaskMessageListRequest {
                id: task_id.clone(),
                limit: Some(10),
            },
        )
        .unwrap();
        assert_eq!(messages.as_array().map(|items| items.len()), Some(1));

        let deleted = TaskStore::delete_task(&adapter, &task_id).unwrap();
        assert_eq!(
            deleted.get("deleted").and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_marketplace_tool_list_and_uninstall() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let local_skill = Skill::new(
            "local-skill".to_string(),
            "Local Skill".to_string(),
            Some("from test".to_string()),
            None,
            "# Local".to_string(),
        );
        skill_storage.create(&local_skill).unwrap();

        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            None,
        );

        let listed = registry
            .execute_safe(
                "manage_marketplace",
                json!({ "operation": "list_installed" }),
            )
            .await
            .unwrap();
        assert!(listed.success);
        assert_eq!(listed.result.as_array().map(|items| items.len()), Some(1));

        let deleted = registry
            .execute_safe(
                "manage_marketplace",
                json!({ "operation": "uninstall", "id": "local-skill" }),
            )
            .await
            .unwrap();
        assert!(deleted.success);
        assert_eq!(deleted.result["deleted"].as_bool(), Some(true));
    }

    #[tokio::test]
    async fn test_trigger_tool_create_list_disable() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            None,
        );

        let created = registry
            .execute_safe(
                "manage_triggers",
                json!({
                    "operation": "create",
                    "workflow_id": "wf-001",
                    "trigger_config": {
                        "type": "schedule",
                        "cron": "0 * * * * *",
                        "timezone": "UTC",
                        "payload": {"from": "test"}
                    }
                }),
            )
            .await
            .unwrap();
        assert!(created.success);
        let trigger_id = created.result["id"].as_str().unwrap().to_string();

        let listed = registry
            .execute_safe("manage_triggers", json!({ "operation": "list" }))
            .await
            .unwrap();
        assert!(listed.success);
        assert_eq!(listed.result.as_array().map(|items| items.len()), Some(1));

        let disabled = registry
            .execute_safe(
                "manage_triggers",
                json!({ "operation": "disable", "id": trigger_id }),
            )
            .await
            .unwrap();
        assert!(disabled.success);
    }

    #[tokio::test]
    async fn test_terminal_tool_create_send_read_close() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            None,
        );

        let created = registry
            .execute_safe(
                "manage_terminal",
                json!({
                    "operation": "create",
                    "name": "Agent Session",
                    "working_directory": "/tmp"
                }),
            )
            .await
            .unwrap();
        assert!(created.success);
        let session_id = created.result["id"].as_str().unwrap().to_string();

        let sent = registry
            .execute_safe(
                "manage_terminal",
                json!({
                    "operation": "send_input",
                    "session_id": session_id,
                    "data": "echo hello"
                }),
            )
            .await
            .unwrap();
        assert!(sent.success);
        let read = registry
            .execute_safe(
                "manage_terminal",
                json!({
                    "operation": "read_output",
                    "session_id": sent.result["session_id"].as_str().unwrap()
                }),
            )
            .await
            .unwrap();
        assert!(read.success);
        assert!(
            read.result["output"]
                .as_str()
                .unwrap_or_default()
                .contains("echo hello")
        );

        let closed = registry
            .execute_safe(
                "manage_terminal",
                json!({
                    "operation": "close",
                    "session_id": sent.result["session_id"].as_str().unwrap()
                }),
            )
            .await
            .unwrap();
        assert!(closed.success);
    }

    #[tokio::test]
    async fn test_security_query_tool_show_policy_and_check_permission() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_storage,
            agent_task_storage,
            trigger_storage,
            terminal_storage,
            None,
        );

        let summary = registry
            .execute_safe("security_query", json!({ "operation": "list_permissions" }))
            .await
            .unwrap();
        assert!(summary.success);
        assert!(summary.result["allowlist_count"].as_u64().unwrap_or(0) > 0);

        let check = registry
            .execute_safe(
                "security_query",
                json!({
                    "operation": "check_permission",
                    "tool_name": "manage_marketplace",
                    "operation_name": "install",
                    "target": "skill-id",
                    "summary": "Install skill"
                }),
            )
            .await
            .unwrap();
        assert!(check.success);
        assert!(check.result.get("allowed").is_some());
    }
}
