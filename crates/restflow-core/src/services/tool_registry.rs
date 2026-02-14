//! Tool registry service for creating tool registries with storage access.

use crate::daemon::{DaemonStatus, check_daemon_status, check_health};
use crate::lsp::LspManager;
use crate::memory::{MemoryExporter, UnifiedSearchEngine};
use crate::models::{
    BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSchedule,
    BackgroundAgentSpec, BackgroundAgentStatus, BackgroundMessageSource, MemoryConfig, MemoryScope,
    MemorySearchQuery, NoteQuery, NoteStatus, ResourceLimits, SearchMode, SharedEntry, Skill,
    TerminalSession, ToolAction, TriggerConfig, UnifiedSearchQuery, Visibility, WorkspaceNote,
    WorkspaceNotePatch as CoreWorkspaceNotePatch, WorkspaceNoteSpec as CoreWorkspaceNoteSpec,
};
use crate::registry::{
    GitHubProvider, MarketplaceProvider, SkillProvider as MarketplaceSkillProvider,
    SkillSearchQuery,
};
use crate::security::SecurityChecker;
use crate::storage::skill::SkillStorage;
use crate::storage::{
    AgentStorage, BackgroundAgentStorage, ChatSessionStorage, ConfigStorage, MemoryStorage,
    SecretStorage, SharedSpaceStorage, TerminalSessionStorage, TriggerStorage,
    WorkspaceNoteStorage,
};
use crate::tools::ops::{ManageOpsOperation, build_response, parse_operation};
use chrono::Utc;
use restflow_ai::error::AiError;
use restflow_ai::tools::{
    AgentCreateRequest, AgentCrudTool, AgentStore, AgentUpdateRequest, AuthProfileCreateRequest,
    AuthProfileStore, AuthProfileTestRequest, AuthProfileTool, BackgroundAgentControlRequest,
    BackgroundAgentCreateRequest, BackgroundAgentMessageListRequest, BackgroundAgentMessageRequest,
    BackgroundAgentProgressRequest, BackgroundAgentStore, BackgroundAgentTool,
    BackgroundAgentUpdateRequest, ConfigTool, MemoryClearRequest, MemoryCompactRequest,
    MemoryExportRequest, MemoryManagementTool, MemoryManager, MemoryStore, SecretsTool,
    SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore, SessionTool,
    WorkspaceNotePatch, WorkspaceNoteProvider, WorkspaceNoteQuery, WorkspaceNoteRecord,
    WorkspaceNoteSpec, WorkspaceNoteStatus, WorkspaceNoteTool,
};
use restflow_ai::tools::{DeleteMemoryTool, ListMemoryTool, ReadMemoryTool, SaveMemoryTool};
use restflow_ai::{
    SecretResolver, SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillTool, SkillUpdate,
    Tool, ToolOutput, ToolRegistry, TranscribeTool, VisionTool,
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
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
    skills: SkillStorage,
    secrets: SecretStorage,
    background_agent_storage: BackgroundAgentStorage,
    known_tools: Arc<RwLock<HashSet<String>>>,
}

impl AgentStoreAdapter {
    fn new(
        storage: AgentStorage,
        skills: SkillStorage,
        secrets: SecretStorage,
        background_agent_storage: BackgroundAgentStorage,
        known_tools: Arc<RwLock<HashSet<String>>>,
    ) -> Self {
        Self {
            storage,
            skills,
            secrets,
            background_agent_storage,
            known_tools,
        }
    }

    fn parse_agent_node(value: serde_json::Value) -> Result<crate::models::AgentNode, AiError> {
        serde_json::from_value(value)
            .map_err(|e| AiError::Tool(format!("Invalid agent payload: {}", e)))
    }

