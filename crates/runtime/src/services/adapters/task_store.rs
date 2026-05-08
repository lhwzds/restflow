//! TaskStore adapter backed by legacy TaskStorage persistence.

use crate::boundary::task::parse_control_action;
use crate::models::{
    ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceQuery, Task, TaskMessageSource,
    TaskStatus,
};
use crate::services::session::SessionService;
use crate::services::task_command::{TaskCommandService, TaskExecutionMode};
use crate::storage::{AgentStorage, TaskStorage};
use crate::telemetry::get_execution_timeline;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use tools::ToolError;
use types::AgentOperationAssessor;
use types::store::{
    TaskArtifactListRequest, TaskControlRequest, TaskConvertSessionRequest, TaskCreateRequest,
    TaskDeleteRequest, TaskMessageListRequest, TaskMessageRequest, TaskProgressRequest, TaskStore,
    TaskTraceListRequest, TaskTraceReadRequest, TaskUpdateRequest,
};
use types::{
    DEFAULT_TASK_MESSAGE_LIST_LIMIT, DEFAULT_TASK_PROGRESS_EVENT_LIMIT,
    DEFAULT_TASK_TRACE_LINE_LIMIT, DEFAULT_TASK_TRACE_LIST_LIMIT,
};

#[derive(Clone)]
pub struct TaskStoreAdapter {
    storage: TaskStorage,
    #[allow(dead_code)]
    agent_storage: AgentStorage,
    command_service: TaskCommandService,
}

impl TaskStoreAdapter {
    pub fn new(
        storage: TaskStorage,
        agent_storage: AgentStorage,
        session_service: SessionService,
    ) -> Self {
        let command_service = TaskCommandService::new(
            storage.clone(),
            agent_storage.clone(),
            session_service,
            None,
        );
        Self {
            storage,
            agent_storage,
            command_service,
        }
    }

    pub fn with_assessor(mut self, assessor: std::sync::Arc<dyn AgentOperationAssessor>) -> Self {
        self.command_service = self.command_service.with_assessor(assessor);
        self
    }

    fn parse_status(status: &str) -> Result<TaskStatus, ToolError> {
        match status.trim().to_lowercase().as_str() {
            "active" => Ok(TaskStatus::Active),
            "paused" => Ok(TaskStatus::Paused),
            "running" => Ok(TaskStatus::Running),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            "interrupted" => Ok(TaskStatus::Interrupted),
            _ => Err(ToolError::Tool(format!("Unknown status: {}", status))),
        }
    }

