use crate::boundary::background_agent::{
    convert_session_request_to_options, create_request_to_spec, parse_control_action,
    update_request_to_patch,
};
use crate::daemon::request_mapper::to_contract;
use crate::models::{
    Task, TaskControlAction, TaskConversionResult, TaskMessage, TaskMessageSource, TaskPatch,
    TaskProgress, TaskSpec,
};
use crate::services::background_agent_conversion::{
    ConvertSessionSpecOptions, build_convert_session_spec,
};
use crate::services::operation_assessment::{
    assessment_requires_confirmation, assessment_summary, ensure_assessment_confirmed,
};
use crate::services::session::SessionService;
use crate::storage::{AgentStorage, BackgroundAgentStorage, Storage};
use restflow_contracts::{DeleteWithIdResponse, ErrorKind, ErrorPayload};
use restflow_tools::ToolError;
use restflow_traits::store::{
    TaskControlRequest, TaskConvertSessionRequest, TaskCreateRequest, TaskDeleteRequest,
    TaskUpdateRequest,
};
use restflow_traits::{AgentOperationAssessor, OperationAssessment, TaskCommandOutcome};
use std::sync::Arc;

type CommandResult<T> = std::result::Result<T, TaskCommandError>;

#[derive(Debug, Clone)]
struct RequestGuard {
    preview: bool,
    approval_id: Option<String>,
}

impl RequestGuard {
    fn capture(preview: bool, approval_id: Option<String>) -> Self {
        Self {
            preview,
            approval_id,
        }
    }
}

struct PreparedSessionConversion {
    spec: TaskSpec,
    source_session_id: String,
    source_session_agent_id: String,
    run_now: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskExecutionMode {
    Guarded,
    Direct,
}

#[doc(hidden)]
pub type BackgroundAgentExecutionMode = TaskExecutionMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskCommandError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

#[doc(hidden)]
pub type BackgroundAgentCommandError = TaskCommandError;

impl std::fmt::Display for TaskCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Internal(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for TaskCommandError {}

impl From<TaskCommandError> for ToolError {
    fn from(error: TaskCommandError) -> Self {
        ToolError::Tool(error.to_string())
    }
}

impl TaskCommandError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    fn classify(message: String) -> Self {
        let normalized = message.trim().to_ascii_lowercase();
        if normalized.contains("not found") {
            Self::not_found(message)
        } else if normalized.contains("ambiguous")
            || normalized.contains("already exists")
            || normalized.contains("conflict")
        {
            Self::conflict(message)
        } else if normalized.contains("missing required field")
            || normalized.contains("must not be empty")
            || normalized.contains("invalid")
            || normalized.contains("unknown")
            || normalized.contains("required")
        {
            Self::validation(message)
        } else {
            Self::internal(message)
        }
    }

    fn from_tool_error(error: ToolError) -> Self {
        Self::classify(error.to_string())
    }

    fn from_anyhow(error: anyhow::Error) -> Self {
        Self::classify(error.to_string())
    }

    pub fn code(&self) -> i32 {
        match self {
            Self::Validation(_) => 400,
            Self::NotFound(_) => 404,
            Self::Conflict(_) => 409,
            Self::Internal(_) => 500,
        }
    }

    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Validation(_) => ErrorKind::Validation,
            Self::NotFound(_) => ErrorKind::NotFound,
            Self::Conflict(_) => ErrorKind::Conflict,
            Self::Internal(_) => ErrorKind::Internal,
        }
    }

    pub fn payload(&self) -> ErrorPayload {
        ErrorPayload::with_kind(self.code(), self.kind(), self.to_string(), None)
    }
}

#[derive(Clone)]
pub struct TaskCommandService {
    storage: BackgroundAgentStorage,
    agents: AgentStorage,
    session_service: SessionService,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
}

#[doc(hidden)]
pub type BackgroundAgentCommandService = TaskCommandService;

impl TaskCommandService {
    pub fn new(
        storage: BackgroundAgentStorage,
        agents: AgentStorage,
        session_service: SessionService,
        assessor: Option<Arc<dyn AgentOperationAssessor>>,
    ) -> Self {
        Self {
            storage,
            agents,
            session_service,
            assessor,
        }
    }

    pub fn from_storage(
        storage: &Storage,
        assessor: Option<Arc<dyn AgentOperationAssessor>>,
    ) -> Self {
        Self::new(
            storage.background_agents.clone(),
            storage.agents.clone(),
            SessionService::from_storage(storage),
            assessor,
        )
    }

    pub fn with_assessor(mut self, assessor: Arc<dyn AgentOperationAssessor>) -> Self {
        self.assessor = Some(assessor);
        self
    }

    fn create(&self, spec: TaskSpec) -> CommandResult<Task> {
        self.storage
            .create_background_agent(spec)
            .map_err(TaskCommandError::from_anyhow)
    }

    pub async fn create_from_request(
        &self,
        request: TaskCreateRequest,
        mode: TaskExecutionMode,
    ) -> CommandResult<TaskCommandOutcome<Task>> {
        let (guard, assessment, spec) = self.prepare_create(request).await?;
        self.finish_request(mode, guard, assessment, || self.create(spec))
    }

