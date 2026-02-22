//! BackgroundAgentStore adapter backed by BackgroundAgentStorage.

use crate::models::{
    BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSchedule,
    BackgroundAgentSpec, BackgroundAgentStatus, BackgroundMessageSource, DurabilityMode,
    MemoryConfig, MemoryScope, ResourceLimits,
};
use crate::storage::{AgentStorage, BackgroundAgentStorage};
use chrono::Utc;
use restflow_ai::tools::{
    BackgroundAgentControlRequest, BackgroundAgentCreateRequest,
    BackgroundAgentDeliverableListRequest, BackgroundAgentMessageListRequest,
    BackgroundAgentMessageRequest, BackgroundAgentProgressRequest,
    BackgroundAgentScratchpadListRequest, BackgroundAgentScratchpadReadRequest,
    BackgroundAgentStore, BackgroundAgentUpdateRequest,
};
use restflow_tools::ToolError;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Clone)]
pub struct BackgroundAgentStoreAdapter {
    storage: BackgroundAgentStorage,
    agent_storage: AgentStorage,
    deliverable_storage: crate::storage::DeliverableStorage,
}

impl BackgroundAgentStoreAdapter {
    pub fn new(
        storage: BackgroundAgentStorage,
        agent_storage: AgentStorage,
        deliverable_storage: crate::storage::DeliverableStorage,
    ) -> Self {
        Self {
            storage,
            agent_storage,
            deliverable_storage,
        }
    }

    fn parse_status(status: &str) -> Result<BackgroundAgentStatus, ToolError> {
        match status.trim().to_lowercase().as_str() {
            "active" => Ok(BackgroundAgentStatus::Active),
            "paused" => Ok(BackgroundAgentStatus::Paused),
            "running" => Ok(BackgroundAgentStatus::Running),
            "completed" => Ok(BackgroundAgentStatus::Completed),
            "failed" => Ok(BackgroundAgentStatus::Failed),
            "interrupted" => Ok(BackgroundAgentStatus::Interrupted),
            _ => Err(ToolError::Tool(format!("Unknown status: {}", status))),
        }
    }

    fn parse_control_action(action: &str) -> Result<BackgroundAgentControlAction, ToolError> {
        match action.trim().to_lowercase().as_str() {
            "start" => Ok(BackgroundAgentControlAction::Start),
            "pause" => Ok(BackgroundAgentControlAction::Pause),
            "resume" => Ok(BackgroundAgentControlAction::Resume),
            "stop" => Ok(BackgroundAgentControlAction::Stop),
            "run_now" | "run-now" | "runnow" => Ok(BackgroundAgentControlAction::RunNow),
            _ => Err(ToolError::Tool(format!("Unknown control action: {}", action))),
        }
    }

    fn parse_message_source(source: Option<&str>) -> Result<BackgroundMessageSource, ToolError> {
        match source.map(|value| value.trim().to_lowercase()) {
            None => Ok(BackgroundMessageSource::User),
            Some(value) if value.is_empty() => Ok(BackgroundMessageSource::User),
            Some(value) if value == "user" => Ok(BackgroundMessageSource::User),
            Some(value) if value == "agent" => Ok(BackgroundMessageSource::Agent),
            Some(value) if value == "system" => Ok(BackgroundMessageSource::System),
            Some(value) => Err(ToolError::Tool(format!("Unknown message source: {}", value))),
        }
    }

