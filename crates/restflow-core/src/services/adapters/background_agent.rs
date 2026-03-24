//! BackgroundAgentStore adapter backed by BackgroundAgentStorage.

use crate::boundary::background_agent::{
    convert_session_request_to_options, create_request_to_spec, resolve_agent_id_alias,
    update_request_to_patch,
};
use crate::models::{
    BackgroundAgentControlAction, BackgroundAgentStatus, BackgroundMessageSource,
    ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceQuery,
};
use crate::services::background_agent_conversion::{
    build_convert_session_spec, ConvertSessionSpecOptions,
};
use crate::storage::{AgentStorage, BackgroundAgentStorage};
use crate::telemetry::get_execution_timeline;
use restflow_tools::ToolError;
use restflow_traits::store::{
    BackgroundAgentControlRequest, BackgroundAgentConvertSessionRequest,
    BackgroundAgentCreateRequest, BackgroundAgentDeliverableListRequest,
    BackgroundAgentMessageListRequest, BackgroundAgentMessageRequest,
    BackgroundAgentProgressRequest, BackgroundAgentStore, BackgroundAgentTraceListRequest,
    BackgroundAgentTraceReadRequest, BackgroundAgentUpdateRequest,
};
use restflow_traits::{
    DEFAULT_BG_MESSAGE_LIST_LIMIT, DEFAULT_BG_PROGRESS_EVENT_LIMIT, DEFAULT_BG_TRACE_LINE_LIMIT,
    DEFAULT_BG_TRACE_LIST_LIMIT,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

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

    fn resolve_agent_id(&self, id_or_prefix: &str) -> Result<String, ToolError> {
        resolve_agent_id_alias(
            id_or_prefix,
            || self.agent_storage.resolve_default_agent_id(),
            |trimmed| self.agent_storage.resolve_existing_agent_id(trimmed),
        )
        .map_err(Into::into)
    }

    fn resolve_task_id(&self, id_or_prefix: &str) -> Result<String, ToolError> {
        Ok(self.storage.resolve_existing_task_id(id_or_prefix)?)
    }

    fn resolve_task(
        &self,
        id_or_prefix: &str,
    ) -> Result<crate::models::BackgroundAgent, ToolError> {
        let resolved_id = self.resolve_task_id(id_or_prefix)?;
        self.storage
            .get_task(&resolved_id)?
            .ok_or_else(|| ToolError::Tool(format!("background agent {} not found", resolved_id)))
    }

    fn task_trace_target(&self, task_id_or_prefix: &str) -> Result<(String, String), ToolError> {
        let task = self.resolve_task(task_id_or_prefix)?;
        let resolved_id = task.id.clone();
        let session_id = task.chat_session_id.trim();
        let session_id = if session_id.is_empty() {
            task.id.clone()
        } else {
            session_id.to_string()
        };
        Ok((resolved_id, session_id))
    }

    fn all_trace_targets(&self) -> Result<Vec<(String, String)>, ToolError> {
        let mut seen = HashSet::new();
        let mut targets = Vec::new();
        for task in self.storage.list_tasks()? {
            let session_id = if task.chat_session_id.trim().is_empty() {
                task.id.clone()
            } else {
                task.chat_session_id.clone()
            };
            let dedupe_key = format!("{}:{}", task.id, session_id);
            if seen.insert(dedupe_key) {
                targets.push((task.id, session_id));
            }
        }
        Ok(targets)
    }

    fn trace_query(task_id: &str, session_id: &str) -> ExecutionTraceQuery {
        ExecutionTraceQuery {
            task_id: Some(task_id.to_string()),
            session_id: Some(session_id.to_string()),
            ..ExecutionTraceQuery::default()
        }
    }

    fn list_trace_events(
        &self,
        task_id: &str,
        session_id: &str,
    ) -> Result<Vec<ExecutionTraceEvent>, ToolError> {
        self.storage
            .execution_traces()
            .query(&Self::trace_query(task_id, session_id))
            .map_err(|e| ToolError::Tool(format!("failed to list traces: {}", e)))
    }

    fn event_run_id(event: &ExecutionTraceEvent) -> Option<String> {
        event.run_id.clone().or_else(|| {
            event
                .turn_id
                .as_deref()
                .map(|turn_id| turn_id.strip_prefix("run-").unwrap_or(turn_id).to_string())
        })
    }

    fn build_trace_summary(events: &[ExecutionTraceEvent]) -> Option<Value> {
        let first = events.first()?;
        let run_id = Self::event_run_id(first)?;
        let turn_id = first
            .turn_id
            .clone()
            .unwrap_or_else(|| format!("run-{run_id}"));
        let session_id = first
            .session_id
            .clone()
            .unwrap_or_else(|| first.task_id.clone());
        let mut status = "running".to_string();
        let mut started_at_ms = Some(first.timestamp);
        let mut ended_at_ms = None;
        let mut last_event_at_ms = first.timestamp;
        let mut tool_call_count = 0usize;
        let mut message_count = 0usize;
        let mut llm_call_count = 0usize;

        for event in events {
            last_event_at_ms = last_event_at_ms.max(event.timestamp);
            match event.category {
                ExecutionTraceCategory::ToolCall => tool_call_count += 1,
                ExecutionTraceCategory::Message => message_count += 1,
                ExecutionTraceCategory::LlmCall => llm_call_count += 1,
                ExecutionTraceCategory::Lifecycle => {
                    if let Some(lifecycle) = event.lifecycle.as_ref() {
                        status = lifecycle.status.clone();
                        if lifecycle.status == "started" {
                            started_at_ms = Some(event.timestamp);
                        }
                        if matches!(
                            lifecycle.status.as_str(),
                            "completed" | "failed" | "interrupted"
                        ) {
                            ended_at_ms = Some(event.timestamp);
                        }
                    }
                }
                _ => {}
            }
        }

        Some(json!({
            "trace_id": run_id,
            "run_id": run_id,
            "parent_run_id": first.parent_run_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "scope_id": first.task_id,
            "actor_id": first.agent_id,
            "status": status,
            "started_at_ms": started_at_ms,
            "ended_at_ms": ended_at_ms,
            "last_event_at_ms": last_event_at_ms,
            "event_count": events.len(),
            "tool_call_count": tool_call_count,
            "message_count": message_count,
            "llm_call_count": llm_call_count,
        }))
    }

    fn build_artifact_previews(events: &[ExecutionTraceEvent], line_limit: usize) -> Vec<Value> {
        events
            .iter()
            .filter_map(|event| {
                let tool_call = event.tool_call.as_ref()?;
                let path = tool_call.output_ref.as_ref()?;
                let content = std::fs::read_to_string(path).ok()?;
                let all_lines = content.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
                let total_lines = all_lines.len();
                let preview_lines = all_lines
                    .into_iter()
                    .rev()
                    .take(line_limit)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>();
                Some(json!({
                    "event_id": event.id,
                    "tool_call_id": tool_call.tool_call_id,
                    "path": path,
                    "total_lines": total_lines,
                    "lines": preview_lines,
                }))
            })
            .collect()
    }
}