    fn update(&self, id: &str, patch: TaskPatch) -> CommandResult<Task> {
        self.storage
            .update_background_agent(id, patch)
            .map_err(TaskCommandError::from_anyhow)
    }

    pub async fn update_from_request(
        &self,
        request: TaskUpdateRequest,
        mode: TaskExecutionMode,
    ) -> CommandResult<TaskCommandOutcome<Task>> {
        let (guard, assessment, resolved_id, patch) = self.prepare_update(request).await?;
        self.finish_request(mode, guard, assessment, || self.update(&resolved_id, patch))
    }

    pub fn delete(&self, id: &str) -> CommandResult<bool> {
        self.storage
            .delete_task(id)
            .map_err(TaskCommandError::from_anyhow)
    }

    pub async fn delete_from_request(
        &self,
        request: TaskDeleteRequest,
        mode: TaskExecutionMode,
    ) -> CommandResult<TaskCommandOutcome<DeleteWithIdResponse>> {
        let (guard, assessment, resolved_id) = self.prepare_delete(request).await?;
        self.finish_request(mode, guard, assessment, || {
            let deleted = self.delete(&resolved_id)?;
            Ok(DeleteWithIdResponse {
                id: resolved_id,
                deleted,
            })
        })
    }

    fn control(&self, id: &str, action: TaskControlAction) -> CommandResult<Task> {
        self.storage
            .control_background_agent(id, action)
            .map_err(TaskCommandError::from_anyhow)
    }

    pub fn progress(&self, id: &str, event_limit: usize) -> CommandResult<TaskProgress> {
        self.storage
            .get_background_agent_progress(id, event_limit)
            .map_err(TaskCommandError::from_anyhow)
    }

    pub fn send_message(
        &self,
        id: &str,
        message: String,
        source: TaskMessageSource,
    ) -> CommandResult<TaskMessage> {
        self.storage
            .send_background_agent_message(id, message, source)
            .map_err(TaskCommandError::from_anyhow)
    }

    pub async fn control_from_request(
        &self,
        request: TaskControlRequest,
        mode: TaskExecutionMode,
    ) -> CommandResult<TaskCommandOutcome<Task>> {
        let (guard, assessment, resolved_id, action) = self.prepare_control(request).await?;
        self.finish_request(mode, guard, assessment, || {
            self.control(&resolved_id, action)
        })
    }

    pub async fn convert_session(
        &self,
        request: TaskConvertSessionRequest,
        mode: TaskExecutionMode,
    ) -> CommandResult<TaskCommandOutcome<TaskConversionResult>> {
        let (guard, assessment, prepared) = self.prepare_convert_session(request).await?;
        self.finish_request(mode, guard, assessment, || {
            self.execute_convert_session(prepared)
        })
    }

    pub fn resolve_default_or_existing_agent_id(&self, id_or_alias: &str) -> CommandResult<String> {
        crate::boundary::background_agent::resolve_agent_id_alias(
            id_or_alias,
            || self.agents.resolve_default_agent_id(),
            |trimmed| self.agents.resolve_existing_agent_id(trimmed),
        )
        .map_err(TaskCommandError::from_anyhow)
    }

    fn assessor(&self) -> CommandResult<Arc<dyn AgentOperationAssessor>> {
        self.assessor.clone().ok_or_else(|| {
            TaskCommandError::internal("Task capability assessment is unavailable in this runtime.")
        })
    }

    fn normalize_create_request(
        &self,
        mut request: TaskCreateRequest,
    ) -> CommandResult<TaskCreateRequest> {
        if request.agent_id.trim().is_empty() {
            return Err(TaskCommandError::validation("agent_id must not be empty"));
        }
        request.agent_id = self.resolve_default_or_existing_agent_id(&request.agent_id)?;
        Ok(request)
    }

    fn normalize_update_request(
        &self,
        mut request: TaskUpdateRequest,
    ) -> CommandResult<TaskUpdateRequest> {
        if request.id.trim().is_empty() {
            return Err(TaskCommandError::validation("id must not be empty"));
        }
        request.id = self
            .storage
            .resolve_existing_task_id(&request.id)
            .map_err(TaskCommandError::from_anyhow)?;
        if let Some(agent_id) = request.agent_id.clone() {
            if agent_id.trim().is_empty() {
                return Err(TaskCommandError::validation("agent_id must not be empty"));
            }
            request.agent_id = Some(self.resolve_default_or_existing_agent_id(&agent_id)?);
        }
        Ok(request)
    }

    fn normalize_control_request(
        &self,
        mut request: TaskControlRequest,
    ) -> CommandResult<(TaskControlRequest, TaskControlAction)> {
        if request.id.trim().is_empty() {
            return Err(TaskCommandError::validation("id must not be empty"));
        }
        request.id = self
            .storage
            .resolve_existing_task_id(&request.id)
            .map_err(TaskCommandError::from_anyhow)?;
        let action =
            parse_control_action(&request.action).map_err(TaskCommandError::from_tool_error)?;
        request.action = to_contract(action.clone()).map_err(TaskCommandError::from_anyhow)?;
        Ok((request, action))
    }