    fn run_async<T, Fut, E>(&self, future: Fut) -> Result<T, ToolError>
    where
        T: Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        E: Into<ToolError> + Send + 'static,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current()
            && matches!(
                handle.runtime_flavor(),
                tokio::runtime::RuntimeFlavor::MultiThread
            )
        {
            return tokio::task::block_in_place(|| handle.block_on(future).map_err(Into::into));
        }

        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| ToolError::Tool(error.to_string()))?
                .block_on(future)
                .map_err(Into::into)
        })
        .join()
        .map_err(|_| ToolError::Tool("task async bridge thread panicked".to_string()))?
    }

    fn parse_message_source(source: Option<&str>) -> Result<TaskMessageSource, ToolError> {
        match source.map(|value| value.trim().to_lowercase()) {
            None => Ok(TaskMessageSource::User),
            Some(value) if value.is_empty() => Ok(TaskMessageSource::User),
            Some(value) if value == "user" => Ok(TaskMessageSource::User),
            Some(value) if value == "agent" => Ok(TaskMessageSource::Agent),
            Some(value) if value == "system" => Ok(TaskMessageSource::System),
            Some(value) => Err(ToolError::Tool(format!(
                "Unknown message source: {}",
                value
            ))),
        }
    }

    fn resolve_task_id(&self, id_or_prefix: &str) -> Result<String, ToolError> {
        Ok(self.storage.resolve_existing_task_id(id_or_prefix)?)
    }

    fn resolve_task(&self, id_or_prefix: &str) -> Result<Task, ToolError> {
        let resolved_id = self.resolve_task_id(id_or_prefix)?;
        self.storage
            .get_task(&resolved_id)?
            .ok_or_else(|| ToolError::Tool(format!("task {} not found", resolved_id)))
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

impl TaskStore for TaskStoreAdapter {
    fn create_task(&self, request: TaskCreateRequest) -> tools::Result<Value> {
        let command_service = self.command_service.clone();
        let outcome = self.run_async(async move {
            command_service
                .create_from_request(request, TaskExecutionMode::Guarded)
                .await
        })?;
        Ok(serde_json::to_value(outcome)?)
    }

    fn convert_session_to_task(&self, request: TaskConvertSessionRequest) -> tools::Result<Value> {
        let command_service = self.command_service.clone();
        let outcome = self.run_async(async move {
            command_service
                .convert_session(request, TaskExecutionMode::Guarded)
                .await
        })?;
        Ok(serde_json::to_value(outcome)?)
    }

    fn update_task(&self, request: TaskUpdateRequest) -> tools::Result<Value> {
        let command_service = self.command_service.clone();
        let outcome = self.run_async(async move {
            command_service
                .update_from_request(request, TaskExecutionMode::Guarded)
                .await
        })?;
        Ok(serde_json::to_value(outcome)?)
    }

    fn delete_task(&self, request: TaskDeleteRequest) -> tools::Result<Value> {
        let command_service = self.command_service.clone();
        let outcome = self.run_async(async move {
            command_service
                .delete_from_request(request, TaskExecutionMode::Guarded)
                .await
        })?;
        Ok(serde_json::to_value(outcome)?)
    }

    fn list_tasks(&self, status: Option<String>) -> tools::Result<Value> {
        let tasks = if let Some(status) = status {
            let status = Self::parse_status(&status)?;
            self.storage.list_tasks_by_status(status)?
        } else {
            self.storage.list_tasks()?
        };

        Ok(serde_json::to_value(tasks)?)
    }

    fn control_task(&self, request: TaskControlRequest) -> tools::Result<Value> {
        let _ = parse_control_action(&request.action)?;
        let command_service = self.command_service.clone();
        let outcome = self.run_async(async move {
            command_service
                .control_from_request(request, TaskExecutionMode::Guarded)
                .await
        })?;
        Ok(serde_json::to_value(outcome)?)
    }

    fn get_task_progress(&self, request: TaskProgressRequest) -> tools::Result<Value> {
        let resolved_id = self.resolve_task_id(&request.id)?;
        let progress = self.command_service.progress(
            &resolved_id,
            request
                .event_limit
                .unwrap_or(DEFAULT_TASK_PROGRESS_EVENT_LIMIT)
                .max(1),
        )?;
        Ok(serde_json::to_value(progress)?)
    }

    fn send_task_message(&self, request: TaskMessageRequest) -> tools::Result<Value> {
        let source = Self::parse_message_source(request.source.as_deref())?;
        let resolved_id = self.resolve_task_id(&request.id)?;
        let message = self
            .command_service
            .send_message(&resolved_id, request.message, source)?;
        Ok(serde_json::to_value(message)?)
    }

    fn list_task_messages(&self, request: TaskMessageListRequest) -> tools::Result<Value> {
        let resolved_id = self.resolve_task_id(&request.id)?;
        let messages = self.storage.list_task_messages(
            &resolved_id,
            request
                .limit
                .unwrap_or(DEFAULT_TASK_MESSAGE_LIST_LIMIT)
                .max(1),
        )?;
        Ok(serde_json::to_value(messages)?)
    }

    fn list_task_artifacts(&self, request: TaskArtifactListRequest) -> tools::Result<Value> {
        self.resolve_task_id(&request.id)?;
        Ok(json!([]))
    }

    fn list_task_traces(&self, request: TaskTraceListRequest) -> tools::Result<Value> {
        let limit = request
            .limit
            .unwrap_or(DEFAULT_TASK_TRACE_LIST_LIMIT)
            .max(1);
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

    fn read_task_trace(&self, request: TaskTraceReadRequest) -> tools::Result<Value> {
        let trace_id = request.trace_id.trim();
        if trace_id.is_empty() {
            return Err(ToolError::Tool("trace_id must not be empty".to_string()));
        }

        let limit = request
            .line_limit
            .unwrap_or(DEFAULT_TASK_TRACE_LINE_LIMIT)
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
    use crate::services::session::SessionService;
    use crate::storage::{ExecutionTraceStorage, SessionStorage};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tempfile::tempdir;
    use types::assessment::{
        AgentOperationAssessor, OperationAssessment, OperationAssessmentIntent,
    };
    use types::request::TaskSchedule as ContractTaskSchedule;
    use types::store::{
        AgentCreateRequest, AgentUpdateRequest, TaskArtifactListRequest, TaskControlRequest,
        TaskConvertSessionRequest, TaskCreateRequest, TaskDeleteRequest, TaskMessageListRequest,
        TaskMessageRequest, TaskStore, TaskTraceReadRequest, TaskUpdateRequest,
    };
    use types::{ContractRunSpawnRequest, ToolError};

    struct MockAssessor;
    struct WarningAssessor;

    #[async_trait]
    impl AgentOperationAssessor for MockAssessor {
        async fn assess_agent_create(
            &self,
            _request: AgentCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "create_agent",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_agent_update(
            &self,
            _request: AgentUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "update_agent",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_task_create(
            &self,
            _request: TaskCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "create_task",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_task_convert_session(
            &self,
            _request: TaskConvertSessionRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "convert_session_to_task",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_task_update(
            &self,
            _request: TaskUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "update_task",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_task_delete(
            &self,
            _request: TaskDeleteRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "delete_task",
                OperationAssessmentIntent::Save,
                vec![],
            ))
        }

        async fn assess_task_control(
            &self,
            _request: TaskControlRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "control_task",
                OperationAssessmentIntent::Run,
            ))
        }

        async fn assess_task_template(
            &self,
            operation: &str,
            intent: OperationAssessmentIntent,
            _agent_ids: Vec<String>,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(operation, intent))
        }

        async fn assess_subagent_spawn(
            &self,
            operation: &str,
            _request: ContractRunSpawnRequest,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                operation,
                OperationAssessmentIntent::Run,
            ))
        }

        async fn assess_subagent_batch(
            &self,
            operation: &str,
            _requests: Vec<ContractRunSpawnRequest>,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                operation,
                OperationAssessmentIntent::Run,
            ))
        }
    }

    #[async_trait]
    impl AgentOperationAssessor for WarningAssessor {
        async fn assess_agent_create(
            &self,
            _request: AgentCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "create_agent",
                OperationAssessmentIntent::Save,
                vec![],
            ))
        }

        async fn assess_agent_update(
            &self,
            _request: AgentUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "update_agent",
                OperationAssessmentIntent::Save,
                vec![],
            ))
        }

        async fn assess_task_create(
            &self,
            _request: TaskCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "create_task",
                OperationAssessmentIntent::Save,
                vec![],
            ))
        }

        async fn assess_task_convert_session(
            &self,
            _request: TaskConvertSessionRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "convert_session_to_task",
                OperationAssessmentIntent::Save,
                vec![],
            ))
        }

        async fn assess_task_update(
            &self,
            _request: TaskUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "update_task",
                OperationAssessmentIntent::Save,
                vec![],
            ))
        }

        async fn assess_task_delete(
            &self,
            _request: TaskDeleteRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "delete_task",
                OperationAssessmentIntent::Save,
                vec![],
            ))
        }

        async fn assess_task_control(
            &self,
            _request: TaskControlRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "control_task",
                OperationAssessmentIntent::Run,
                vec![],
            ))
        }

        async fn assess_task_template(
            &self,
            operation: &str,
            intent: OperationAssessmentIntent,
            _agent_ids: Vec<String>,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                operation,
                intent,
                vec![],
            ))
        }

        async fn assess_subagent_spawn(
            &self,
            operation: &str,
            _request: ContractRunSpawnRequest,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                operation,
                OperationAssessmentIntent::Run,
                vec![],
            ))
        }

        async fn assess_subagent_batch(
            &self,
            operation: &str,
            _requests: Vec<ContractRunSpawnRequest>,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                operation,
                OperationAssessmentIntent::Run,
                vec![],
            ))
        }
    }

    fn default_schedule() -> ContractTaskSchedule {
        ContractTaskSchedule::Interval {
            interval_ms: 60_000,
            start_at: None,
        }
    }

    fn setup() -> (
        TaskStoreAdapter,
        tempfile::TempDir,
        std::sync::MutexGuard<'static, ()>,
    ) {
        setup_with_assessor(Arc::new(MockAssessor))
    }

    fn setup_with_assessor(
        assessor: Arc<dyn AgentOperationAssessor>,
    ) -> (
        TaskStoreAdapter,
        tempfile::TempDir,
        std::sync::MutexGuard<'static, ()>,
    ) {
        let guard = prompt_files::agents_dir_env_lock();
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let bg_storage = TaskStorage::new(db.clone()).unwrap();
        let agent_storage = AgentStorage::new(db.clone()).unwrap();
        let chat_storage = crate::storage::ChatSessionStorage::new(db.clone()).unwrap();
        let trace_storage = ExecutionTraceStorage::new(db.clone()).unwrap();
        let session_storage = SessionStorage::new(chat_storage, trace_storage);
        let session_service = SessionService::new(
            session_storage,
            Some(agent_storage.clone()),
            bg_storage.clone(),
        );
        let prev_disable_file_catalog = std::env::var_os("RESTFLOW_DISABLE_AGENT_FILE_CATALOG");
        unsafe { std::env::set_var("RESTFLOW_DISABLE_AGENT_FILE_CATALOG", "1") };
        // Create a default agent for referencing
        let agent = crate::models::AgentNode::default();
        agent_storage
            .create_agent("test-agent".to_string(), agent)
            .unwrap();
        unsafe {
            match prev_disable_file_catalog {
                Some(value) => std::env::set_var("RESTFLOW_DISABLE_AGENT_FILE_CATALOG", value),
                None => std::env::remove_var("RESTFLOW_DISABLE_AGENT_FILE_CATALOG"),
            }
        }

        (
            TaskStoreAdapter::new(bg_storage, agent_storage, session_service)
                .with_assessor(assessor),
            temp_dir,
            guard,
        )
    }

    fn get_agent_id(adapter: &TaskStoreAdapter) -> String {
        let agents = adapter.agent_storage.list_agents().unwrap();
        agents[0].id.clone()
    }

    #[test]
    fn setup_returns_task_store_adapter() {
        let (adapter, _dir, _guard) = setup();
        let _: &TaskStoreAdapter = &adapter;
    }

    #[test]
    fn test_create_and_list_task() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let request = TaskCreateRequest {
            name: "Test BG Task".to_string(),
            agent_id,
            chat_session_id: None,
            input: Some("Do something".to_string()),
            input_template: None,
            schedule: default_schedule(),
            timeout_secs: None,
            resource_limits: None,
            preview: false,
            approval_id: None,
        };
        let created = adapter.create_task(request).unwrap();
        assert_eq!(created["status"], "executed");
        assert!(created["result"]["id"].as_str().is_some());

        let list = adapter.list_tasks(None).unwrap();
        let tasks = list.as_array().unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn test_create_task_accepts_agent_id() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_task(TaskCreateRequest {
                name: "Explicit Agent".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("use explicit agent id".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: false,
                approval_id: None,
            })
            .expect("agent id should resolve");
        assert_eq!(created["status"], "executed");
        assert!(created["result"]["id"].as_str().is_some());
    }

    #[test]
    fn test_convert_session_to_task_binds_existing_session() {
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
        let session = adapter
            .command_service
            .create_session_for_test(session)
            .unwrap();

        let converted = adapter
            .convert_session_to_task(TaskConvertSessionRequest {
                session_id: session.id.clone(),
                name: Some("Converted Task".to_string()),
                schedule: None,
                input: None,
                timeout_secs: Some(120),
                resource_limits: None,
                run_now: Some(false),
                preview: false,
                approval_id: None,
            })
            .unwrap();

        assert_eq!(converted["status"], "executed");
        assert_eq!(converted["result"]["source_session_id"], session.id);
        assert_eq!(converted["result"]["source_session_agent_id"], agent_id);
        assert_eq!(converted["result"]["run_now"], false);
        assert_eq!(converted["result"]["task"]["chat_session_id"], session.id);
        assert_eq!(converted["result"]["task"]["agent_id"], agent_id);
        assert_eq!(
            converted["result"]["task"]["input"],
            "Please continue this task."
        );
        assert_eq!(converted["result"]["task"]["name"], "Converted Task");
    }

    #[test]
    fn test_create_task_returns_confirmation_required_for_warning_assessment() {
        let (adapter, _dir, _guard) = setup_with_assessor(Arc::new(WarningAssessor));
        let agent_id = get_agent_id(&adapter);

        let created = adapter
            .create_task(TaskCreateRequest {
                name: "Guarded Create".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("guarded".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: false,
                approval_id: None,
            })
            .unwrap();

        assert_eq!(created["status"], "confirmation_required");
        assert_eq!(
            adapter.list_tasks(None).unwrap().as_array().unwrap().len(),
            0
        );
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
        let session = adapter
            .command_service
            .create_session_for_test(session)
            .unwrap();

        let error = adapter
            .convert_session_to_task(TaskConvertSessionRequest {
                session_id: session.id,
                name: None,
                schedule: None,
                input: None,
                timeout_secs: None,
                resource_limits: None,
                run_now: Some(false),
                preview: false,
                approval_id: None,
            })
            .expect_err("conversion should fail when no input can be derived");
        assert!(
            error
                .to_string()
                .contains(crate::services::task_conversion::MISSING_CONVERSION_INPUT_ERROR),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn test_convert_session_returns_confirmation_required_for_warning_assessment() {
        let (adapter, _dir, _guard) = setup_with_assessor(Arc::new(WarningAssessor));
        let agent_id = get_agent_id(&adapter);

        let mut session = crate::models::ChatSession::new(
            agent_id,
            crate::models::ModelId::Gpt5.as_serialized_str().to_string(),
        );
        session.add_message(crate::models::ChatMessage::user("Continue guarded task"));
        let session = adapter
            .command_service
            .create_session_for_test(session)
            .unwrap();

        let converted = adapter
            .convert_session_to_task(TaskConvertSessionRequest {
                session_id: session.id,
                name: Some("Guarded Convert".to_string()),
                schedule: None,
                input: None,
                timeout_secs: None,
                resource_limits: None,
                run_now: Some(false),
                preview: false,
                approval_id: None,
            })
            .unwrap();

        assert_eq!(converted["status"], "confirmation_required");
        assert_eq!(
            adapter.list_tasks(None).unwrap().as_array().unwrap().len(),
            0
        );
    }

    #[test]
    fn test_delete_task() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let request = TaskCreateRequest {
            name: "Delete Me".to_string(),
            agent_id,
            chat_session_id: None,
            input: Some("task to delete".to_string()),
            input_template: None,
            schedule: default_schedule(),
            timeout_secs: None,
            resource_limits: None,
            preview: false,
            approval_id: None,
        };
        let created = adapter.create_task(request).unwrap();
        let id = created["result"]["id"].as_str().unwrap();

        let preview = adapter
            .delete_task(TaskDeleteRequest {
                id: id.to_string(),
                preview: true,
                approval_id: None,
            })
            .unwrap();
        assert_eq!(preview["status"], "preview");

        let token = preview["assessment"]["approval_id"]
            .as_str()
            .expect("preview should return confirmation token")
            .to_string();
        let result = adapter
            .delete_task(TaskDeleteRequest {
                id: id.to_string(),
                preview: false,
                approval_id: Some(token),
            })
            .unwrap();
        assert_eq!(result["status"], "executed");
        assert_eq!(result["result"]["deleted"], true);
    }

    #[test]
    fn test_delete_task_returns_resolved_id_for_prefix() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_task(TaskCreateRequest {
                name: "Delete Prefix".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("task to delete".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: false,
                approval_id: None,
            })
            .unwrap();
        let id = created["result"]["id"].as_str().unwrap().to_string();
        let prefix = &id[..8];

        let preview = adapter
            .delete_task(TaskDeleteRequest {
                id: prefix.to_string(),
                preview: true,
                approval_id: None,
            })
            .unwrap();
        let token = preview["assessment"]["approval_id"]
            .as_str()
            .expect("preview should return confirmation token")
            .to_string();
        let result = adapter
            .delete_task(TaskDeleteRequest {
                id: prefix.to_string(),
                preview: false,
                approval_id: Some(token),
            })
            .unwrap();
        assert_eq!(result["result"]["id"], id);
        assert_eq!(result["result"]["deleted"], true);
    }

    #[test]
    fn test_update_task_returns_confirmation_required_for_warning_assessment() {
        let (adapter, _dir, _guard) = setup_with_assessor(Arc::new(WarningAssessor));
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_task(TaskCreateRequest {
                name: "Update Guarded".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("update".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: true,
                approval_id: None,
            })
            .unwrap();
        let preview_token = created["assessment"]["approval_id"]
            .as_str()
            .expect("preview token")
            .to_string();
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_task(TaskCreateRequest {
                name: "Update Guarded".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("update".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: false,
                approval_id: Some(preview_token),
            })
            .unwrap();
        let id = created["result"]["id"].as_str().unwrap().to_string();

        let updated = adapter
            .update_task(TaskUpdateRequest {
                id: id.clone(),
                name: Some("Updated Name".to_string()),
                description: None,
                agent_id: None,
                chat_session_id: None,
                input: None,
                input_template: None,
                schedule: None,
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                preview: false,
                approval_id: None,
            })
            .unwrap();

        assert_eq!(updated["status"], "confirmation_required");
        let stored = adapter.storage.get_task(&id).unwrap().unwrap();
        assert_eq!(stored.name, "Update Guarded");
    }

    #[test]
    fn test_control_task_returns_confirmation_required_for_warning_assessment() {
        let (adapter, _dir, _guard) = setup_with_assessor(Arc::new(WarningAssessor));
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_task(TaskCreateRequest {
                name: "Control Guarded".to_string(),
                agent_id: agent_id.clone(),
                chat_session_id: None,
                input: Some("control".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: true,
                approval_id: None,
            })
            .unwrap();
        let preview_token = created["assessment"]["approval_id"]
            .as_str()
            .expect("preview token")
            .to_string();
        let created = adapter
            .create_task(TaskCreateRequest {
                name: "Control Guarded".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("control".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: false,
                approval_id: Some(preview_token),
            })
            .unwrap();
        let id = created["result"]["id"].as_str().unwrap().to_string();

        let updated = adapter
            .control_task(TaskControlRequest {
                id: id.clone(),
                action: "pause".to_string(),
                preview: false,
                approval_id: None,
            })
            .unwrap();

        assert_eq!(updated["status"], "confirmation_required");
        let stored = adapter.storage.get_task(&id).unwrap().unwrap();
        assert_eq!(stored.status.as_str(), "active");
    }

    #[test]
    fn test_send_and_list_messages() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_task(TaskCreateRequest {
                name: "Messaging".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("messaging task".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: false,
                approval_id: None,
            })
            .unwrap();
        let task_id = created["result"]["id"].as_str().unwrap().to_string();

        adapter
            .send_task_message(TaskMessageRequest {
                id: task_id.clone(),
                message: "Hello from test".to_string(),
                source: Some("user".to_string()),
            })
            .unwrap();

        let messages = adapter
            .list_task_messages(TaskMessageListRequest {
                id: task_id,
                limit: Some(50),
            })
            .unwrap();
        let msgs = messages.as_array().unwrap();
        assert!(!msgs.is_empty());
    }

    #[test]
    fn test_list_task_artifacts_resolves_prefix() {
        let (adapter, _dir, _guard) = setup();
        let agent_id = get_agent_id(&adapter);
        let created = adapter
            .create_task(TaskCreateRequest {
                name: "Artifacts".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("artifacts task".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: false,
                approval_id: None,
            })
            .unwrap();
        let id = created["result"]["id"].as_str().unwrap().to_string();
        let prefix = &id[..8];

        let value = adapter
            .list_task_artifacts(TaskArtifactListRequest {
                id: prefix.to_string(),
            })
            .unwrap();
        assert!(value.as_array().is_some());
    }

    #[test]
    fn test_parse_status() {
        assert!(TaskStoreAdapter::parse_status("active").is_ok());
        assert!(TaskStoreAdapter::parse_status("PAUSED").is_ok());
        assert!(TaskStoreAdapter::parse_status("invalid").is_err());
    }

    #[test]
    fn test_parse_control_action() {
        assert!(parse_control_action("start").is_ok());
        assert!(parse_control_action("run_now").is_ok());
        assert!(parse_control_action("run-now").is_ok());
        assert!(parse_control_action("invalid").is_err());
    }

    #[test]
    fn test_read_trace_requires_non_empty_trace_id() {
        let (adapter, _dir, _guard) = setup();
        let result = adapter.read_task_trace(TaskTraceReadRequest {
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
            .create_task(TaskCreateRequest {
                name: "Trace Reader".to_string(),
                agent_id,
                chat_session_id: None,
                input: Some("trace task".to_string()),
                input_template: None,
                schedule: default_schedule(),
                timeout_secs: None,
                resource_limits: None,
                preview: false,
                approval_id: None,
            })
            .unwrap();
        let task_id = created["result"]["id"].as_str().unwrap().to_string();
        let session_id = created["result"]["chat_session_id"]
            .as_str()
            .unwrap()
            .to_string();

        let output_path = temp_dir.path().join("trace-output.txt");
        std::fs::write(&output_path, "line-1\nline-2\nline-3\nline-4\n").unwrap();

        let trace = ai::telemetry::RestflowTrace::new("run-1", &session_id, &task_id, "agent-1");
        let event = crate::models::execution_trace_builders::with_trace_context(
            crate::models::execution_trace_builders::tool_call(
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
            ),
            &trace,
        );
        adapter.storage.execution_traces().store(&event).unwrap();

        let value = adapter
            .read_task_trace(TaskTraceReadRequest {
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