    fn parse_optional_value<T: DeserializeOwned>(
        field: &str,
        value: Option<Value>,
    ) -> Result<Option<T>, ToolError> {
        match value {
            Some(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(|e| ToolError::Tool(format!("Invalid {}: {}", field, e))),
            None => Ok(None),
        }
    }

    fn parse_memory_scope(value: Option<&str>) -> Result<Option<MemoryScope>, ToolError> {
        match value.map(|scope| scope.trim().to_lowercase()) {
            None => Ok(None),
            Some(scope) if scope.is_empty() => Ok(None),
            Some(scope) if scope == "shared_agent" => Ok(Some(MemoryScope::SharedAgent)),
            Some(scope) if scope == "per_background_agent" => {
                Ok(Some(MemoryScope::PerBackgroundAgent))
            }
            Some(scope) => Err(ToolError::Tool(format!("Unknown memory_scope: {}", scope))),
        }
    }

    fn parse_durability_mode(value: Option<&str>) -> Result<Option<DurabilityMode>, ToolError> {
        match value.map(|mode| mode.trim().to_lowercase()) {
            None => Ok(None),
            Some(mode) if mode.is_empty() => Ok(None),
            Some(mode) if mode == "sync" => Ok(Some(DurabilityMode::Sync)),
            Some(mode) if mode == "async" => Ok(Some(DurabilityMode::Async)),
            Some(mode) if mode == "exit" => Ok(Some(DurabilityMode::Exit)),
            Some(mode) => Err(ToolError::Tool(format!("Unknown durability_mode: {}", mode))),
        }
    }

    fn merge_memory_scope(
        memory: Option<MemoryConfig>,
        memory_scope: Option<String>,
    ) -> Result<Option<MemoryConfig>, ToolError> {
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

    fn resolve_agent_id(&self, id_or_prefix: &str) -> Result<String, ToolError> {
        self.agent_storage
            .resolve_existing_agent_id(id_or_prefix)
            .map_err(|e| ToolError::Tool(e.to_string()))
    }

    fn resolve_task_id(&self, id_or_prefix: &str) -> Result<String, ToolError> {
        self.storage
            .resolve_existing_task_id(id_or_prefix)
            .map_err(|e| ToolError::Tool(e.to_string()))
    }

    fn scratchpad_dir() -> Result<PathBuf, ToolError> {
        let dir = crate::paths::ensure_restflow_dir()
            .map_err(|e| ToolError::Tool(e.to_string()))?
            .join("scratchpads");
        std::fs::create_dir_all(&dir).map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(dir)
    }

    pub(crate) fn validate_scratchpad_name(name: &str) -> Result<(), ToolError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ToolError::Tool(
                "scratchpad name must not be empty".to_string(),
            ));
        }
        if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
            return Err(ToolError::Tool("invalid scratchpad name".to_string()));
        }
        if !trimmed.ends_with(".jsonl") {
            return Err(ToolError::Tool(
                "scratchpad must be a .jsonl file".to_string(),
            ));
        }
        Ok(())
    }
}

impl BackgroundAgentStore for BackgroundAgentStoreAdapter {
    fn create_background_agent(
        &self,
        request: BackgroundAgentCreateRequest,
    ) -> restflow_tools::Result<Value> {
        let resolved_agent_id = self.resolve_agent_id(&request.agent_id)?;
        let schedule =
            Self::parse_optional_value::<BackgroundAgentSchedule>("schedule", request.schedule)?
                .unwrap_or_default();
        let memory = Self::parse_optional_value("memory", request.memory)?;
        let memory = Self::merge_memory_scope(memory, request.memory_scope)?;
        let durability_mode = Self::parse_durability_mode(request.durability_mode.as_deref())?;
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
                durability_mode,
                resource_limits,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(ToolError::from)
    }

    fn update_background_agent(
        &self,
        request: BackgroundAgentUpdateRequest,
    ) -> restflow_tools::Result<Value> {
        let resolved_agent_id = request
            .agent_id
            .as_deref()
            .map(|id| self.resolve_agent_id(id))
            .transpose()?;
        let memory = Self::parse_optional_value("memory", request.memory)?;
        let memory = Self::merge_memory_scope(memory, request.memory_scope)?;
        let durability_mode = Self::parse_durability_mode(request.durability_mode.as_deref())?;
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
            durability_mode,
            resource_limits,
            prerequisites: None,
            continuation: None,
        };

