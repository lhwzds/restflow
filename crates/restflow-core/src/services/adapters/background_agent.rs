//! BackgroundAgentStore adapter backed by BackgroundAgentStorage.

use crate::models::{
    BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSchedule,
    BackgroundAgentSpec, BackgroundAgentStatus, BackgroundMessageSource, DurabilityMode,
    MemoryConfig, MemoryScope, ResourceLimits,
};
use crate::storage::{AgentStorage, BackgroundAgentStorage};
use restflow_tools::ToolError;
use restflow_traits::store::{
    BackgroundAgentControlRequest, BackgroundAgentCreateRequest,
    BackgroundAgentDeliverableListRequest, BackgroundAgentMessageListRequest,
    BackgroundAgentMessageRequest, BackgroundAgentProgressRequest, BackgroundAgentStore,
    BackgroundAgentTraceListRequest, BackgroundAgentTraceReadRequest, BackgroundAgentUpdateRequest,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::HashSet;

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
            _ => Err(ToolError::Tool(format!(
                "Unknown control action: {}",
                action
            ))),
        }
    }

    fn parse_message_source(source: Option<&str>) -> Result<BackgroundMessageSource, ToolError> {
        match source.map(|value| value.trim().to_lowercase()) {
            None => Ok(BackgroundMessageSource::User),
            Some(value) if value.is_empty() => Ok(BackgroundMessageSource::User),
            Some(value) if value == "user" => Ok(BackgroundMessageSource::User),
            Some(value) if value == "agent" => Ok(BackgroundMessageSource::Agent),
            Some(value) if value == "system" => Ok(BackgroundMessageSource::System),
            Some(value) => Err(ToolError::Tool(format!(
                "Unknown message source: {}",
                value
            ))),
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
            Some(mode) => Err(ToolError::Tool(format!(
                "Unknown durability_mode: {}",
                mode
            ))),
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
        Ok(self.agent_storage.resolve_existing_agent_id(id_or_prefix)?)
    }

    fn resolve_task_id(&self, id_or_prefix: &str) -> Result<String, ToolError> {
        Ok(self.storage.resolve_existing_task_id(id_or_prefix)?)
    }

    fn task_session_id(&self, task_id_or_prefix: &str) -> Result<String, ToolError> {
        let resolved_id = self.resolve_task_id(task_id_or_prefix)?;
        let task = self.storage.get_task(&resolved_id)?.ok_or_else(|| {
            ToolError::Tool(format!("background agent {} not found", resolved_id))
        })?;
        let session_id = task.chat_session_id.trim();
        if session_id.is_empty() {
            Ok(task.id)
        } else {
            Ok(session_id.to_string())
        }
    }

    fn all_session_ids(&self) -> Result<Vec<String>, ToolError> {
        let mut seen = HashSet::new();
        let mut session_ids = Vec::new();
        for task in self.storage.list_tasks()? {
            let session_id = if task.chat_session_id.trim().is_empty() {
                task.id
            } else {
                task.chat_session_id
            };
            if seen.insert(session_id.clone()) {
                session_ids.push(session_id);
            }
        }
        Ok(session_ids)
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
        let task = self.storage.create_background_agent(BackgroundAgentSpec {
            name: request.name,
            agent_id: resolved_agent_id,
            chat_session_id: request.chat_session_id,
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
        })?;
        Ok(serde_json::to_value(task)?)
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
            chat_session_id: request.chat_session_id,
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
        let task = self.storage.update_background_agent(&resolved_id, patch)?;
        Ok(serde_json::to_value(task)?)
    }

    fn delete_background_agent(&self, id: &str) -> restflow_tools::Result<Value> {
        let resolved_id = self.resolve_task_id(id)?;
        let deleted = self.storage.delete_task(&resolved_id)?;
        Ok(json!({ "id": id, "deleted": deleted }))
    }

    fn list_background_agents(&self, status: Option<String>) -> restflow_tools::Result<Value> {
        let tasks = if let Some(status) = status {
            let status = Self::parse_status(&status)?;
            self.storage.list_tasks_by_status(status)?
        } else {
            self.storage.list_tasks()?
        };

        Ok(serde_json::to_value(tasks)?)
    }

    fn control_background_agent(
        &self,
        request: BackgroundAgentControlRequest,
    ) -> restflow_tools::Result<Value> {
        let action = Self::parse_control_action(&request.action)?;
        let resolved_id = self.resolve_task_id(&request.id)?;
        let task = self
            .storage
            .control_background_agent(&resolved_id, action)?;
        Ok(serde_json::to_value(task)?)
    }

    fn get_background_agent_progress(
        &self,
        request: BackgroundAgentProgressRequest,
    ) -> restflow_tools::Result<Value> {
        let resolved_id = self.resolve_task_id(&request.id)?;
        let progress = self.storage.get_background_agent_progress(
            &resolved_id,
            request.event_limit.unwrap_or(10).max(1),
        )?;
        Ok(serde_json::to_value(progress)?)
    }

    fn send_background_agent_message(
        &self,
        request: BackgroundAgentMessageRequest,
    ) -> restflow_tools::Result<Value> {
        let source = Self::parse_message_source(request.source.as_deref())?;
        let resolved_id = self.resolve_task_id(&request.id)?;
        let message =
            self.storage
                .send_background_agent_message(&resolved_id, request.message, source)?;
        Ok(serde_json::to_value(message)?)
    }

    fn list_background_agent_messages(
        &self,
        request: BackgroundAgentMessageListRequest,
    ) -> restflow_tools::Result<Value> {
        let resolved_id = self.resolve_task_id(&request.id)?;
        let messages = self
            .storage
            .list_background_agent_messages(&resolved_id, request.limit.unwrap_or(50).max(1))?;
        Ok(serde_json::to_value(messages)?)
    }

    fn list_background_agent_deliverables(
        &self,
        request: BackgroundAgentDeliverableListRequest,
    ) -> restflow_tools::Result<Value> {
        let items = self.deliverable_storage.list_by_task(&request.id)?;
        Ok(serde_json::to_value(items)?)
    }

    fn list_background_agent_traces(
        &self,
        request: BackgroundAgentTraceListRequest,
    ) -> restflow_tools::Result<Value> {
        let limit = request.limit.unwrap_or(50).max(1);
        let session_ids = if let Some(task_id) = request.id.as_deref() {
            vec![self.task_session_id(task_id)?]
        } else {
            self.all_session_ids()?
        };

        let mut traces = Vec::new();
        for session_id in session_ids {
            let mut session_traces = self
                .storage
                .tool_traces()
                .list_by_session(&session_id, None)?;
            traces.append(&mut session_traces);
        }
        traces.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        traces.truncate(limit);

        let data = traces
            .into_iter()
            .map(|trace| {
                json!({
                    "trace_id": trace.id,
                    "session_id": trace.session_id,
                    "turn_id": trace.turn_id,
                    "event_type": trace.event_type,
                    "tool_call_id": trace.tool_call_id,
                    "tool_name": trace.tool_name,
                    "success": trace.success,
                    "duration_ms": trace.duration_ms,
                    "created_at": trace.created_at,
                    "output_ref": trace.output_ref,
                })
            })
            .collect::<Vec<_>>();
        Ok(Value::Array(data))
    }

    fn read_background_agent_trace(
        &self,
        request: BackgroundAgentTraceReadRequest,
    ) -> restflow_tools::Result<Value> {
        let trace_id = request.trace_id.trim();
        if trace_id.is_empty() {
            return Err(ToolError::Tool("trace_id must not be empty".to_string()));
        }

        let limit = request.line_limit.unwrap_or(200).max(1);
        let mut found = None;
        for session_id in self.all_session_ids()? {
            if let Some(trace) = self
                .storage
                .tool_traces()
                .list_by_session(&session_id, None)?
                .into_iter()
                .find(|trace| trace.id == trace_id)
            {
                found = Some(trace);
                break;
            }
        }
        let trace =
            found.ok_or_else(|| ToolError::Tool(format!("trace {} not found", trace_id)))?;

        let mut response = serde_json::Map::new();
        response.insert("trace".to_string(), serde_json::to_value(&trace)?);
        response.insert("line_limit".to_string(), json!(limit));

        if let Some(path) = trace.output_ref.as_deref() {
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    let lines = content.lines().collect::<Vec<_>>();
                    let total_lines = lines.len();
                    let start = total_lines.saturating_sub(limit);
                    let tail = lines[start..]
                        .iter()
                        .map(|line| (*line).to_string())
                        .collect::<Vec<_>>();
                    response.insert(
                        "output".to_string(),
                        json!({
                            "path": path,
                            "total_lines": total_lines,
                            "lines": tail,
                        }),
                    );
                }
                Err(error) => {
                    response.insert(
                        "output_error".to_string(),
                        json!(format!("failed to read output_ref {}: {}", path, error)),
                    );
                }
            }
        }

        Ok(Value::Object(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::store::BackgroundAgentStore;
    use std::sync::{Arc, Mutex, OnceLock};
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn setup() -> (
        BackgroundAgentStoreAdapter,
        tempfile::TempDir,
        std::sync::MutexGuard<'static, ()>,
    ) {
        let guard = env_lock();
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let bg_storage = BackgroundAgentStorage::new(db.clone()).unwrap();
        let agent_storage = AgentStorage::new(db.clone()).unwrap();
        let deliverable_storage = crate::storage::DeliverableStorage::new(db).unwrap();

        // Set RESTFLOW_DIR so create_agent can write agent prompt files
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let prev_dir = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", &state_dir) };

        // Create a default agent for referencing
        let agent = crate::models::AgentNode::default();
        agent_storage
            .create_agent("test-agent".to_string(), agent)
            .unwrap();

        // Restore env var immediately after agent creation
        unsafe {
            match prev_dir {
                Some(v) => std::env::set_var("RESTFLOW_DIR", v),
                None => std::env::remove_var("RESTFLOW_DIR"),
            }
        }

        (
            BackgroundAgentStoreAdapter::new(bg_storage, agent_storage, deliverable_storage),
            temp_dir,
            guard,
        )
    }

    fn get_agent_id(adapter: &BackgroundAgentStoreAdapter) -> String {
        let agents = adapter.agent_storage.list_agents().unwrap();
        agents[0].id.clone()
    }

    #[test]
    fn test_create_and_list_background_agent() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let request = BackgroundAgentCreateRequest {
            name: "Test BG Task".to_string(),
            agent_id,
            chat_session_id: None,
            input: Some("Do something".to_string()),
            input_template: None,
            schedule: None,
            timeout_secs: None,
            memory: None,
            memory_scope: None,
            durability_mode: None,
            resource_limits: None,
        };
        let created = adapter.create_background_agent(request).unwrap();
        assert!(created["id"].as_str().is_some());

        let list = adapter.list_background_agents(None).unwrap();
        let tasks = list.as_array().unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn test_delete_background_agent() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let request = BackgroundAgentCreateRequest {
            name: "Delete Me".to_string(),
            agent_id,
            chat_session_id: None,
            input: Some("task to delete".to_string()),
            input_template: None,
            schedule: None,
            timeout_secs: None,
            memory: None,
            memory_scope: None,
            durability_mode: None,
            resource_limits: None,
        };
        let created = adapter.create_background_agent(request).unwrap();
        let id = created["id"].as_str().unwrap();

        let result = adapter.delete_background_agent(id).unwrap();
        assert_eq!(result["deleted"], true);
    }

    #[test]
    fn test_send_and_list_messages() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_background_agent(BackgroundAgentCreateRequest {
                name: "Messaging".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("messaging task".to_string()),
                input_template: None,
                schedule: None,
                timeout_secs: None,
                memory: None,
                memory_scope: None,
                durability_mode: None,
                resource_limits: None,
            })
            .unwrap();
        let task_id = created["id"].as_str().unwrap().to_string();

        adapter
            .send_background_agent_message(BackgroundAgentMessageRequest {
                id: task_id.clone(),
                message: "Hello from test".to_string(),
                source: Some("user".to_string()),
            })
            .unwrap();

        let messages = adapter
            .list_background_agent_messages(BackgroundAgentMessageListRequest {
                id: task_id,
                limit: Some(50),
            })
            .unwrap();
        let msgs = messages.as_array().unwrap();
        assert!(!msgs.is_empty());
    }

    #[test]
    fn test_parse_status() {
        assert!(BackgroundAgentStoreAdapter::parse_status("active").is_ok());
        assert!(BackgroundAgentStoreAdapter::parse_status("PAUSED").is_ok());
        assert!(BackgroundAgentStoreAdapter::parse_status("invalid").is_err());
    }

    #[test]
    fn test_parse_control_action() {
        assert!(BackgroundAgentStoreAdapter::parse_control_action("start").is_ok());
        assert!(BackgroundAgentStoreAdapter::parse_control_action("run_now").is_ok());
        assert!(BackgroundAgentStoreAdapter::parse_control_action("run-now").is_ok());
        assert!(BackgroundAgentStoreAdapter::parse_control_action("invalid").is_err());
    }

    #[test]
    fn test_read_trace_requires_non_empty_trace_id() {
        let (adapter, _dir, _guard) = setup();
        let result = adapter.read_background_agent_trace(BackgroundAgentTraceReadRequest {
            trace_id: String::new(),
            line_limit: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_read_trace_includes_output_ref_tail_lines() {
        let (adapter, temp_dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);

        let created = adapter
            .create_background_agent(BackgroundAgentCreateRequest {
                name: "Trace Reader".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("trace task".to_string()),
                input_template: None,
                schedule: None,
                timeout_secs: None,
                memory: None,
                memory_scope: None,
                durability_mode: None,
                resource_limits: None,
            })
            .unwrap();
        let session_id = created["chat_session_id"].as_str().unwrap().to_string();

        let output_path = temp_dir.path().join("trace-output.txt");
        std::fs::write(&output_path, "line-1\nline-2\nline-3\nline-4\n").unwrap();

        let mut trace = crate::models::ToolTrace::tool_call_completed(
            &session_id,
            "turn-1",
            "call-1",
            "bash",
            crate::models::ToolCallCompletion {
                output: None,
                output_ref: Some(output_path.to_string_lossy().to_string()),
                success: true,
                duration_ms: Some(8),
                error: None,
            },
        );
        trace.id = "trace-id-1".to_string();
        adapter.storage.tool_traces().append(&trace).unwrap();

        let value = adapter
            .read_background_agent_trace(BackgroundAgentTraceReadRequest {
                trace_id: "trace-id-1".to_string(),
                line_limit: Some(2),
            })
            .unwrap();

        assert_eq!(value["trace"]["id"], "trace-id-1");
        assert_eq!(value["output"]["total_lines"], 4);
        assert_eq!(value["output"]["lines"][0], "line-3");
        assert_eq!(value["output"]["lines"][1], "line-4");
    }
}