    fn normalize_delete_request(
        &self,
        mut request: TaskDeleteRequest,
    ) -> CommandResult<TaskDeleteRequest> {
        if request.id.trim().is_empty() {
            return Err(TaskCommandError::validation("id must not be empty"));
        }
        request.id = self
            .storage
            .resolve_existing_task_id(&request.id)
            .map_err(TaskCommandError::from_anyhow)?;
        Ok(request)
    }

    fn normalize_convert_session_request(
        &self,
        mut request: TaskConvertSessionRequest,
    ) -> TaskConvertSessionRequest {
        request.session_id = request.session_id.trim().to_string();
        request.run_now = Some(request.run_now.unwrap_or(false));
        request
    }

    fn validate_create_request(&self, request: &TaskCreateRequest) -> CommandResult<()> {
        if request.name.trim().is_empty() {
            return Err(TaskCommandError::validation("name must not be empty"));
        }
        Ok(())
    }

    fn validate_update_request(&self, request: &TaskUpdateRequest) -> CommandResult<()> {
        if let Some(name) = request.name.as_deref()
            && name.trim().is_empty()
        {
            return Err(TaskCommandError::validation("name must not be empty"));
        }
        Ok(())
    }

    fn validate_control_request(&self, request: &TaskControlRequest) -> CommandResult<()> {
        if request.action.trim().is_empty() {
            return Err(TaskCommandError::validation("action must not be empty"));
        }
        Ok(())
    }

    fn validate_delete_request(&self, request: &TaskDeleteRequest) -> CommandResult<()> {
        if request.id.trim().is_empty() {
            return Err(TaskCommandError::validation("id must not be empty"));
        }
        Ok(())
    }

    fn validate_convert_session_request(
        &self,
        request: &TaskConvertSessionRequest,
    ) -> CommandResult<()> {
        if request.session_id.is_empty() {
            return Err(TaskCommandError::validation("session_id must not be empty"));
        }
        if let Some(name) = request.name.as_deref()
            && name.trim().is_empty()
        {
            return Err(TaskCommandError::validation("name must not be empty"));
        }
        Ok(())
    }

    fn finish_mutation<T>(
        &self,
        assessment: OperationAssessment,
        preview: bool,
        approval_id: Option<&str>,
        execute: impl FnOnce() -> CommandResult<T>,
    ) -> CommandResult<TaskCommandOutcome<T>> {
        if preview {
            return Ok(TaskCommandOutcome::Preview { assessment });
        }
        if !assessment.blockers.is_empty() {
            return Ok(TaskCommandOutcome::Blocked { assessment });
        }
        if assessment_requires_confirmation(&assessment)
            && ensure_assessment_confirmed(&assessment, approval_id).is_err()
        {
            return Ok(TaskCommandOutcome::ConfirmationRequired { assessment });
        }
        Ok(TaskCommandOutcome::Executed { result: execute()? })
    }

    fn finish_direct_mutation<T>(
        &self,
        assessment: OperationAssessment,
        execute: impl FnOnce() -> CommandResult<T>,
    ) -> CommandResult<T> {
        if !assessment.blockers.is_empty() {
            return Err(TaskCommandError::classify(assessment_summary(&assessment)));
        }
        execute()
    }

    fn finish_request<T>(
        &self,
        mode: TaskExecutionMode,
        guard: RequestGuard,
        assessment: OperationAssessment,
        execute: impl FnOnce() -> CommandResult<T>,
    ) -> CommandResult<TaskCommandOutcome<T>> {
        match mode {
            TaskExecutionMode::Guarded => self.finish_mutation(
                assessment,
                guard.preview,
                guard.approval_id.as_deref(),
                execute,
            ),
            TaskExecutionMode::Direct => {
                let result = self.finish_direct_mutation(assessment, execute)?;
                Ok(TaskCommandOutcome::Executed { result })
            }
        }
    }

    pub fn into_direct_result<T>(outcome: TaskCommandOutcome<T>) -> CommandResult<T> {
        match outcome {
            TaskCommandOutcome::Executed { result } => Ok(result),
            TaskCommandOutcome::Blocked { assessment } => {
                Err(TaskCommandError::classify(assessment_summary(&assessment)))
            }
            TaskCommandOutcome::Preview { .. }
            | TaskCommandOutcome::ConfirmationRequired { .. } => Err(TaskCommandError::internal(
                "Direct task execution returned a guarded outcome.",
            )),
        }
    }

    async fn prepare_create(
        &self,
        request: TaskCreateRequest,
    ) -> CommandResult<(RequestGuard, OperationAssessment, TaskSpec)> {
        let request = self.normalize_create_request(request)?;
        self.validate_create_request(&request)?;
        let guard = RequestGuard::capture(request.preview, request.approval_id.clone());
        let assessment = self
            .assessor()?
            .assess_task_create(request.clone())
            .await
            .map_err(TaskCommandError::from_tool_error)?;
        let spec = create_request_to_spec(request).map_err(TaskCommandError::from_tool_error)?;
        Ok((guard, assessment, spec))
    }