        let resolved_id = self.resolve_task_id(&request.id)?;
        let task = self
            .storage
            .update_background_agent(&resolved_id, patch)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(ToolError::from)
    }

    fn delete_background_agent(&self, id: &str) -> restflow_tools::Result<Value> {
        let resolved_id = self.resolve_task_id(id)?;
        let deleted = self
            .storage
            .delete_task(&resolved_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": deleted }))
    }

    fn list_background_agents(
        &self,
        status: Option<String>,
    ) -> restflow_tools::Result<Value> {
        let tasks = if let Some(status) = status {
            let status = Self::parse_status(&status)?;
            self.storage
                .list_tasks_by_status(status)
                .map_err(|e| ToolError::Tool(e.to_string()))?
        } else {
            self.storage
                .list_tasks()
                .map_err(|e| ToolError::Tool(e.to_string()))?
        };

        serde_json::to_value(tasks).map_err(ToolError::from)
    }

    fn control_background_agent(
        &self,
        request: BackgroundAgentControlRequest,
    ) -> restflow_tools::Result<Value> {
        let action = Self::parse_control_action(&request.action)?;
        let resolved_id = self.resolve_task_id(&request.id)?;
        let task = self
            .storage
            .control_background_agent(&resolved_id, action)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(ToolError::from)
    }

    fn get_background_agent_progress(
        &self,
        request: BackgroundAgentProgressRequest,
    ) -> restflow_tools::Result<Value> {
        let resolved_id = self.resolve_task_id(&request.id)?;
        let progress = self
            .storage
            .get_background_agent_progress(&resolved_id, request.event_limit.unwrap_or(10).max(1))
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(progress).map_err(ToolError::from)
    }

    fn send_background_agent_message(
        &self,
        request: BackgroundAgentMessageRequest,
    ) -> restflow_tools::Result<Value> {
        let source = Self::parse_message_source(request.source.as_deref())?;
        let resolved_id = self.resolve_task_id(&request.id)?;
        let message = self
            .storage
            .send_background_agent_message(&resolved_id, request.message, source)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(message).map_err(ToolError::from)
    }

    fn list_background_agent_messages(
        &self,
        request: BackgroundAgentMessageListRequest,
    ) -> restflow_tools::Result<Value> {
        let resolved_id = self.resolve_task_id(&request.id)?;
        let messages = self
            .storage
            .list_background_agent_messages(&resolved_id, request.limit.unwrap_or(50).max(1))
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(messages).map_err(ToolError::from)
    }

    fn list_background_agent_deliverables(
        &self,
        request: BackgroundAgentDeliverableListRequest,
    ) -> restflow_tools::Result<Value> {
        let items = self
            .deliverable_storage
            .list_by_task(&request.id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(items).map_err(ToolError::from)
    }

    fn list_background_agent_scratchpads(
        &self,
        request: BackgroundAgentScratchpadListRequest,
    ) -> restflow_tools::Result<Value> {
        let dir = Self::scratchpad_dir()?;
        let prefix = request.id.map(|id| format!("{id}-"));
        let mut entries: Vec<(std::time::SystemTime, Value)> = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| ToolError::Tool(e.to_string()))? {
            let entry = entry.map_err(|e| ToolError::Tool(e.to_string()))?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            let file_name = match path.file_name().and_then(|name| name.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };
            if let Some(prefix) = &prefix
                && !file_name.starts_with(prefix)
            {
                continue;
            }

            let metadata = entry.metadata().map_err(|e| ToolError::Tool(e.to_string()))?;
            let modified = metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
            let modified_at = chrono::DateTime::<Utc>::from(modified).to_rfc3339();
            let task_id = file_name.strip_suffix(".jsonl").and_then(|name| {
                let mut parts = name.rsplitn(3, '-');
                let _time = parts.next();
                let _date = parts.next();
                parts.next().map(ToString::to_string)
            });
            entries.push((
                modified,
                json!({
                    "scratchpad": file_name,
                    "task_id": task_id,
                    "size_bytes": metadata.len(),
                    "modified_at": modified_at,
                }),
            ));
        }

        entries.sort_by(|a, b| b.0.cmp(&a.0));
        let limit = request.limit.unwrap_or(50).max(1);
        let data: Vec<Value> = entries
            .into_iter()
            .take(limit)
            .map(|(_, value)| value)
            .collect();
        Ok(Value::Array(data))
    }

    fn read_background_agent_scratchpad(
        &self,
        request: BackgroundAgentScratchpadReadRequest,
    ) -> restflow_tools::Result<Value> {
        Self::validate_scratchpad_name(&request.scratchpad)?;
        let dir = Self::scratchpad_dir()?;
        let path = dir.join(&request.scratchpad);
        // Reject symlinks to prevent path traversal attacks.
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.file_type().is_symlink()
        {
            return Err(ToolError::Tool(
                "scratchpad does not allow symlink paths".to_string(),
            ));
        }

        // Canonicalize and verify path stays within scratchpad directory.
        let canonical_dir = dir
            .canonicalize()
            .map_err(|e| ToolError::Tool(format!("failed to resolve scratchpad dir: {}", e)))?;
        let canonical_path = path
            .canonicalize()
            .map_err(|e| ToolError::Tool(format!("failed to resolve scratchpad path: {}", e)))?;
        if !canonical_path.starts_with(&canonical_dir) {
            return Err(ToolError::Tool(
                "scratchpad path escapes scratchpad directory".to_string(),
            ));
        }

        if !path.is_file() {
            return Err(ToolError::Tool(format!(
                "scratchpad {} not found",
                request.scratchpad
            )));
        }

        let content = std::fs::read_to_string(&path).map_err(|e| ToolError::Tool(e.to_string()))?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let line_limit = request.line_limit.unwrap_or(200).max(1);
        let start = total_lines.saturating_sub(line_limit);
        let tail: Vec<String> = lines[start..]
            .iter()
            .map(|line| (*line).to_string())
            .collect();
        Ok(json!({
            "scratchpad": request.scratchpad,
            "path": path.to_string_lossy().into_owned(),
            "total_lines": total_lines,
            "line_limit": line_limit,
            "lines": tail,
        }))
    }
}