impl BackgroundAgentStore for BackgroundAgentStoreAdapter {
    fn create_background_agent(
        &self,
        request: BackgroundAgentCreateRequest,
    ) -> restflow_tools::Result<Value> {
        let resolved_agent_id = self.resolve_agent_id(&request.agent_id)?;
        let spec = create_request_to_spec(request, resolved_agent_id)?;
        let task = self.storage.create_background_agent(spec)?;
        Ok(serde_json::to_value(task)?)
    }

    fn convert_session_to_background_agent(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> restflow_tools::Result<Value> {
        let session_id = request.session_id.trim();
        if session_id.is_empty() {
            return Err(ToolError::Tool("session_id must not be empty".to_string()));
        }

        let session = self
            .storage
            .chat_sessions()
            .get(session_id)?
            .ok_or_else(|| ToolError::Tool(format!("Session not found: {}", session_id)))?;
        let boundary = convert_session_request_to_options(request)?;

        let spec = build_convert_session_spec(
            &session,
            ConvertSessionSpecOptions {
                name: boundary.name,
                description: None,
                schedule: Some(boundary.schedule),
                input: boundary.input,
                notification: None,
                execution_mode: None,
                timeout_secs: boundary.timeout_secs,
                memory: boundary.memory,
                durability_mode: boundary.durability_mode,
                resource_limits: boundary.resource_limits,
                prerequisites: Vec::new(),
                continuation: None,
            },
        )
        .map_err(|e| ToolError::Tool(e.to_string()))?;

        let mut task = self.storage.create_background_agent(spec)?;
        let run_now = boundary.run_now;
        if run_now {
            task = self
                .storage
                .control_background_agent(&task.id, BackgroundAgentControlAction::RunNow)?;
        }

        Ok(json!({
            "task": task,
            "source_session": {
                "id": session.id,
                "agent_id": session.agent_id,
            },
            "run_now": run_now,
        }))
    }

    fn update_background_agent(
        &self,
        request: BackgroundAgentUpdateRequest,
    ) -> restflow_tools::Result<Value> {
        let resolved_id = self.resolve_task_id(&request.id)?;
        let resolved_agent_id = request
            .agent_id
            .as_deref()
            .map(|id| self.resolve_agent_id(id))
            .transpose()?;
        let patch = update_request_to_patch(request, resolved_agent_id)?;
        let task = self.storage.update_background_agent(&resolved_id, patch)?;
        Ok(serde_json::to_value(task)?)
    }

    fn delete_background_agent(&self, id: &str) -> restflow_tools::Result<Value> {
        let resolved_id = self.resolve_task_id(id)?;
        let deleted = self.storage.delete_task(&resolved_id)?;
        Ok(json!({ "id": resolved_id, "deleted": deleted }))
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
            request
                .event_limit
                .unwrap_or(DEFAULT_BG_PROGRESS_EVENT_LIMIT)
                .max(1),
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
        let messages = self.storage.list_background_agent_messages(
            &resolved_id,
            request
                .limit
                .unwrap_or(DEFAULT_BG_MESSAGE_LIST_LIMIT)
                .max(1),
        )?;
        Ok(serde_json::to_value(messages)?)
    }

    fn list_background_agent_deliverables(
        &self,
        request: BackgroundAgentDeliverableListRequest,
    ) -> restflow_tools::Result<Value> {
        let resolved_id = self.resolve_task_id(&request.id)?;
        let items = self.deliverable_storage.list_by_task(&resolved_id)?;
        Ok(serde_json::to_value(items)?)
    }

    fn list_background_agent_traces(
        &self,
        request: BackgroundAgentTraceListRequest,
    ) -> restflow_tools::Result<Value> {
        let limit = request.limit.unwrap_or(DEFAULT_BG_TRACE_LIST_LIMIT).max(1);
        let trace_targets = if let Some(task_id) = request.id.as_deref() {
            vec![self.task_trace_target(task_id)?]
        } else {
            self.all_trace_targets()?
        };

        let mut summaries = Vec::new();
        for (scope_id, session_id) in trace_targets {
            let events = self.list_trace_events(&scope_id, &session_id)?;
            let mut by_run = BTreeMap::<String, Vec<ExecutionTraceEvent>>::new();
            for event in events {
                let Some(run_id) = Self::event_run_id(&event) else {
                    continue;
                };
                by_run.entry(run_id).or_default().push(event);
            }
            summaries.extend(
                by_run
                    .into_values()
                    .filter_map(|events| Self::build_trace_summary(&events)),
            );
        }
        summaries.sort_by(|a, b| {
            b["last_event_at_ms"]
                .as_i64()
                .cmp(&a["last_event_at_ms"].as_i64())
                .then_with(|| b["run_id"].as_str().cmp(&a["run_id"].as_str()))
        });
        summaries.truncate(limit);
        let data = summaries;
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

        let limit = request
            .line_limit
            .unwrap_or(DEFAULT_BG_TRACE_LINE_LIMIT)
            .max(1);
        for (scope_id, session_id) in self.all_trace_targets()? {
            let query = ExecutionTraceQuery {
                task_id: Some(scope_id.clone()),
                session_id: Some(session_id.clone()),
                run_id: Some(trace_id.to_string()),
                limit: Some(limit),
                ..ExecutionTraceQuery::default()
            };
            let timeline = get_execution_timeline(self.storage.execution_traces(), &query)
                .map_err(|e| ToolError::Tool(format!("failed to read trace: {}", e)))?;
            if timeline.events.is_empty() {
                continue;
            }

            let summary = Self::build_trace_summary(&timeline.events)
                .ok_or_else(|| ToolError::Tool(format!("trace {} not found", trace_id)))?;
            let artifact_previews = Self::build_artifact_previews(&timeline.events, limit);
            return Ok(json!({
                "trace_id": trace_id,
                "summary": summary,
                "timeline": timeline,
                "artifact_previews": artifact_previews,
            }));
        }

        Err(ToolError::Tool(format!("trace {} not found", trace_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_files;
    use restflow_contracts::request::DurabilityMode as ContractDurabilityMode;
    use restflow_traits::store::BackgroundAgentStore;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (
        BackgroundAgentStoreAdapter,
        tempfile::TempDir,
        std::sync::MutexGuard<'static, ()>,
    ) {
        let guard = prompt_files::agents_dir_env_lock();
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let bg_storage = BackgroundAgentStorage::new(db.clone()).unwrap();
        let agent_storage = AgentStorage::new(db.clone()).unwrap();
        let deliverable_storage = crate::storage::DeliverableStorage::new(db).unwrap();

        let prompts_dir = temp_dir.path().join("state").join("agents");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        let prev_agents_dir = std::env::var_os(prompt_files::AGENTS_DIR_ENV);
        unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, &prompts_dir) };

        // Create a default agent for referencing
        let agent = crate::models::AgentNode::default();
        agent_storage
            .create_agent("test-agent".to_string(), agent)
            .unwrap();

        // Restore env var immediately after agent creation
        unsafe {
            match prev_agents_dir {
                Some(v) => std::env::set_var(prompt_files::AGENTS_DIR_ENV, v),
                None => std::env::remove_var(prompt_files::AGENTS_DIR_ENV),
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
    fn test_create_background_agent_accepts_default_agent_alias() {
        let (adapter, _dir, _guard) = setup();
        let created = adapter
            .create_background_agent(BackgroundAgentCreateRequest {
                name: "Default Alias".to_string(),
                agent_id: "default".to_string(),
                chat_session_id: None,
                input: Some("use default alias".to_string()),
                input_template: None,
                schedule: None,
                timeout_secs: None,
                memory: None,
                memory_scope: None,
                durability_mode: None,
                resource_limits: None,
            })
            .expect("default alias should resolve to the only/default agent");
        assert!(created["id"].as_str().is_some());
    }

    #[test]
    fn test_convert_session_to_background_agent_binds_existing_session() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);

        let mut session = crate::models::ChatSession::new(
            agent_id.clone(),
            crate::models::ModelId::Gpt5.as_serialized_str().to_string(),
        )
        .with_name("Session to Convert");
        session.add_message(crate::models::ChatMessage::user(
            "Please continue this task.",
        ));
        adapter.storage.chat_sessions().create(&session).unwrap();

        let converted = adapter
            .convert_session_to_background_agent(BackgroundAgentConvertSessionRequest {
                session_id: session.id.clone(),
                name: Some("Converted Task".to_string()),
                schedule: None,
                input: None,
                timeout_secs: Some(120),
                durability_mode: Some(ContractDurabilityMode::Async),
                memory: None,
                memory_scope: None,
                resource_limits: None,
                run_now: Some(false),
            })
            .unwrap();

        assert_eq!(converted["source_session"]["id"], session.id);
        assert_eq!(converted["source_session"]["agent_id"], agent_id);
        assert_eq!(converted["run_now"], false);
        assert_eq!(converted["task"]["chat_session_id"], session.id);
        assert_eq!(converted["task"]["agent_id"], agent_id);
        assert_eq!(converted["task"]["input"], "Please continue this task.");
        assert_eq!(converted["task"]["name"], "Converted Task");
    }

    #[test]
    fn test_convert_session_requires_input_or_existing_user_message() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);

        let session = crate::models::ChatSession::new(
            agent_id,
            crate::models::ModelId::Gpt5.as_serialized_str().to_string(),
        )
        .with_name("Empty Session");
        adapter.storage.chat_sessions().create(&session).unwrap();

        let error = adapter
            .convert_session_to_background_agent(BackgroundAgentConvertSessionRequest {
                session_id: session.id,
                name: None,
                schedule: None,
                input: None,
                timeout_secs: None,
                durability_mode: None,
                memory: None,
                memory_scope: None,
                resource_limits: None,
                run_now: Some(false),
            })
            .expect_err("expected conversion to fail without user input");

        assert!(
            error
                .to_string()
                .contains("no non-empty user message found"),
            "unexpected error: {}",
            error
        );
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
    fn test_delete_background_agent_returns_resolved_id_for_prefix() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_background_agent(BackgroundAgentCreateRequest {
                name: "Delete Prefix".to_string(),
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
            })
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        let prefix = &id[..8];

        let result = adapter.delete_background_agent(prefix).unwrap();
        assert_eq!(result["id"], id);
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
    fn test_list_background_agent_deliverables_resolves_prefix() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_background_agent(BackgroundAgentCreateRequest {
                name: "Deliverables".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("deliverables task".to_string()),
                input_template: None,
                schedule: None,
                timeout_secs: None,
                memory: None,
                memory_scope: None,
                durability_mode: None,
                resource_limits: None,
            })
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        let prefix = &id[..8];

        let value = adapter
            .list_background_agent_deliverables(BackgroundAgentDeliverableListRequest {
                id: prefix.to_string(),
            })
            .unwrap();
        assert!(value.as_array().is_some());
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
        let task_id = created["id"].as_str().unwrap().to_string();
        let session_id = created["chat_session_id"].as_str().unwrap().to_string();

        let output_path = temp_dir.path().join("trace-output.txt");
        std::fs::write(&output_path, "line-1\nline-2\nline-3\nline-4\n").unwrap();

        let trace =
            restflow_telemetry::RestflowTrace::new("run-1", &session_id, &task_id, "agent-1");
        let event = crate::models::ExecutionTraceEvent::tool_call(
            task_id,
            "agent-1",
            crate::models::ToolCallTrace {
                phase: crate::models::ToolCallPhase::Completed,
                tool_call_id: "call-1".to_string(),
                tool_name: "bash".to_string(),
                input: None,
                input_summary: None,
                output: None,
                output_ref: Some(output_path.to_string_lossy().to_string()),
                success: Some(true),
                error: None,
                duration_ms: Some(8),
            },
        )
        .with_trace_context(&trace);
        adapter.storage.execution_traces().store(&event).unwrap();

        let value = adapter
            .read_background_agent_trace(BackgroundAgentTraceReadRequest {
                trace_id: "run-1".to_string(),
                line_limit: Some(2),
            })
            .unwrap();

        assert_eq!(value["summary"]["run_id"], "run-1");
        assert_eq!(value["timeline"]["events"][0]["id"], event.id);
        assert_eq!(value["artifact_previews"][0]["total_lines"], 4);
        assert_eq!(value["artifact_previews"][0]["lines"][0], "line-3");
        assert_eq!(value["artifact_previews"][0]["lines"][1], "line-4");
    }
}