    fn validate_agent_node(&self, agent: &crate::models::AgentNode) -> Result<(), AiError> {
        if let Err(errors) = agent.validate() {
            return Err(AiError::Tool(crate::models::encode_validation_error(
                errors,
            )));
        }

        let mut errors = Vec::new();
        if let Some(tools) = &agent.tools {
            for tool_name in tools {
                let normalized = tool_name.trim();
                if normalized.is_empty() {
                    errors.push(crate::models::ValidationError::new(
                        "tools",
                        "tool name must not be empty",
                    ));
                    continue;
                }
                let is_known = self
                    .known_tools
                    .read()
                    .map(|set| set.contains(normalized))
                    .unwrap_or(false);
                if !is_known {
                    errors.push(crate::models::ValidationError::new(
                        "tools",
                        format!("unknown tool: {}", normalized),
                    ));
                }
            }
        }

        if let Some(skills) = &agent.skills {
            for skill_id in skills {
                let normalized = skill_id.trim();
                if normalized.is_empty() {
                    errors.push(crate::models::ValidationError::new(
                        "skills",
                        "skill ID must not be empty",
                    ));
                    continue;
                }
                match self.skills.exists(normalized) {
                    Ok(true) => {}
                    Ok(false) => errors.push(crate::models::ValidationError::new(
                        "skills",
                        format!("unknown skill: {}", normalized),
                    )),
                    Err(err) => errors.push(crate::models::ValidationError::new(
                        "skills",
                        format!("failed to verify skill '{}': {}", normalized, err),
                    )),
                }
            }
        }

        if let Some(crate::models::ApiKeyConfig::Secret(secret_name)) = &agent.api_key_config {
            let normalized = secret_name.trim();
            if !normalized.is_empty() {
                match self.secrets.has_secret(normalized) {
                    Ok(true) => {}
                    Ok(false) => errors.push(crate::models::ValidationError::new(
                        "api_key_config",
                        format!("secret not found: {}", normalized),
                    )),
                    Err(err) => errors.push(crate::models::ValidationError::new(
                        "api_key_config",
                        format!("failed to verify secret '{}': {}", normalized, err),
                    )),
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(AiError::Tool(crate::models::encode_validation_error(
                errors,
            )))
        }
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
        self.validate_agent_node(&agent)?;
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
            Some(value) => {
                let node = Self::parse_agent_node(value)?;
                self.validate_agent_node(&node)?;
                Some(node)
            }
            None => None,
        };
        let updated = self
            .storage
            .update_agent(request.id, request.name, agent)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(updated).map_err(AiError::from)
    }

    fn delete_agent(&self, id: &str) -> restflow_ai::error::Result<serde_json::Value> {
        let active_tasks = self
            .background_agent_storage
            .list_active_tasks_by_agent_id(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        if !active_tasks.is_empty() {
            let task_names = active_tasks
                .iter()
                .map(|task| task.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(AiError::Tool(format!(
                "Cannot delete agent {}: active background tasks exist ({})",
                id, task_names
            )));
        }

        self.storage
            .delete_agent(id.to_string())
            .map_err(|e| AiError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": true }))
    }
}

#[derive(Clone)]
struct BackgroundAgentStoreAdapter {
    storage: BackgroundAgentStorage,
    agent_storage: AgentStorage,
}

impl BackgroundAgentStoreAdapter {
    fn new(storage: BackgroundAgentStorage, agent_storage: AgentStorage) -> Self {
        Self {
            storage,
            agent_storage,
        }
    }

    fn parse_status(status: &str) -> Result<BackgroundAgentStatus, AiError> {
        match status.trim().to_lowercase().as_str() {
            "active" => Ok(BackgroundAgentStatus::Active),
            "paused" => Ok(BackgroundAgentStatus::Paused),
            "running" => Ok(BackgroundAgentStatus::Running),
            "completed" => Ok(BackgroundAgentStatus::Completed),
            "failed" => Ok(BackgroundAgentStatus::Failed),
            "interrupted" => Ok(BackgroundAgentStatus::Interrupted),
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
            Some(scope) if scope == "per_background_agent" => {
                Ok(Some(MemoryScope::PerBackgroundAgent))
            }
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

    fn resolve_agent_id(&self, id_or_prefix: &str) -> Result<String, AiError> {
        self.agent_storage
            .resolve_existing_agent_id(id_or_prefix)
            .map_err(|e| AiError::Tool(e.to_string()))
    }
}

impl BackgroundAgentStore for BackgroundAgentStoreAdapter {
    fn create_background_agent(
        &self,
        request: BackgroundAgentCreateRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let resolved_agent_id = self.resolve_agent_id(&request.agent_id)?;
        let schedule =
            Self::parse_optional_value::<BackgroundAgentSchedule>("schedule", request.schedule)?
                .unwrap_or_default();
        let memory = Self::parse_optional_value("memory", request.memory)?;
        let memory = Self::merge_memory_scope(memory, request.memory_scope)?;
        let resource_limits: Option<ResourceLimits> =
            Self::parse_optional_value("resource_limits", request.resource_limits)?;
        let task = self
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: request.name,
                agent_id: resolved_agent_id,
                description: None,
                input: request.input,
                input_template: request.input_template,
                schedule,
                notification: None,
                execution_mode: None,
                timeout_secs: request.timeout_secs,
                memory,
                resource_limits,
            })
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(AiError::from)
    }

    fn update_background_agent(
        &self,
        request: BackgroundAgentUpdateRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let resolved_agent_id = request
            .agent_id
            .as_deref()
            .map(|id| self.resolve_agent_id(id))
            .transpose()?;
        let memory = Self::parse_optional_value("memory", request.memory)?;
        let memory = Self::merge_memory_scope(memory, request.memory_scope)?;
        let resource_limits: Option<ResourceLimits> =
            Self::parse_optional_value("resource_limits", request.resource_limits)?;
        let patch = BackgroundAgentPatch {
            name: request.name,
            description: request.description,
            agent_id: resolved_agent_id,
            input: request.input,
            input_template: request.input_template,
            schedule: Self::parse_optional_value("schedule", request.schedule)?,
            notification: Self::parse_optional_value("notification", request.notification)?,
            execution_mode: Self::parse_optional_value("execution_mode", request.execution_mode)?,
            timeout_secs: request.timeout_secs,
            memory,
            resource_limits,
        };

        let task = self
            .storage
            .update_background_agent(&request.id, patch)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(AiError::from)
    }

    fn delete_background_agent(&self, id: &str) -> restflow_ai::error::Result<serde_json::Value> {
        let deleted = self
            .storage
            .delete_task(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": deleted }))
    }

    fn list_background_agents(
        &self,
        status: Option<String>,
    ) -> restflow_ai::error::Result<serde_json::Value> {
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

    fn control_background_agent(
        &self,
        request: BackgroundAgentControlRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let action = Self::parse_control_action(&request.action)?;
        let task = self
            .storage
            .control_background_agent(&request.id, action)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(AiError::from)
    }

    fn get_background_agent_progress(
        &self,
        request: BackgroundAgentProgressRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let progress = self
            .storage
            .get_background_agent_progress(&request.id, request.event_limit.unwrap_or(10).max(1))
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(progress).map_err(AiError::from)
    }

    fn send_background_agent_message(
        &self,
        request: BackgroundAgentMessageRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let source = Self::parse_message_source(request.source.as_deref())?;
        let message = self
            .storage
            .send_background_agent_message(&request.id, request.message, source)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(message).map_err(AiError::from)
    }

    fn list_background_agent_messages(
        &self,
        request: BackgroundAgentMessageListRequest,
    ) -> restflow_ai::error::Result<serde_json::Value> {
        let messages = self
            .storage
            .list_background_agent_messages(&request.id, request.limit.unwrap_or(50).max(1))
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(messages).map_err(AiError::from)
    }
}

// ============== Session Storage Adapter ==============

#[derive(Clone)]
struct SessionStorageAdapter {
    storage: ChatSessionStorage,
    agent_storage: AgentStorage,
}

impl SessionStorageAdapter {
    fn new(storage: ChatSessionStorage, agent_storage: AgentStorage) -> Self {
        Self {
            storage,
            agent_storage,
        }
    }
}

impl SessionStore for SessionStorageAdapter {
    fn list_sessions(&self, filter: SessionListFilter) -> restflow_ai::error::Result<Value> {
        let sessions = if let Some(agent_id) = &filter.agent_id {
            self.storage
                .list_by_agent(agent_id)
                .map_err(|e| AiError::Tool(e.to_string()))?
        } else if let Some(skill_id) = &filter.skill_id {
            self.storage
                .list_by_skill(skill_id)
                .map_err(|e| AiError::Tool(e.to_string()))?
        } else {
            self.storage
                .list()
                .map_err(|e| AiError::Tool(e.to_string()))?
        };

        if filter.include_messages.unwrap_or(false) {
            serde_json::to_value(sessions).map_err(AiError::from)
        } else {
            let summaries = self
                .storage
                .list_summaries()
                .map_err(|e| AiError::Tool(e.to_string()))?;
            serde_json::to_value(summaries).map_err(AiError::from)
        }
    }

    fn get_session(&self, id: &str) -> restflow_ai::error::Result<Value> {
        let session = self
            .storage
            .get(id)
            .map_err(|e| AiError::Tool(e.to_string()))?
            .ok_or_else(|| AiError::Tool(format!("Session {} not found", id)))?;
        serde_json::to_value(session).map_err(AiError::from)
    }

    fn create_session(&self, request: SessionCreateRequest) -> restflow_ai::error::Result<Value> {
        let resolved_agent_id = self
            .agent_storage
            .resolve_existing_agent_id(&request.agent_id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        let mut session = crate::models::ChatSession::new(resolved_agent_id, request.model);
        if let Some(name) = request.name {
            session = session.with_name(name);
        }
        if let Some(skill_id) = request.skill_id {
            session = session.with_skill(skill_id);
        }
        if let Some(retention) = request.retention {
            session = session.with_retention(retention);
        }
        self.storage
            .create(&session)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(session).map_err(AiError::from)
    }

    fn delete_session(&self, id: &str) -> restflow_ai::error::Result<Value> {
        let deleted = self
            .storage
            .delete(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": deleted }))
    }

    fn search_sessions(&self, query: SessionSearchQuery) -> restflow_ai::error::Result<Value> {
        // Search by iterating sessions and filtering by query string
        let sessions = if let Some(agent_id) = &query.agent_id {
            self.storage
                .list_by_agent(agent_id)
                .map_err(|e| AiError::Tool(e.to_string()))?
        } else {
            self.storage
                .list()
                .map_err(|e| AiError::Tool(e.to_string()))?
        };

        let keyword = query.query.to_lowercase();
        let limit = query.limit.unwrap_or(20) as usize;

        let matched: Vec<_> = sessions
            .into_iter()
            .filter(|s| {
                let name_match = s.name.to_lowercase().contains(&keyword);
                let msg_match = s
                    .messages
                    .iter()
                    .any(|m| m.content.to_lowercase().contains(&keyword));
                name_match || msg_match
            })
            .take(limit)
            .collect();

        serde_json::to_value(matched).map_err(AiError::from)
    }

    fn cleanup_sessions(&self) -> restflow_ai::error::Result<Value> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let stats = self
            .storage
            .cleanup_by_session_retention(now_ms)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(stats).map_err(AiError::from)
    }
}

// ============== Memory Manager Adapter ==============

#[derive(Clone)]
struct MemoryManagerAdapter {
    storage: MemoryStorage,
}

impl MemoryManagerAdapter {
    fn new(storage: MemoryStorage) -> Self {
        Self { storage }
    }
}

impl MemoryManager for MemoryManagerAdapter {
    fn stats(&self, agent_id: &str) -> restflow_ai::error::Result<Value> {
        let stats = self
            .storage
            .get_stats(agent_id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(stats).map_err(AiError::from)
    }

    fn export(&self, request: MemoryExportRequest) -> restflow_ai::error::Result<Value> {
        let exporter = MemoryExporter::new(self.storage.clone());
        let result = if let Some(session_id) = &request.session_id {
            exporter
                .export_session(session_id)
                .map_err(|e| AiError::Tool(e.to_string()))?
        } else {
            exporter
                .export_agent(&request.agent_id)
                .map_err(|e| AiError::Tool(e.to_string()))?
        };
        serde_json::to_value(result).map_err(AiError::from)
    }

    fn clear(&self, request: MemoryClearRequest) -> restflow_ai::error::Result<Value> {
        if let Some(session_id) = &request.session_id {
            let delete_chunks = request.delete_sessions.unwrap_or(true);
            let deleted = self
                .storage
                .delete_session(session_id, delete_chunks)
                .map_err(|e| AiError::Tool(e.to_string()))?;
            Ok(json!({
                "agent_id": request.agent_id,
                "session_id": session_id,
                "deleted": deleted
            }))
        } else {
            let deleted = self
                .storage
                .delete_chunks_for_agent(&request.agent_id)
                .map_err(|e| AiError::Tool(e.to_string()))?;
            Ok(json!({
                "agent_id": request.agent_id,
                "chunks_deleted": deleted
            }))
        }
    }

    fn compact(&self, request: MemoryCompactRequest) -> restflow_ai::error::Result<Value> {
        let chunks = self
            .storage
            .list_chunks(&request.agent_id)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        let keep_recent = request.keep_recent.unwrap_or(10) as usize;
        let before_ms = request.before_ms;

        let mut to_delete: Vec<String> = Vec::new();

        if chunks.len() > keep_recent {
            // Sort by created_at ascending, delete oldest first
            let mut sorted = chunks.clone();
            sorted.sort_by_key(|c| c.created_at);

            let removable = sorted.len() - keep_recent;
            for chunk in sorted.into_iter().take(removable) {
                if let Some(threshold) = before_ms {
                    if chunk.created_at < threshold {
                        to_delete.push(chunk.id.clone());
                    }
                } else {
                    to_delete.push(chunk.id.clone());
                }
            }
        }

        let deleted_count = to_delete.len();
        for chunk_id in &to_delete {
            self.storage
                .delete_chunk(chunk_id)
                .map_err(|e| AiError::Tool(e.to_string()))?;
        }

        Ok(json!({
            "agent_id": request.agent_id,
            "total_chunks": chunks.len(),
            "deleted": deleted_count,
            "remaining": chunks.len() - deleted_count
        }))
    }
}

// ============== Auth Profile Storage Adapter ==============

#[derive(Clone)]
struct AuthProfileStorageAdapter {
    storage: SecretStorage,
}

impl AuthProfileStorageAdapter {
    fn new(storage: SecretStorage) -> Self {
        Self { storage }
    }
}

impl AuthProfileStore for AuthProfileStorageAdapter {
    fn list_profiles(&self) -> restflow_ai::error::Result<Value> {
        let secrets = self
            .storage
            .list_secrets()
            .map_err(|e| AiError::Tool(e.to_string()))?;

        let profiles: Vec<Value> = secrets
            .iter()
            .filter(|s| s.key.ends_with("_API_KEY") || s.key.ends_with("_TOKEN"))
            .map(|s| {
                // list_secrets() clears values for security, use has_secret to check
                let has_value = self.storage.has_secret(&s.key).unwrap_or(false);
                json!({
                    "id": s.key,
                    "name": s.key,
                    "has_credential": has_value,
                    "description": s.description
                })
            })
            .collect();

        Ok(json!(profiles))
    }

    fn discover_profiles(&self) -> restflow_ai::error::Result<Value> {
        let known_vars = [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "GROQ_API_KEY",
            "DEEPSEEK_API_KEY",
            "OPENROUTER_API_KEY",
            "XAI_API_KEY",
            "GITHUB_TOKEN",
        ];

        let discovered: Vec<Value> = known_vars
            .iter()
            .filter_map(|var| {
                std::env::var(var).ok().map(|_| {
                    json!({
                        "env_var": var,
                        "available": true
                    })
                })
            })
            .collect();

        Ok(json!({
            "total": discovered.len(),
            "profiles": discovered
        }))
    }

    fn add_profile(&self, request: AuthProfileCreateRequest) -> restflow_ai::error::Result<Value> {
        let key_name = format!(
            "{}_API_KEY",
            request.provider.to_uppercase().replace('-', "_")
        );
        let secret_value = match &request.credential {
            restflow_ai::tools::CredentialInput::ApiKey { key, .. } => key.clone(),
            restflow_ai::tools::CredentialInput::Token { token, .. } => token.clone(),
            restflow_ai::tools::CredentialInput::OAuth { access_token, .. } => access_token.clone(),
        };
        self.storage
            .set_secret(
                &key_name,
                &secret_value,
                Some(format!("Auth profile: {}", request.name)),
            )
            .map_err(|e| AiError::Tool(e.to_string()))?;

        Ok(json!({
            "id": key_name,
            "name": request.name,
            "provider": request.provider,
            "created": true
        }))
    }

    fn remove_profile(&self, id: &str) -> restflow_ai::error::Result<Value> {
        self.storage
            .delete_secret(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "removed": true }))
    }

    fn test_profile(&self, request: AuthProfileTestRequest) -> restflow_ai::error::Result<Value> {
        if let Some(id) = &request.id {
            let available = self
                .storage
                .get_secret(id)
                .ok()
                .flatten()
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            Ok(json!({
                "id": id,
                "available": available
            }))
        } else if let Some(provider) = &request.provider {
            let key_name = format!("{}_API_KEY", provider.to_uppercase().replace('-', "_"));
            let available = self
                .storage
                .get_secret(&key_name)
                .ok()
                .flatten()
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            Ok(json!({
                "provider": provider,
                "key_name": key_name,
                "available": available
            }))
        } else {
            Ok(json!({ "available": false, "reason": "No id or provider specified" }))
        }
    }
}

#[derive(Clone)]
struct DbWorkspaceNoteAdapter {
    storage: WorkspaceNoteStorage,
}

impl DbWorkspaceNoteAdapter {
    fn new(storage: WorkspaceNoteStorage) -> Self {
        Self { storage }
    }
}

fn to_tool_note_status(status: NoteStatus) -> WorkspaceNoteStatus {
    match status {
        NoteStatus::Open => WorkspaceNoteStatus::Open,
        NoteStatus::InProgress => WorkspaceNoteStatus::InProgress,
        NoteStatus::Done => WorkspaceNoteStatus::Done,
        NoteStatus::Archived => WorkspaceNoteStatus::Archived,
    }
}

fn to_core_note_status(status: WorkspaceNoteStatus) -> NoteStatus {
    match status {
        WorkspaceNoteStatus::Open => NoteStatus::Open,
        WorkspaceNoteStatus::InProgress => NoteStatus::InProgress,
        WorkspaceNoteStatus::Done => NoteStatus::Done,
        WorkspaceNoteStatus::Archived => NoteStatus::Archived,
    }
}

fn to_tool_note(note: WorkspaceNote) -> WorkspaceNoteRecord {
    WorkspaceNoteRecord {
        id: note.id,
        folder: note.folder,
        title: note.title,
        content: note.content,
        priority: note.priority,
        status: to_tool_note_status(note.status),
        tags: note.tags,
        assignee: note.assignee,
        created_at: note.created_at,
        updated_at: note.updated_at,
    }
}

impl WorkspaceNoteProvider for DbWorkspaceNoteAdapter {
    fn create(&self, spec: WorkspaceNoteSpec) -> std::result::Result<WorkspaceNoteRecord, String> {
        self.storage
            .create_note(CoreWorkspaceNoteSpec {
                folder: spec.folder,
                title: spec.title,
                content: spec.content,
                priority: spec.priority,
                tags: spec.tags,
            })
            .map(to_tool_note)
            .map_err(|e| e.to_string())
    }

    fn get(&self, id: &str) -> std::result::Result<Option<WorkspaceNoteRecord>, String> {
        self.storage
            .get_note(id)
            .map(|note| note.map(to_tool_note))
            .map_err(|e| e.to_string())
    }

    fn update(
        &self,
        id: &str,
        patch: WorkspaceNotePatch,
    ) -> std::result::Result<WorkspaceNoteRecord, String> {
        self.storage
            .update_note(
                id,
                CoreWorkspaceNotePatch {
                    title: patch.title,
                    content: patch.content,
                    priority: patch.priority,
                    status: patch.status.map(to_core_note_status),
                    tags: patch.tags,
                    assignee: patch.assignee,
                    folder: patch.folder,
                },
            )
            .map(to_tool_note)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, id: &str) -> std::result::Result<bool, String> {
        match self.storage.get_note(id) {
            Ok(None) => Ok(false),
            Ok(Some(_)) => self
                .storage
                .delete_note(id)
                .map(|_| true)
                .map_err(|e| e.to_string()),
            Err(err) => Err(err.to_string()),
        }
    }

    fn list(
        &self,
        query: WorkspaceNoteQuery,
    ) -> std::result::Result<Vec<WorkspaceNoteRecord>, String> {
        self.storage
            .list_notes(NoteQuery {
                folder: query.folder,
                status: query.status.map(to_core_note_status),
                priority: query.priority,
                tag: query.tag,
                assignee: query.assignee,
                search: query.search,
            })
            .map(|notes| notes.into_iter().map(to_tool_note).collect())
            .map_err(|e| e.to_string())
    }

    fn list_folders(&self) -> std::result::Result<Vec<String>, String> {
        self.storage.list_folders().map_err(|e| e.to_string())
    }
}

// ============== DB Memory Store Adapter ==============

/// Database-backed implementation of MemoryStore.
///
/// Stores memories as MemoryChunks in the redb database, enabling interoperability
/// with memory_search and manage_memory tools. Title is stored as a `__title:{value}` tag.
#[derive(Clone)]
struct DbMemoryStoreAdapter {
    storage: MemoryStorage,
}

impl DbMemoryStoreAdapter {
    fn new(storage: MemoryStorage) -> Self {
        Self { storage }
    }

    /// Extract title from tags (stored as `__title:{value}`)
    fn extract_title(tags: &[String]) -> String {
        tags.iter()
            .find(|t| t.starts_with("__title:"))
            .map(|t| t.trim_start_matches("__title:").to_string())
            .unwrap_or_default()
    }

    /// Build tags list: prepend __title tag, then user tags
    fn build_tags(title: &str, user_tags: &[String]) -> Vec<String> {
        let mut tags = vec![format!("__title:{}", title)];
        tags.extend(user_tags.iter().cloned());
        tags
    }

    /// Filter out internal __title tags from user-visible output
    fn user_tags(tags: &[String]) -> Vec<String> {
        tags.iter()
            .filter(|t| !t.starts_with("__title:"))
            .cloned()
            .collect()
    }

    /// Format a MemoryChunk as a memory entry JSON (matching file memory output)
    fn chunk_to_entry_json(chunk: &crate::models::memory::MemoryChunk) -> Value {
        let title = Self::extract_title(&chunk.tags);
        let user_tags = Self::user_tags(&chunk.tags);
        json!({
            "id": chunk.id,
            "title": title,
            "content": chunk.content,
            "tags": user_tags,
            "created_at": chrono::DateTime::from_timestamp_millis(chunk.created_at)
                .unwrap_or_default()
                .to_rfc3339(),
            "updated_at": chrono::DateTime::from_timestamp_millis(chunk.created_at)
                .unwrap_or_default()
                .to_rfc3339(),
            "agent_id": chunk.agent_id,
            "session_id": chunk.session_id,
        })
    }

    /// Format a MemoryChunk as metadata-only JSON (for list operations)
    fn chunk_to_meta_json(chunk: &crate::models::memory::MemoryChunk) -> Value {
        let title = Self::extract_title(&chunk.tags);
        let user_tags = Self::user_tags(&chunk.tags);
        json!({
            "id": chunk.id,
            "title": title,
            "tags": user_tags,
            "created_at": chrono::DateTime::from_timestamp_millis(chunk.created_at)
                .unwrap_or_default()
                .to_rfc3339(),
            "updated_at": chrono::DateTime::from_timestamp_millis(chunk.created_at)
                .unwrap_or_default()
                .to_rfc3339(),
        })
    }
}

impl MemoryStore for DbMemoryStoreAdapter {
    fn save(
        &self,
        agent_id: &str,
        title: &str,
        content: &str,
        tags: &[String],
    ) -> restflow_ai::error::Result<Value> {
        use crate::models::memory::MemorySource;

        let db_tags = Self::build_tags(title, tags);
        let chunk =
            crate::models::memory::MemoryChunk::new(agent_id.to_string(), content.to_string())
                .with_tags(db_tags)
                .with_source(MemorySource::AgentGenerated {
                    tool_name: "save_to_memory".to_string(),
                });

        let stored_id = self
            .storage
            .store_chunk(&chunk)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        // If stored_id differs from chunk.id, content was a duplicate
        let is_dedup = stored_id != chunk.id;
        let message = if is_dedup {
            "Duplicate content, returning existing memory"
        } else {
            "Memory saved successfully"
        };

        Ok(json!({
            "success": true,
            "id": stored_id,
            "title": title,
            "message": message
        }))
    }

    fn read_by_id(&self, id: &str) -> restflow_ai::error::Result<Option<Value>> {
        let chunk = self
            .storage
            .get_chunk(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        match chunk {
            Some(c) => {
                let entry = Self::chunk_to_entry_json(&c);
                Ok(Some(json!({
                    "found": true,
                    "entry": entry
                })))
            }
            None => Ok(None),
        }
    }

    fn search(
        &self,
        agent_id: &str,
        tag: Option<&str>,
        search: Option<&str>,
        limit: usize,
    ) -> restflow_ai::error::Result<Value> {
        let mut chunks = self
            .storage
            .list_chunks(agent_id)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        // Filter by user tag (case-insensitive contains)
        if let Some(tag_filter) = tag {
            let tag_lower = tag_filter.to_lowercase();
            chunks.retain(|c| {
                Self::user_tags(&c.tags)
                    .iter()
                    .any(|t| t.to_lowercase().contains(&tag_lower))
            });
        }

        // Filter by title keyword (case-insensitive contains)
        if let Some(search_text) = search {
            let search_lower = search_text.to_lowercase();
            chunks.retain(|c| {
                Self::extract_title(&c.tags)
                    .to_lowercase()
                    .contains(&search_lower)
            });
        }

        // Already sorted by created_at desc from list_chunks
        chunks.truncate(limit);

        let results: Vec<Value> = chunks.iter().map(Self::chunk_to_meta_json).collect();

        Ok(json!({
            "count": results.len(),
            "memories": results
        }))
    }

    fn list(
        &self,
        agent_id: &str,
        tag: Option<&str>,
        limit: usize,
    ) -> restflow_ai::error::Result<Value> {
        let chunks = self
            .storage
            .list_chunks(agent_id)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        let total = chunks.len();
        let mut filtered = chunks;

        // Filter by user tag (case-insensitive contains)
        if let Some(tag_filter) = tag {
            let tag_lower = tag_filter.to_lowercase();
            filtered.retain(|c| {
                Self::user_tags(&c.tags)
                    .iter()
                    .any(|t| t.to_lowercase().contains(&tag_lower))
            });
        }

        filtered.truncate(limit);

        let results: Vec<Value> = filtered.iter().map(Self::chunk_to_meta_json).collect();

        Ok(json!({
            "total": total,
            "count": results.len(),
            "memories": results
        }))
    }

    fn delete(&self, id: &str) -> restflow_ai::error::Result<Value> {
        let deleted = self
            .storage
            .delete_chunk(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        if deleted {
            Ok(json!({
                "deleted": true,
                "id": id,
                "message": "Memory deleted successfully"
            }))
        } else {
            Ok(json!({
                "deleted": false,
                "message": format!("No memory found with ID: {}", id)
            }))
        }
    }
}

/// Create a tool registry with all available tools including storage-backed tools.
///
/// This function creates a registry with:
/// - Default tools from restflow-ai (http_request, send_email)
/// - SkillTool that can access skills from storage
/// - Memory search tool for unified memory and session search
/// - Agent memory CRUD tools (save_to_memory, read_memory, etc.) — always registered, agent_id is a tool input
#[allow(clippy::too_many_arguments)]
pub fn create_tool_registry(
    skill_storage: SkillStorage,
    memory_storage: MemoryStorage,
    chat_storage: ChatSessionStorage,
    shared_space_storage: SharedSpaceStorage,
    workspace_note_storage: WorkspaceNoteStorage,
    secret_storage: SecretStorage,
    config_storage: ConfigStorage,
    agent_storage: AgentStorage,
    background_agent_storage: BackgroundAgentStorage,
    trigger_storage: TriggerStorage,
    terminal_storage: TerminalSessionStorage,
    accessor_id: Option<String>,
    _agent_id: Option<String>,
) -> ToolRegistry {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let lsp_manager = Arc::new(LspManager::new(root));
    let mut registry = restflow_ai::tools::default_registry_with_diagnostics(lsp_manager);

    let secret_resolver: SecretResolver = {
        let secrets = Arc::new(secret_storage.clone());
        Arc::new(move |key| secrets.get_secret(key).ok().flatten())
    };

    registry.register(TranscribeTool::new(secret_resolver.clone()));
    registry.register(VisionTool::new(secret_resolver.clone()));
    // Re-register WebSearchTool with secret resolver so Brave/Tavily API keys are available
    registry
        .register(restflow_ai::tools::WebSearchTool::new().with_secret_resolver(secret_resolver));

    // Add SkillTool with storage access
    let skill_provider = Arc::new(SkillStorageProvider::new(skill_storage.clone()));
    registry.register(SkillTool::new(skill_provider));

    // Session management tool (clone before move)
    let session_store = Arc::new(SessionStorageAdapter::new(
        chat_storage.clone(),
        agent_storage.clone(),
    ));
    registry.register(SessionTool::new(session_store).with_write(true));

    // Memory management tool (clone before move)
    let memory_manager = Arc::new(MemoryManagerAdapter::new(memory_storage.clone()));
    registry.register(MemoryManagementTool::new(memory_manager).with_write(true));

    // Agent memory CRUD tools (save_to_memory, read_memory, list_memories, delete_memory)
    // Always registered — agent_id is provided as a tool input parameter
    {
        let mem_store: Arc<dyn MemoryStore> =
            Arc::new(DbMemoryStoreAdapter::new(memory_storage.clone()));
        registry.register(SaveMemoryTool::new(mem_store.clone()));
        registry.register(ReadMemoryTool::new(mem_store.clone()));
        registry.register(ListMemoryTool::new(mem_store.clone()));
        registry.register(DeleteMemoryTool::new(mem_store));
    }

    // Add unified memory search tool
    let search_engine = UnifiedSearchEngine::new(memory_storage, chat_storage.clone());
    registry.register(MemorySearchTool::new(search_engine));
    registry.register(ManageOpsTool::new(
        background_agent_storage.clone(),
        chat_storage.clone(),
    ));

    // Add shared space tool
    registry.register(SharedSpaceTool::new(shared_space_storage, accessor_id));

    // Add workspace note tool
    let workspace_note_provider = Arc::new(DbWorkspaceNoteAdapter::new(workspace_note_storage));
    registry.register(WorkspaceNoteTool::new(workspace_note_provider).with_write(true));

    // Auth profile management tool (clone before move)
    let auth_store = Arc::new(AuthProfileStorageAdapter::new(secret_storage.clone()));
    registry.register(AuthProfileTool::new(auth_store).with_write(true));

    // Add system management tools (read-only by default)
    registry.register(SecretsTool::new(Arc::new(secret_storage.clone())));
    registry.register(ConfigTool::new(Arc::new(config_storage)));
    let known_tools = Arc::new(RwLock::new(HashSet::new()));
    let agent_store = Arc::new(AgentStoreAdapter::new(
        agent_storage.clone(),
        skill_storage.clone(),
        secret_storage.clone(),
        background_agent_storage.clone(),
        known_tools.clone(),
    ));
    registry.register(AgentCrudTool::new(agent_store).with_write(true));
    let background_agent_store = Arc::new(BackgroundAgentStoreAdapter::new(
        background_agent_storage,
        agent_storage,
    ));
    registry.register(BackgroundAgentTool::new(background_agent_store).with_write(true));
    registry.register(MarketplaceTool::new(skill_storage));
    registry.register(TriggerTool::new(trigger_storage));
    registry.register(TerminalTool::new(terminal_storage));
    registry.register(SecurityQueryTool::new());
    if let Ok(mut known) = known_tools.write() {
        *known = registry
            .list()
            .into_iter()
            .map(|name| name.to_string())
            .collect::<HashSet<_>>();
    }

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
struct ManageOpsTool {
    background_storage: BackgroundAgentStorage,
    chat_storage: ChatSessionStorage,
}

impl ManageOpsTool {
    fn new(background_storage: BackgroundAgentStorage, chat_storage: ChatSessionStorage) -> Self {
        Self {
            background_storage,
            chat_storage,
        }
    }

    fn parse_status_filter(
        input: &Value,
    ) -> restflow_ai::error::Result<Option<BackgroundAgentStatus>> {
        let Some(status) = input.get("status").and_then(Value::as_str) else {
            return Ok(None);
        };
        let parsed = match status.trim().to_ascii_lowercase().as_str() {
            "active" => BackgroundAgentStatus::Active,
            "paused" => BackgroundAgentStatus::Paused,
            "running" => BackgroundAgentStatus::Running,
            "completed" => BackgroundAgentStatus::Completed,
            "failed" => BackgroundAgentStatus::Failed,
            "interrupted" => BackgroundAgentStatus::Interrupted,
            value => {
                return Err(AiError::Tool(format!(
                    "Unknown status: {}. Supported: active, paused, running, completed, failed, interrupted",
                    value
                )));
            }
        };
        Ok(Some(parsed))
    }

    fn parse_limit(input: &Value, key: &str, default: usize, max: usize) -> usize {
        input
            .get(key)
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(default)
            .clamp(1, max)
    }

    fn canonical_existing_ancestor(path: &Path) -> anyhow::Result<PathBuf> {
        let mut current = if path.exists() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.to_path_buf())
        };

        while !current.exists() {
            if !current.pop() {
                break;
            }
        }

        if !current.exists() {
            anyhow::bail!("No existing ancestor found for path: {}", path.display());
        }

        Ok(current.canonicalize()?)
    }

    fn resolve_log_tail_path(input: &Value) -> restflow_ai::error::Result<PathBuf> {
        let logs_dir = crate::paths::logs_dir().map_err(|e| AiError::Tool(e.to_string()))?;
        let path = match input
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|raw| !raw.is_empty())
            .map(PathBuf::from)
        {
            Some(custom_path) if custom_path.is_absolute() => custom_path,
            Some(custom_path) => logs_dir.join(custom_path),
            None => crate::paths::daemon_log_path().map_err(|e| AiError::Tool(e.to_string()))?,
        };

        let logs_root = Self::canonical_existing_ancestor(&logs_dir)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        let path_root =
            Self::canonical_existing_ancestor(&path).map_err(|e| AiError::Tool(e.to_string()))?;
        if !path_root.starts_with(&logs_root) {
            return Err(AiError::Tool(format!(
                "log_tail path must stay under {}",
                logs_dir.display()
            )));
        }

        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.file_type().is_symlink()
        {
            return Err(AiError::Tool(
                "log_tail does not allow symlink paths".to_string(),
            ));
        }

        Ok(path)
    }

    fn read_log_tail(path: &Path, lines: usize) -> anyhow::Result<(Vec<String>, bool)> {
        let mut file = std::fs::File::open(path)?;
        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content)?;
        let all_lines: Vec<String> = content.lines().map(str::to_string).collect();
        let total = all_lines.len();
        let start = total.saturating_sub(lines);
        let truncated = total > lines;
        Ok((all_lines[start..].to_vec(), truncated))
    }

    fn daemon_status_payload() -> anyhow::Result<Value> {
        let status = check_daemon_status()?;
        let payload = match status {
            DaemonStatus::Running { pid } => json!({
                "status": "running",
                "pid": pid
            }),
            DaemonStatus::NotRunning => json!({
                "status": "not_running"
            }),
            DaemonStatus::Stale { pid } => json!({
                "status": "stale",
                "pid": pid
            }),
        };
        Ok(payload)
    }

    fn background_summary_payload(
        &self,
        input: &Value,
    ) -> restflow_ai::error::Result<(Value, Value)> {
        let status_filter = Self::parse_status_filter(input)?;
        let tasks = match status_filter.clone() {
            Some(status) => self
                .background_storage
                .list_tasks_by_status(status)
                .map_err(|e| AiError::Tool(e.to_string()))?,
            None => self
                .background_storage
                .list_tasks()
                .map_err(|e| AiError::Tool(e.to_string()))?,
        };
        let limit = Self::parse_limit(input, "limit", 5, 100);
        let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
        for task in &tasks {
            *by_status
                .entry(task.status.as_str().to_string())
                .or_default() += 1;
        }
        let sample: Vec<Value> = tasks
            .iter()
            .take(limit)
            .map(|task| {
                json!({
                    "id": task.id,
                    "name": task.name,
                    "agent_id": task.agent_id,
                    "status": task.status.as_str(),
                    "updated_at": task.updated_at
                })
            })
            .collect();
        let evidence = json!({
            "total": tasks.len(),
            "by_status": by_status,
            "sample": sample
        });
        let verification = json!({
            "status_filter": status_filter.as_ref().map(|status| status.as_str()),
            "sample_limit": limit,
            "derived_from": "background_agent_storage"
        });
        Ok((evidence, verification))
    }

    fn session_summary_payload(&self, input: &Value) -> restflow_ai::error::Result<(Value, Value)> {
        let limit = Self::parse_limit(input, "limit", 10, 100);
        let summaries = self
            .chat_storage
            .list_summaries()
            .map_err(|e| AiError::Tool(e.to_string()))?;
        let recent: Vec<Value> = summaries
            .iter()
            .take(limit)
            .map(|session| {
                json!({
                    "id": session.id,
                    "name": session.name,
                    "agent_id": session.agent_id,
                    "model": session.model,
                    "message_count": session.message_count,
                    "updated_at": session.updated_at,
                    "last_message_preview": session.last_message_preview
                })
            })
            .collect();
        let evidence = json!({
            "total": summaries.len(),
            "recent": recent
        });
        let verification = json!({
            "sorted_by": "updated_at_desc",
            "sample_limit": limit,
            "derived_from": "chat_session_storage"
        });
        Ok((evidence, verification))
    }

    fn log_tail_payload(input: &Value) -> restflow_ai::error::Result<(Value, Value)> {
        let lines = Self::parse_limit(input, "lines", 100, 1000);
        let path = Self::resolve_log_tail_path(input)?;
        if !path.exists() {
            let evidence = json!({
                "path": path.to_string_lossy(),
                "lines": [],
                "line_count": 0
            });
            let verification = json!({
                "path_exists": false,
                "requested_lines": lines
            });
            return Ok((evidence, verification));
        }

        let (tail, truncated) =
            Self::read_log_tail(&path, lines).map_err(|e| AiError::Tool(e.to_string()))?;
        let evidence = json!({
            "path": path.to_string_lossy(),
            "lines": tail,
            "line_count": tail.len()
        });
        let verification = json!({
            "path_exists": true,
            "requested_lines": lines,
            "truncated": truncated
        });
        Ok((evidence, verification))
    }
}

#[async_trait::async_trait]
impl Tool for ManageOpsTool {
    fn name(&self) -> &str {
        "manage_ops"
    }

    fn description(&self) -> &str {
        "Unified operational diagnostics and control entry for daemon status, health snapshot, background-agent summary, session summary, and log tail."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["daemon_status", "daemon_health", "background_summary", "session_summary", "log_tail"],
                    "description": "Operation to execute."
                },
                "status": {
                    "type": "string",
                    "description": "Optional status filter for background_summary."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Optional row limit for summary operations."
                },
                "lines": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Number of lines for log_tail."
                },
                "path": {
                    "type": "string",
                    "description": "Optional log file path for log_tail. Must stay under ~/.restflow/logs."
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> restflow_ai::error::Result<ToolOutput> {
        let operation_raw = input
            .get("operation")
            .and_then(Value::as_str)
            .ok_or_else(|| AiError::Tool("Missing operation parameter".to_string()))?;
        let operation = parse_operation(operation_raw).map_err(|e| AiError::Tool(e.to_string()))?;

        let payload = match operation {
            ManageOpsOperation::DaemonStatus => {
                let evidence =
                    Self::daemon_status_payload().map_err(|e| AiError::Tool(e.to_string()))?;
                let verification = json!({
                    "source": "daemon_pid_file",
                    "checked_at": Utc::now().timestamp_millis()
                });
                build_response(operation, evidence, verification)
            }
            ManageOpsOperation::DaemonHealth => {
                let socket =
                    crate::paths::socket_path().map_err(|e| AiError::Tool(e.to_string()))?;
                let health = check_health(socket, None)
                    .await
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                let evidence = serde_json::to_value(health).map_err(AiError::from)?;
                let verification = json!({
                    "healthy": evidence["healthy"],
                    "ipc_checked": true,
                    "http_checked": false
                });
                build_response(operation, evidence, verification)
            }
            ManageOpsOperation::BackgroundSummary => {
                let (evidence, verification) = self.background_summary_payload(&input)?;
                build_response(operation, evidence, verification)
            }
            ManageOpsOperation::SessionSummary => {
                let (evidence, verification) = self.session_summary_payload(&input)?;
                build_response(operation, evidence, verification)
            }
            ManageOpsOperation::LogTail => {
                let (evidence, verification) = Self::log_tail_payload(&input)?;
                build_response(operation, evidence, verification)
            }
        };

        Ok(ToolOutput::success(payload))
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
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn restflow_dir_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn setup_storage() -> (
        SkillStorage,
        MemoryStorage,
        ChatSessionStorage,
        SharedSpaceStorage,
        WorkspaceNoteStorage,
        SecretStorage,
        ConfigStorage,
        AgentStorage,
        BackgroundAgentStorage,
        TriggerStorage,
        TerminalSessionStorage,
        tempfile::TempDir,
    ) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let _restflow_env_lock = restflow_dir_env_lock();

        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let previous_master_key = std::env::var_os("RESTFLOW_MASTER_KEY");
        // SAFETY: env vars are modified in a narrow scope and callers use
        // #[tokio::test(flavor = "current_thread")] so no worker threads
        // can race on reads.
        unsafe {
            std::env::set_var("RESTFLOW_DIR", &state_dir);
            std::env::remove_var("RESTFLOW_MASTER_KEY");
        }

        let skill_storage = SkillStorage::new(db.clone()).unwrap();
        let memory_storage = MemoryStorage::new(db.clone()).unwrap();
        let chat_storage = ChatSessionStorage::new(db.clone()).unwrap();
        let shared_space_storage =
            SharedSpaceStorage::new(restflow_storage::SharedSpaceStorage::new(db.clone()).unwrap());
        let workspace_note_storage = WorkspaceNoteStorage::new(db.clone()).unwrap();
        let secret_storage = SecretStorage::with_config(
            db.clone(),
            restflow_storage::SecretStorageConfig {
                allow_insecure_file_permissions: true,
            },
        )
        .unwrap();
        let config_storage = ConfigStorage::new(db.clone()).unwrap();
        let agent_storage = AgentStorage::new(db.clone()).unwrap();
        let background_agent_storage = BackgroundAgentStorage::new(db.clone()).unwrap();
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
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
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
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();
        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            None,
            None,
        );

        // Should have default tools + skill tool
        assert!(registry.has("http_request"));
        assert!(registry.has("send_email"));
        assert!(registry.has("skill"));
        assert!(registry.has("memory_search"));
        assert!(registry.has("shared_space"));
        // New system management tools
        assert!(registry.has("manage_secrets"));
        assert!(registry.has("manage_config"));
        assert!(registry.has("manage_agents"));
        assert!(registry.has("manage_background_agents"));
        assert!(registry.has("manage_marketplace"));
        assert!(registry.has("manage_triggers"));
        assert!(registry.has("manage_terminal"));
        assert!(registry.has("manage_ops"));
        assert!(registry.has("security_query"));
        // Session, memory management, and auth profile tools
        assert!(registry.has("manage_sessions"));
        assert!(registry.has("manage_memory"));
        assert!(registry.has("manage_auth_profiles"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_manage_ops_session_summary_response_schema() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let session =
            crate::models::ChatSession::new("agent-test".to_string(), "gpt-5-mini".to_string())
                .with_name("Ops Session");
        chat_storage.create(&session).unwrap();

        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            None,
            None,
        );

        let output = registry
            .execute_safe(
                "manage_ops",
                json!({ "operation": "session_summary", "limit": 5 }),
            )
            .await
            .unwrap();
        assert!(output.success);
        assert_eq!(output.result["operation"], "session_summary");
        assert!(output.result.get("evidence").is_some());
        assert!(output.result.get("verification").is_some());
    }

    #[test]
    fn test_manage_ops_log_tail_rejects_path_outside_logs_dir() {
        let _lock = restflow_dir_env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let outside_log = temp_dir.path().join("outside.log");
        std::fs::write(&outside_log, "line-1\nline-2\n").unwrap();

        let previous_restflow_dir = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", &state_dir) };

        let result = ManageOpsTool::log_tail_payload(&json!({
            "path": outside_log.to_string_lossy(),
            "lines": 10
        }));

        unsafe {
            if let Some(value) = previous_restflow_dir {
                std::env::set_var("RESTFLOW_DIR", value);
            } else {
                std::env::remove_var("RESTFLOW_DIR");
            }
        }

        let err = result.expect_err("path outside ~/.restflow/logs should be rejected");
        assert!(err.to_string().contains("log_tail path must stay under"));
    }

    #[test]
    fn test_manage_ops_log_tail_allows_relative_path_in_logs_dir() {
        let _lock = restflow_dir_env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        let previous_restflow_dir = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", &state_dir) };

        let logs_dir = crate::paths::logs_dir().unwrap();
        let custom_log = logs_dir.join("custom.log");
        std::fs::write(&custom_log, "line-1\nline-2\nline-3\n").unwrap();

        let result = ManageOpsTool::log_tail_payload(&json!({
            "path": "custom.log",
            "lines": 2
        }));

        unsafe {
            if let Some(value) = previous_restflow_dir {
                std::env::set_var("RESTFLOW_DIR", value);
            } else {
                std::env::remove_var("RESTFLOW_DIR");
            }
        }

        let (evidence, verification) = result.expect("path under ~/.restflow/logs should pass");
        let lines = evidence["lines"]
            .as_array()
            .expect("lines should be an array");
        assert_eq!(evidence["line_count"], json!(2));
        assert_eq!(verification["path_exists"], json!(true));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].as_str(), Some("line-2"));
        assert_eq!(lines[1].as_str(), Some("line-3"));
    }

    #[cfg(unix)]
    #[test]
    fn test_manage_ops_log_tail_rejects_symlink_path() {
        let _lock = restflow_dir_env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        let previous_restflow_dir = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", &state_dir) };

        let logs_dir = crate::paths::logs_dir().unwrap();
        let outside_log = temp_dir.path().join("outside.log");
        std::fs::write(&outside_log, "line-1\nline-2\n").unwrap();
        let symlink_path = logs_dir.join("symlink.log");
        std::os::unix::fs::symlink(&outside_log, &symlink_path).unwrap();

        let result = ManageOpsTool::log_tail_payload(&json!({
            "path": "symlink.log",
            "lines": 2
        }));

        unsafe {
            if let Some(value) = previous_restflow_dir {
                std::env::set_var("RESTFLOW_DIR", value);
            } else {
                std::env::remove_var("RESTFLOW_DIR");
            }
        }

        let err = result.expect_err("symlink path should be rejected");
        let message = err.to_string();
        assert!(
            message.contains("symlink") || message.contains("must stay under"),
            "unexpected error message: {message}"
        );
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn test_manage_agents_accepts_tools_registered_after_snapshot_point() {
        struct AgentsDirEnvCleanup;
        impl Drop for AgentsDirEnvCleanup {
            fn drop(&mut self) {
                unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
            }
        }
        let _cleanup = AgentsDirEnvCleanup;
        let _env_lock = crate::prompt_files::agents_dir_env_lock();
        let agents_temp = tempdir().unwrap();
        unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            None,
            None,
        );

        let output = registry
            .execute_safe(
                "manage_agents",
                json!({
                    "operation": "create",
                    "name": "Late Tool Validation Agent",
                    "agent": {
                        "tools": [
                            "manage_background_agents",
                            "manage_terminal",
                            "security_query"
                        ]
                    }
                }),
            )
            .await
            .unwrap();

        assert!(
            output.success,
            "expected create to pass known tool validation, got: {:?}",
            output.result
        );
    }

    #[test]
    fn test_skill_provider_list_empty() {
        let (
            storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _workspace_note_storage,
            _secret_storage,
            _config_storage,
            _agent_storage,
            _background_agent_storage,
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
            _workspace_note_storage,
            _secret_storage,
            _config_storage,
            _agent_storage,
            _background_agent_storage,
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
        struct AgentsDirEnvCleanup;
        impl Drop for AgentsDirEnvCleanup {
            fn drop(&mut self) {
                unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
            }
        }

        let _cleanup = AgentsDirEnvCleanup;

        // Acquire the shared env lock to avoid racing with prompt_files tests
        let _env_lock = crate::prompt_files::agents_dir_env_lock();
        let agents_temp = tempdir().unwrap();
        unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

        let (
            skill_storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _workspace_note_storage,
            secret_storage,
            _config_storage,
            agent_storage,
            background_agent_storage,
            _trigger_storage,
            _terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let ops_skill = crate::models::Skill::new(
            "ops-skill".to_string(),
            "Ops Skill".to_string(),
            None,
            None,
            "ops".to_string(),
        );
        skill_storage.create(&ops_skill).unwrap();
        let audit_skill = crate::models::Skill::new(
            "audit-skill".to_string(),
            "Audit Skill".to_string(),
            None,
            None,
            "audit".to_string(),
        );
        skill_storage.create(&audit_skill).unwrap();

        let known_tools = Arc::new(RwLock::new(
            [
                "manage_background_agents".to_string(),
                "manage_agents".to_string(),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
        ));
        let adapter = AgentStoreAdapter::new(
            agent_storage,
            skill_storage,
            secret_storage,
            background_agent_storage,
            known_tools,
        );
        let base_node = crate::models::AgentNode {
            model: Some(crate::models::AIModel::ClaudeSonnet4_5),
            prompt: Some("You are a testing assistant".to_string()),
            temperature: Some(0.3),
            codex_cli_reasoning_effort: None,
            codex_cli_execution_mode: None,
            api_key_config: Some(crate::models::ApiKeyConfig::Direct("test-key".to_string())),
            tools: Some(vec!["manage_background_agents".to_string()]),
            skills: Some(vec!["ops-skill".to_string()]),
            skill_variables: None,
            python_runtime_policy: None,
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
                    "tools": ["manage_background_agents", "manage_agents"],
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
    fn test_agent_store_adapter_rejects_unknown_tool() {
        struct AgentsDirEnvCleanup;
        impl Drop for AgentsDirEnvCleanup {
            fn drop(&mut self) {
                unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
            }
        }

        let _cleanup = AgentsDirEnvCleanup;
        let _env_lock = crate::prompt_files::agents_dir_env_lock();
        let agents_temp = tempdir().unwrap();
        unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

        let (
            skill_storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _workspace_note_storage,
            secret_storage,
            _config_storage,
            agent_storage,
            background_agent_storage,
            _trigger_storage,
            _terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let known_tools = Arc::new(RwLock::new(
            ["manage_background_agents".to_string()]
                .into_iter()
                .collect::<HashSet<_>>(),
        ));
        let adapter = AgentStoreAdapter::new(
            agent_storage,
            skill_storage,
            secret_storage,
            background_agent_storage,
            known_tools,
        );

        let err = AgentStore::create_agent(
            &adapter,
            AgentCreateRequest {
                name: "Invalid".to_string(),
                agent: serde_json::json!({
                    "tools": ["unknown_tool"]
                }),
            },
        )
        .expect_err("expected validation error");
        assert!(err.to_string().contains("validation_error"));
    }

    #[test]
    fn test_agent_store_adapter_blocks_delete_with_active_task() {
        struct AgentsDirEnvCleanup;
        impl Drop for AgentsDirEnvCleanup {
            fn drop(&mut self) {
                unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
            }
        }

        let _cleanup = AgentsDirEnvCleanup;
        let _env_lock = crate::prompt_files::agents_dir_env_lock();
        let agents_temp = tempdir().unwrap();
        unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

        let (
            skill_storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _workspace_note_storage,
            secret_storage,
            _config_storage,
            agent_storage,
            background_agent_storage,
            _trigger_storage,
            _terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let known_tools = Arc::new(RwLock::new(
            ["manage_background_agents".to_string()]
                .into_iter()
                .collect::<HashSet<_>>(),
        ));
        let adapter = AgentStoreAdapter::new(
            agent_storage.clone(),
            skill_storage,
            secret_storage,
            background_agent_storage.clone(),
            known_tools,
        );

        let created = AgentStore::create_agent(
            &adapter,
            AgentCreateRequest {
                name: "Task Owner".to_string(),
                agent: serde_json::json!({
                    "model": "claude-sonnet-4-5",
                    "prompt": "owner"
                }),
            },
        )
        .unwrap();
        let agent_id = created
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap()
            .to_string();

        background_agent_storage
            .create_task(
                "Active MCP Task".to_string(),
                agent_id.clone(),
                crate::models::BackgroundAgentSchedule::default(),
            )
            .unwrap();

        let err = AgentStore::delete_agent(&adapter, &agent_id).expect_err("should be blocked");
        let msg = err.to_string();
        assert!(msg.contains("Cannot delete agent"));
        assert!(msg.contains("Active MCP Task"));
    }

    #[test]
    fn test_task_store_adapter_background_agent_flow() {
        struct AgentsDirEnvCleanup;
        impl Drop for AgentsDirEnvCleanup {
            fn drop(&mut self) {
                unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
            }
        }
        let _cleanup = AgentsDirEnvCleanup;
        let _env_lock = crate::prompt_files::agents_dir_env_lock();
        let agents_temp = tempdir().unwrap();
        unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

        let (
            _skill_storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _workspace_note_storage,
            _secret_storage,
            _config_storage,
            agent_storage,
            background_agent_storage,
            _trigger_storage,
            _terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let created_agent = agent_storage
            .create_agent(
                "Background Owner".to_string(),
                crate::models::AgentNode::new(),
            )
            .unwrap();
        let adapter = BackgroundAgentStoreAdapter::new(background_agent_storage, agent_storage);

        let created = BackgroundAgentStore::create_background_agent(
            &adapter,
            BackgroundAgentCreateRequest {
                name: "Background Agent".to_string(),
                agent_id: created_agent.id,
                schedule: None,
                input: Some("Run periodic checks".to_string()),
                input_template: Some("Template {{task.id}}".to_string()),
                timeout_secs: Some(1800),
                memory: None,
                memory_scope: Some("per_background_agent".to_string()),
                resource_limits: None,
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
            Some("per_background_agent")
        );
        let task_id = created
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap()
            .to_string();

        let updated = BackgroundAgentStore::update_background_agent(
            &adapter,
            BackgroundAgentUpdateRequest {
                id: task_id.clone(),
                name: Some("Background Agent Updated".to_string()),
                description: Some("Updated description".to_string()),
                agent_id: None,
                input: Some("Run checks and summarize".to_string()),
                input_template: Some("Updated {{task.name}}".to_string()),
                schedule: None,
                notification: None,
                execution_mode: None,
                timeout_secs: Some(900),
                memory: None,
                memory_scope: Some("shared_agent".to_string()),
                resource_limits: None,
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
        assert_eq!(
            updated.get("timeout_secs").and_then(|value| value.as_u64()),
            Some(900)
        );

        let controlled = BackgroundAgentStore::control_background_agent(
            &adapter,
            BackgroundAgentControlRequest {
                id: task_id.clone(),
                action: "run_now".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            controlled.get("status").and_then(|value| value.as_str()),
            Some("active")
        );

        let message = BackgroundAgentStore::send_background_agent_message(
            &adapter,
            BackgroundAgentMessageRequest {
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

        let progress = BackgroundAgentStore::get_background_agent_progress(
            &adapter,
            BackgroundAgentProgressRequest {
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

        let messages = BackgroundAgentStore::list_background_agent_messages(
            &adapter,
            BackgroundAgentMessageListRequest {
                id: task_id.clone(),
                limit: Some(10),
            },
        )
        .unwrap();
        assert_eq!(messages.as_array().map(|items| items.len()), Some(1));

        let deleted = BackgroundAgentStore::delete_background_agent(&adapter, &task_id).unwrap();
        assert_eq!(
            deleted.get("deleted").and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_marketplace_tool_list_and_uninstall() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
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
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            None,
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

    #[tokio::test(flavor = "current_thread")]
    async fn test_trigger_tool_create_list_disable() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            None,
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

    #[tokio::test(flavor = "current_thread")]
    async fn test_terminal_tool_create_send_read_close() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            None,
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

    #[tokio::test(flavor = "current_thread")]
    async fn test_security_query_tool_show_policy_and_check_permission() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            None,
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

    #[tokio::test(flavor = "current_thread")]
    async fn test_db_memory_store_adapter_crud() {
        let (
            _skill_storage,
            memory_storage,
            _chat_storage,
            _shared_space_storage,
            _workspace_note_storage,
            _secret_storage,
            _config_storage,
            _agent_storage,
            _background_agent_storage,
            _trigger_storage,
            _terminal_storage,
            _temp_dir,
        ) = setup_storage();

        let store = DbMemoryStoreAdapter::new(memory_storage);

        // Test save
        let saved = store
            .save(
                "test-agent",
                "My Note",
                "Hello world content",
                &["tag1".into(), "tag2".into()],
            )
            .unwrap();
        assert!(saved["success"].as_bool().unwrap());
        let entry_id = saved["id"].as_str().unwrap().to_string();
        assert_eq!(saved["title"].as_str().unwrap(), "My Note");

        // Test read_by_id
        let read = store.read_by_id(&entry_id).unwrap().unwrap();
        assert!(read["found"].as_bool().unwrap());
        assert_eq!(read["entry"]["title"].as_str().unwrap(), "My Note");
        assert_eq!(
            read["entry"]["content"].as_str().unwrap(),
            "Hello world content"
        );
        let tags = read["entry"]["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(tags.contains(&"tag1"));
        assert!(tags.contains(&"tag2"));
        // __title tag should NOT appear in user tags
        assert!(!tags.iter().any(|t| t.starts_with("__title:")));

        // Test list
        let listed = store.list("test-agent", None, 10).unwrap();
        assert_eq!(listed["total"].as_u64().unwrap(), 1);
        let memories = listed["memories"].as_array().unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0]["title"].as_str().unwrap(), "My Note");

        // Test list by tag
        let listed = store.list("test-agent", Some("tag1"), 10).unwrap();
        assert_eq!(listed["count"].as_u64().unwrap(), 1);
        let listed = store.list("test-agent", Some("nonexistent"), 10).unwrap();
        assert_eq!(listed["count"].as_u64().unwrap(), 0);

        // Test search by title keyword
        let found = store.search("test-agent", None, Some("Note"), 10).unwrap();
        assert!(found["count"].as_u64().unwrap() >= 1);
        let found = store
            .search("test-agent", None, Some("nonexistent"), 10)
            .unwrap();
        assert_eq!(found["count"].as_u64().unwrap(), 0);

        // Test search by tag
        let found = store.search("test-agent", Some("tag2"), None, 10).unwrap();
        assert!(found["count"].as_u64().unwrap() >= 1);

        // Test dedup: saving same content again should not create a duplicate
        let saved2 = store
            .save(
                "test-agent",
                "My Note",
                "Hello world content",
                &["tag1".into()],
            )
            .unwrap();
        assert!(saved2["success"].as_bool().unwrap());
        let listed = store.list("test-agent", None, 10).unwrap();
        assert_eq!(listed["total"].as_u64().unwrap(), 1);

        // Test delete
        let deleted = store.delete(&entry_id).unwrap();
        assert!(deleted["deleted"].as_bool().unwrap());
        let listed = store.list("test-agent", None, 10).unwrap();
        assert_eq!(listed["total"].as_u64().unwrap(), 0);

        // Test read_by_id after delete
        let read = store.read_by_id(&entry_id).unwrap();
        assert!(read.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_create_tool_registry_always_has_memory_tools() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            _temp_dir,
        ) = setup_storage();

        // Memory tools are always registered even without agent_id
        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            workspace_note_storage,
            secret_storage,
            config_storage,
            agent_storage,
            background_agent_storage,
            trigger_storage,
            terminal_storage,
            None,
            None,
        );

        assert!(registry.has("save_to_memory"));
        assert!(registry.has("read_memory"));
        assert!(registry.has("list_memories"));
        assert!(registry.has("delete_memory"));
    }
}