    async fn prepare_update(
        &self,
        request: TaskUpdateRequest,
    ) -> CommandResult<(RequestGuard, OperationAssessment, String, TaskPatch)> {
        let request = self.normalize_update_request(request)?;
        self.validate_update_request(&request)?;
        let resolved_id = request.id.clone();
        let guard = RequestGuard::capture(request.preview, request.approval_id.clone());
        let assessment = self
            .assessor()?
            .assess_task_update(request.clone())
            .await
            .map_err(TaskCommandError::from_tool_error)?;
        let patch = update_request_to_patch(request).map_err(TaskCommandError::from_tool_error)?;
        Ok((guard, assessment, resolved_id, patch))
    }

    async fn prepare_delete(
        &self,
        request: TaskDeleteRequest,
    ) -> CommandResult<(RequestGuard, OperationAssessment, String)> {
        let request = self.normalize_delete_request(request)?;
        self.validate_delete_request(&request)?;
        let resolved_id = request.id.clone();
        let guard = RequestGuard::capture(request.preview, request.approval_id.clone());
        let assessment = self
            .assessor()?
            .assess_task_delete(request)
            .await
            .map_err(TaskCommandError::from_tool_error)?;
        Ok((guard, assessment, resolved_id))
    }

    async fn prepare_control(
        &self,
        request: TaskControlRequest,
    ) -> CommandResult<(RequestGuard, OperationAssessment, String, TaskControlAction)> {
        let (request, action) = self.normalize_control_request(request)?;
        self.validate_control_request(&request)?;
        let resolved_id = request.id.clone();
        let guard = RequestGuard::capture(request.preview, request.approval_id.clone());
        let assessment = self
            .assessor()?
            .assess_task_control(request.clone())
            .await
            .map_err(TaskCommandError::from_tool_error)?;
        Ok((guard, assessment, resolved_id, action))
    }

    async fn prepare_convert_session(
        &self,
        request: TaskConvertSessionRequest,
    ) -> CommandResult<(RequestGuard, OperationAssessment, PreparedSessionConversion)> {
        let request = self.normalize_convert_session_request(request);
        self.validate_convert_session_request(&request)?;
        let session_id = request.session_id.clone();
        let guard = RequestGuard::capture(request.preview, request.approval_id.clone());
        let assessment = self
            .assessor()?
            .assess_task_convert_session(request.clone())
            .await
            .map_err(TaskCommandError::from_tool_error)?;

        let session = self
            .session_service
            .get_session_view(&session_id)
            .map_err(TaskCommandError::from_anyhow)?
            .ok_or_else(|| {
                TaskCommandError::not_found(format!("Session not found: {}", session_id))
            })?;
        let options = convert_session_request_to_options(request)
            .map_err(TaskCommandError::from_tool_error)?;
        let spec = build_convert_session_spec(
            &session,
            ConvertSessionSpecOptions {
                name: options.name,
                description: None,
                schedule: Some(options.schedule),
                input: options.input,
                notification: None,
                execution_mode: None,
                timeout_secs: options.timeout_secs,
                memory: options.memory,
                durability_mode: options.durability_mode,
                resource_limits: options.resource_limits,
                prerequisites: Vec::new(),
                continuation: None,
            },
        )
        .map_err(|error| TaskCommandError::internal(error.to_string()))?;

        Ok((
            guard,
            assessment,
            PreparedSessionConversion {
                spec,
                source_session_id: session.id,
                source_session_agent_id: session.agent_id,
                run_now: options.run_now,
            },
        ))
    }

    fn execute_convert_session(
        &self,
        prepared: PreparedSessionConversion,
    ) -> CommandResult<TaskConversionResult> {
        let mut task = self
            .storage
            .create_background_agent(prepared.spec)
            .map_err(TaskCommandError::from_anyhow)?;
        if prepared.run_now {
            task = self
                .storage
                .control_background_agent(&task.id, TaskControlAction::RunNow)
                .map_err(TaskCommandError::from_anyhow)?;
        }
        Ok(TaskConversionResult {
            task,
            source_session_id: prepared.source_session_id,
            source_session_agent_id: prepared.source_session_agent_id,
            run_now: prepared.run_now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{TaskCommandService, TaskExecutionMode};
    use crate::models::{AgentNode, BackgroundAgentSpec, ChatMessage, ChatSession, ModelId};
    use crate::prompt_files;
    use crate::services::session::SessionService;
    use crate::storage::{
        AgentStorage, BackgroundAgentStorage, ChannelSessionBindingStorage, ChatSessionStorage,
        ExecutionTraceStorage, MemoryStorage, SessionStorage,
    };
    use async_trait::async_trait;
    use restflow_traits::ContractSubagentSpawnRequest;
    use restflow_traits::TaskCommandOutcome;
    use restflow_traits::ToolError;
    use restflow_traits::assessment::{
        AgentOperationAssessor, OperationAssessment, OperationAssessmentIntent,
    };
    use restflow_traits::store::{
        AgentCreateRequest, AgentUpdateRequest, BackgroundAgentControlRequest,
        BackgroundAgentConvertSessionRequest, BackgroundAgentCreateRequest,
        BackgroundAgentDeleteRequest, BackgroundAgentUpdateRequest, TaskControlRequest,
        TaskConvertSessionRequest, TaskCreateRequest, TaskDeleteRequest, TaskUpdateRequest,
    };
    use std::sync::Arc;
    use tempfile::tempdir;

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

        async fn assess_background_agent_create(
            &self,
            _request: BackgroundAgentCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "create_background_agent",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_background_agent_convert_session(
            &self,
            _request: BackgroundAgentConvertSessionRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "convert_session_to_background_agent",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_background_agent_update(
            &self,
            _request: BackgroundAgentUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "update_background_agent",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_background_agent_delete(
            &self,
            request: BackgroundAgentDeleteRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "delete_background_agent",
                OperationAssessmentIntent::Save,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "destructive_delete".to_string(),
                    message: format!("delete guard for {}", request.id),
                    field: Some("id".to_string()),
                    suggestion: Some("Confirm delete".to_string()),
                }],
            ))
        }

        async fn assess_background_agent_control(
            &self,
            _request: BackgroundAgentControlRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "control_background_agent",
                OperationAssessmentIntent::Run,
            ))
        }

        async fn assess_background_agent_template(
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
            _request: ContractSubagentSpawnRequest,
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
            _requests: Vec<ContractSubagentSpawnRequest>,
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
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_agent_update(
            &self,
            _request: AgentUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "update_agent",
                OperationAssessmentIntent::Save,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_background_agent_create(
            &self,
            _request: BackgroundAgentCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "create_background_agent",
                OperationAssessmentIntent::Save,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_background_agent_convert_session(
            &self,
            _request: BackgroundAgentConvertSessionRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "convert_session_to_background_agent",
                OperationAssessmentIntent::Save,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_background_agent_update(
            &self,
            _request: BackgroundAgentUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "update_background_agent",
                OperationAssessmentIntent::Save,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_background_agent_delete(
            &self,
            request: BackgroundAgentDeleteRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "delete_background_agent",
                OperationAssessmentIntent::Save,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "destructive_delete".to_string(),
                    message: format!("delete guard for {}", request.id),
                    field: Some("id".to_string()),
                    suggestion: Some("Confirm delete".to_string()),
                }],
            ))
        }

        async fn assess_background_agent_control(
            &self,
            _request: BackgroundAgentControlRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "control_background_agent",
                OperationAssessmentIntent::Run,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_background_agent_template(
            &self,
            operation: &str,
            intent: OperationAssessmentIntent,
            _agent_ids: Vec<String>,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                operation,
                intent,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_subagent_spawn(
            &self,
            operation: &str,
            _request: ContractSubagentSpawnRequest,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                operation,
                OperationAssessmentIntent::Run,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_subagent_batch(
            &self,
            operation: &str,
            _requests: Vec<ContractSubagentSpawnRequest>,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                operation,
                OperationAssessmentIntent::Run,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }
    }

    struct CanonicalTaskAssessor;

    #[async_trait]
    impl AgentOperationAssessor for CanonicalTaskAssessor {
        async fn assess_agent_create(
            &self,
            _request: AgentCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("agent create should not be called")
        }

        async fn assess_agent_update(
            &self,
            _request: AgentUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("agent update should not be called")
        }

        async fn assess_task_create(
            &self,
            _request: TaskCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "task_create",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_task_convert_session(
            &self,
            _request: TaskConvertSessionRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "task_convert_session",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_task_update(
            &self,
            _request: TaskUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "task_update",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_task_delete(
            &self,
            _request: TaskDeleteRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "task_delete",
                OperationAssessmentIntent::Save,
                vec![restflow_traits::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_task_control(
            &self,
            _request: TaskControlRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::ok(
                "task_control",
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

        async fn assess_background_agent_create(
            &self,
            _request: BackgroundAgentCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("background create should not be called")
        }

        async fn assess_background_agent_convert_session(
            &self,
            _request: BackgroundAgentConvertSessionRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("background convert should not be called")
        }

        async fn assess_background_agent_update(
            &self,
            _request: BackgroundAgentUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("background update should not be called")
        }

        async fn assess_background_agent_delete(
            &self,
            _request: BackgroundAgentDeleteRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("background delete should not be called")
        }

        async fn assess_background_agent_control(
            &self,
            _request: BackgroundAgentControlRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("background control should not be called")
        }

        async fn assess_background_agent_template(
            &self,
            _operation: &str,
            _intent: OperationAssessmentIntent,
            _agent_ids: Vec<String>,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("background template should not be called")
        }

        async fn assess_subagent_spawn(
            &self,
            _operation: &str,
            _request: ContractSubagentSpawnRequest,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("subagent spawn should not be called")
        }

        async fn assess_subagent_batch(
            &self,
            _operation: &str,
            _requests: Vec<ContractSubagentSpawnRequest>,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("subagent batch should not be called")
        }
    }

    fn setup() -> (TaskCommandService, ChatSession, tempfile::TempDir) {
        setup_with_assessor(Arc::new(MockAssessor))
    }

    fn setup_with_assessor(
        assessor: Arc<dyn AgentOperationAssessor>,
    ) -> (TaskCommandService, ChatSession, tempfile::TempDir) {
        let _guard = prompt_files::agents_dir_env_lock();
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("background-command.db");
        let db = Arc::new(redb::Database::create(&db_path).expect("create db"));

        let background_storage = BackgroundAgentStorage::new(db.clone()).expect("bg storage");
        let agent_storage = AgentStorage::new(db.clone()).expect("agent storage");
        let chat_storage = ChatSessionStorage::new(db.clone()).expect("chat storage");
        let binding_storage =
            ChannelSessionBindingStorage::new(db.clone()).expect("binding storage");
        let trace_storage = ExecutionTraceStorage::new(db.clone()).expect("trace storage");
        let memory_storage = MemoryStorage::new(db).expect("memory storage");
        let session_storage =
            SessionStorage::new(chat_storage.clone(), binding_storage, trace_storage);
        let session_service = SessionService::new(
            session_storage,
            Some(agent_storage.clone()),
            background_storage.clone(),
            Some(memory_storage),
        );

        let prompts_dir = temp_dir.path().join("state").join("agents");
        std::fs::create_dir_all(&prompts_dir).expect("prompts dir");
        let prev_agents_dir = std::env::var_os(prompt_files::AGENTS_DIR_ENV);
        unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, &prompts_dir) };
        agent_storage
            .create_agent("svc-agent".to_string(), AgentNode::default())
            .expect("create agent");
        unsafe {
            match prev_agents_dir {
                Some(value) => std::env::set_var(prompt_files::AGENTS_DIR_ENV, value),
                None => std::env::remove_var(prompt_files::AGENTS_DIR_ENV),
            }
        }

        let agent_id = agent_storage
            .list_agents()
            .expect("list agents")
            .into_iter()
            .next()
            .expect("agent present")
            .id;
        let mut session = ChatSession::new(agent_id, ModelId::Gpt5.as_serialized_str().to_string())
            .with_name("Convert Me");
        session.add_message(ChatMessage::user("continue this task"));
        chat_storage.create(&session).expect("create session");

        (
            TaskCommandService::new(
                background_storage,
                agent_storage,
                session_service,
                Some(assessor),
            ),
            session,
            temp_dir,
        )
    }

    #[test]
    fn task_command_aliases_legacy_background_agent_names() {
        let (service, _session, _temp_dir) = setup();
        let _: &TaskCommandService = &service;
        let _: &TaskCommandService = &service;
        let _: TaskExecutionMode = TaskExecutionMode::Guarded;
        let _: TaskExecutionMode = TaskExecutionMode::Direct;
    }

    #[tokio::test]
    async fn convert_session_returns_conversion_result() {
        let (service, session, _dir) = setup();
        let result = service
            .convert_session(
                BackgroundAgentConvertSessionRequest {
                    session_id: session.id.clone(),
                    name: Some("Converted Session".to_string()),
                    schedule: None,
                    input: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    run_now: Some(false),
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("convert session");

        match result {
            TaskCommandOutcome::Executed { result } => {
                assert_eq!(result.source_session_id, session.id);
                assert_eq!(result.source_session_agent_id, session.agent_id);
                assert_eq!(result.task.chat_session_id, result.source_session_id);
                assert_eq!(result.task.name, "Converted Session");
                assert!(!result.run_now);
            }
            other => panic!("expected executed outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn convert_session_preview_does_not_create_task() {
        let (service, session, _dir) = setup();
        let result = service
            .convert_session(
                BackgroundAgentConvertSessionRequest {
                    session_id: session.id,
                    name: Some("Preview Convert".to_string()),
                    schedule: None,
                    input: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    run_now: None,
                    preview: true,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("preview convert");

        match result {
            TaskCommandOutcome::Preview { assessment } => {
                assert_eq!(assessment.operation, "convert_session_to_background_agent");
            }
            other => panic!("expected preview outcome, got {other:?}"),
        }
        assert!(service.storage.list_tasks().expect("list tasks").is_empty());
    }

    #[tokio::test]
    async fn create_rejects_blank_name_before_assessment() {
        let (service, _session, _dir) = setup();
        let err = service
            .create_from_request(
                BackgroundAgentCreateRequest {
                    name: "   ".to_string(),
                    agent_id: "default".to_string(),
                    chat_session_id: None,
                    schedule: restflow_contracts::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect_err("blank name should fail");
        assert!(err.to_string().contains("name must not be empty"));
    }

    #[tokio::test]
    async fn create_requires_confirmation_when_warning_assessment_requires_it() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));

        let result = service
            .create_from_request(
                BackgroundAgentCreateRequest {
                    name: "Create Guarded Warning".to_string(),
                    agent_id: session.agent_id,
                    chat_session_id: None,
                    schedule: restflow_contracts::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("create should return confirmation_required");

        match result {
            TaskCommandOutcome::ConfirmationRequired { assessment } => {
                assert_eq!(assessment.operation, "create_background_agent");
                assert!(assessment.requires_confirmation);
            }
            other => panic!("expected confirmation_required outcome, got {other:?}"),
        }

        assert!(service.storage.list_tasks().expect("list tasks").is_empty());
    }

    #[tokio::test]
    async fn update_requires_confirmation_when_warning_assessment_requires_it() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Update Guarded Warning".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("update guarded".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .update_from_request(
                BackgroundAgentUpdateRequest {
                    id: task.id.clone(),
                    name: Some("Should Not Persist".to_string()),
                    description: None,
                    agent_id: None,
                    chat_session_id: None,
                    input: None,
                    input_template: None,
                    schedule: None,
                    notification: None,
                    execution_mode: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("update should return confirmation_required");

        match result {
            TaskCommandOutcome::ConfirmationRequired { assessment } => {
                assert_eq!(assessment.operation, "update_background_agent");
                assert!(assessment.requires_confirmation);
            }
            other => panic!("expected confirmation_required outcome, got {other:?}"),
        }

        let stored = service
            .storage
            .get_task(&task.id)
            .expect("load task")
            .expect("task should still exist");
        assert_eq!(stored.name, "Update Guarded Warning");
    }

    #[tokio::test]
    async fn control_requires_confirmation_when_warning_assessment_requires_it() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Control Guarded Warning".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("control guarded".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .control_from_request(
                BackgroundAgentControlRequest {
                    id: task.id.clone(),
                    action: "pause".to_string(),
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("control should return confirmation_required");

        match result {
            TaskCommandOutcome::ConfirmationRequired { assessment } => {
                assert_eq!(assessment.operation, "control_background_agent");
                assert!(assessment.requires_confirmation);
            }
            other => panic!("expected confirmation_required outcome, got {other:?}"),
        }

        let stored = service
            .storage
            .get_task(&task.id)
            .expect("load task")
            .expect("task should still exist");
        assert_eq!(stored.status, crate::models::BackgroundAgentStatus::Active);
    }

    #[tokio::test]
    async fn convert_session_requires_confirmation_when_warning_assessment_requires_it() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));

        let result = service
            .convert_session(
                BackgroundAgentConvertSessionRequest {
                    session_id: session.id.clone(),
                    name: Some("Convert Guarded Warning".to_string()),
                    schedule: None,
                    input: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    run_now: Some(false),
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("convert should return confirmation_required");

        match result {
            TaskCommandOutcome::ConfirmationRequired { assessment } => {
                assert_eq!(assessment.operation, "convert_session_to_background_agent");
                assert!(assessment.requires_confirmation);
            }
            other => panic!("expected confirmation_required outcome, got {other:?}"),
        }

        assert!(service.storage.list_tasks().expect("list tasks").is_empty());
    }

    #[tokio::test]
    async fn delete_preview_returns_confirmation_assessment_without_removing_task() {
        let (service, session, _dir) = setup();
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Delete Preview".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("delete preview".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .delete_from_request(
                BackgroundAgentDeleteRequest {
                    id: task.id.clone(),
                    preview: true,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("delete preview");

        match result {
            TaskCommandOutcome::Preview { assessment } => {
                assert_eq!(assessment.operation, "delete_background_agent");
                assert!(assessment.requires_confirmation);
                assert!(assessment.approval_id.is_some());
                assert_eq!(
                    assessment.warnings[0].message,
                    format!("delete guard for {}", task.id)
                );
            }
            other => panic!("expected preview outcome, got {other:?}"),
        }

        assert!(
            service
                .storage
                .get_task(&task.id)
                .expect("load task")
                .is_some()
        );
    }

    #[tokio::test]
    async fn delete_requires_confirmation_before_execution() {
        let (service, session, _dir) = setup();
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Delete Requires Confirmation".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("delete requires confirmation".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .delete_from_request(
                BackgroundAgentDeleteRequest {
                    id: task.id.clone(),
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("delete should return confirmation_required");

        match result {
            TaskCommandOutcome::ConfirmationRequired { assessment } => {
                assert_eq!(assessment.operation, "delete_background_agent");
                assert!(assessment.requires_confirmation);
                assert_eq!(
                    assessment.warnings[0].message,
                    format!("delete guard for {}", task.id)
                );
            }
            other => panic!("expected confirmation_required outcome, got {other:?}"),
        }

        assert!(
            service
                .storage
                .get_task(&task.id)
                .expect("load task")
                .is_some()
        );
    }

    #[tokio::test]
    async fn delete_executes_when_approval_id_matches() {
        let (service, session, _dir) = setup();
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Delete Confirmed".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("delete confirmed".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let preview = service
            .delete_from_request(
                BackgroundAgentDeleteRequest {
                    id: task.id.clone(),
                    preview: true,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("delete preview");

        let token = match preview {
            TaskCommandOutcome::Preview { assessment } => assessment
                .approval_id
                .expect("delete preview should carry confirmation token"),
            other => panic!("expected preview outcome, got {other:?}"),
        };

        let result = service
            .delete_from_request(
                BackgroundAgentDeleteRequest {
                    id: task.id.clone(),
                    preview: false,
                    approval_id: Some(token),
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("delete confirmed");

        match result {
            TaskCommandOutcome::Executed { result } => {
                assert_eq!(result.id, task.id);
                assert!(result.deleted);
            }
            other => panic!("expected executed outcome, got {other:?}"),
        }

        assert!(
            service
                .storage
                .get_task(&task.id)
                .expect("load task")
                .is_none()
        );
    }

    #[tokio::test]
    async fn delete_direct_executes_without_approval_id() {
        let (service, session, _dir) = setup();
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Delete Direct".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("delete direct".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .delete_from_request(
                BackgroundAgentDeleteRequest {
                    id: task.id.clone(),
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result)
            .expect("delete direct");

        assert_eq!(result.id, task.id);
        assert!(result.deleted);
        assert!(
            service
                .storage
                .get_task(&task.id)
                .expect("load task")
                .is_none()
        );
    }

    #[tokio::test]
    async fn create_direct_executes_with_warning_assessment() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));

        let result = service
            .create_from_request(
                BackgroundAgentCreateRequest {
                    name: "Create Direct Warning".to_string(),
                    agent_id: session.agent_id,
                    chat_session_id: None,
                    schedule: restflow_contracts::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result)
            .expect("create direct");

        assert_eq!(result.name, "Create Direct Warning");
    }

    #[tokio::test]
    async fn update_direct_executes_with_warning_assessment() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Update Direct Warning".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("update direct".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .update_from_request(
                BackgroundAgentUpdateRequest {
                    id: task.id.clone(),
                    name: Some("Updated Name".to_string()),
                    description: None,
                    agent_id: None,
                    chat_session_id: None,
                    input: None,
                    input_template: None,
                    schedule: None,
                    notification: None,
                    execution_mode: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result)
            .expect("update direct");

        assert_eq!(result.id, task.id);
        assert_eq!(result.name, "Updated Name");
    }

    #[tokio::test]
    async fn control_direct_executes_with_warning_assessment() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Control Direct Warning".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("control direct".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .control_from_request(
                BackgroundAgentControlRequest {
                    id: task.id.clone(),
                    action: "pause".to_string(),
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result)
            .expect("control direct");

        assert_eq!(result.id, task.id);
        assert_eq!(result.status, crate::models::BackgroundAgentStatus::Paused);
    }

    #[tokio::test]
    async fn convert_session_direct_executes_with_warning_assessment() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));

        let result = service
            .convert_session(
                BackgroundAgentConvertSessionRequest {
                    session_id: session.id.clone(),
                    name: Some("Converted Direct Warning".to_string()),
                    schedule: None,
                    input: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    run_now: Some(false),
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result)
            .expect("convert direct");

        assert_eq!(result.source_session_id, session.id);
        assert_eq!(result.task.name, "Converted Direct Warning");
        assert!(!result.run_now);
    }

    #[tokio::test]
    async fn task_assessment_methods_are_used_by_command_service() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(CanonicalTaskAssessor));
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Canonical Task".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("canonical".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let create = service
            .create_from_request(
                TaskCreateRequest {
                    name: "Create Canonical Task".to_string(),
                    agent_id: session.agent_id.clone(),
                    chat_session_id: None,
                    schedule: restflow_contracts::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    preview: true,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("create preview");
        match create {
            TaskCommandOutcome::Preview { assessment } => {
                assert_eq!(assessment.operation, "task_create");
            }
            other => panic!("expected preview outcome, got {other:?}"),
        }

        let update = service
            .update_from_request(
                TaskUpdateRequest {
                    id: task.id.clone(),
                    name: Some("Update Canonical Task".to_string()),
                    description: None,
                    agent_id: None,
                    chat_session_id: None,
                    input: None,
                    input_template: None,
                    schedule: None,
                    notification: None,
                    execution_mode: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    preview: true,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("update preview");
        match update {
            TaskCommandOutcome::Preview { assessment } => {
                assert_eq!(assessment.operation, "task_update");
            }
            other => panic!("expected preview outcome, got {other:?}"),
        }

        let control = service
            .control_from_request(
                TaskControlRequest {
                    id: task.id.clone(),
                    action: "pause".to_string(),
                    preview: true,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("control preview");
        match control {
            TaskCommandOutcome::Preview { assessment } => {
                assert_eq!(assessment.operation, "task_control");
            }
            other => panic!("expected preview outcome, got {other:?}"),
        }

        let convert = service
            .convert_session(
                TaskConvertSessionRequest {
                    session_id: session.id.clone(),
                    name: Some("Canonical Convert".to_string()),
                    schedule: None,
                    input: None,
                    timeout_secs: None,
                    durability_mode: None,
                    memory: None,
                    memory_scope: None,
                    resource_limits: None,
                    run_now: Some(false),
                    preview: true,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("convert preview");
        match convert {
            TaskCommandOutcome::Preview { assessment } => {
                assert_eq!(assessment.operation, "task_convert_session");
            }
            other => panic!("expected preview outcome, got {other:?}"),
        }

        let delete = service
            .delete_from_request(
                TaskDeleteRequest {
                    id: task.id,
                    preview: true,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("delete preview");
        match delete {
            TaskCommandOutcome::Preview { assessment } => {
                assert_eq!(assessment.operation, "task_delete");
            }
            other => panic!("expected preview outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn delete_requires_assessor_availability() {
        let (service_with_assessor, session, _dir) = setup();
        let service = TaskCommandService::new(
            service_with_assessor.storage.clone(),
            service_with_assessor.agents.clone(),
            service_with_assessor.session_service.clone(),
            None,
        );
        let task = service
            .storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Delete Without Assessor".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("delete requires assessor".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let err = service
            .delete_from_request(
                BackgroundAgentDeleteRequest {
                    id: task.id,
                    preview: true,
                    approval_id: None,
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect_err("delete should fail closed without assessor");

        assert!(
            err.to_string()
                .contains("Task capability assessment is unavailable")
        );
    }
}
