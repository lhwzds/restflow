use crate::boundary::task::{
    convert_session_request_to_options, create_request_to_spec, parse_control_action,
    update_request_to_patch,
};
use crate::daemon::request_mapper::to_contract;
use crate::models::{
    ChatSession, ChatSessionSource, ChatTurnEvent, ChatTurnEventKind, ModelId, Task,
    TaskControlAction, TaskConversionResult, TaskMessage, TaskMessageSource, TaskPatch,
    TaskProgress, TaskSpec, TaskTranscriptPreview,
};
use crate::services::operation_assessment::{
    assessment_requires_confirmation, assessment_summary, ensure_assessment_confirmed,
};
use crate::services::session::SessionService;
use crate::services::task_conversion::{ConvertSessionSpecOptions, build_convert_session_spec};
use crate::storage::task_runtime::TaskSessionBinding;
use crate::storage::{AgentStorage, Storage, TaskStorage};
use std::sync::Arc;
use tools::ToolError;
use types::store::{
    TaskControlRequest, TaskConvertSessionRequest, TaskCreateRequest, TaskDeleteRequest,
    TaskUpdateRequest,
};
use types::{AgentOperationAssessor, OperationAssessment, TaskCommandOutcome};
use types::{DeleteWithIdResponse, ErrorKind, ErrorPayload};

type CommandResult<T> = std::result::Result<T, TaskCommandError>;
const TASK_PROGRESS_MESSAGE_LIMIT: usize = 6;
const TASK_PROGRESS_TURN_EVENT_LIMIT: usize = 20;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskCommandError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

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
    storage: TaskStorage,
    agents: AgentStorage,
    session_service: SessionService,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
}

impl TaskCommandService {
    pub fn new(
        storage: TaskStorage,
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
            storage.tasks.clone(),
            storage.agents.clone(),
            SessionService::from_storage(storage),
            assessor,
        )
    }

    pub fn with_assessor(mut self, assessor: Arc<dyn AgentOperationAssessor>) -> Self {
        self.assessor = Some(assessor);
        self
    }

    fn normalize_optional_id(value: Option<String>) -> Option<String> {
        value.and_then(|id| {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    fn resolve_agent_model_for_session(&self, agent_id: &str) -> anyhow::Result<String> {
        let fallback_model = ModelId::Gpt5_4.as_serialized_str().to_string();
        let Some(agent) = self.agents.get_agent(agent_id.to_string())? else {
            return Ok(fallback_model);
        };

        Ok(agent
            .agent
            .resolved_model_ref()
            .map(|model_ref| model_ref.model.as_serialized_str().to_string())
            .unwrap_or(fallback_model))
    }

    fn create_bound_session(&self, agent_id: &str, task_name: &str) -> anyhow::Result<String> {
        let model = self.resolve_agent_model_for_session(agent_id)?;
        let session_name = format!("Background: {}", task_name);
        let session = ChatSession::new(agent_id.to_string(), model)
            .with_name(session_name)
            .with_source(ChatSessionSource::Background, task_name.to_string());
        let session = self.session_service.create_external_session(session)?;
        Ok(session.id)
    }

    fn ensure_session_binding(&self, chat_session_id: &str, agent_id: &str) -> anyhow::Result<()> {
        let session = self
            .session_service
            .get_session_view(chat_session_id)?
            .ok_or_else(|| anyhow::anyhow!("chat_session_id '{}' not found", chat_session_id))?;

        if session.agent_id != agent_id {
            anyhow::bail!(
                "chat_session_id '{}' is bound to agent '{}', expected '{}'",
                chat_session_id,
                session.agent_id,
                agent_id
            );
        }

        Ok(())
    }

    fn ensure_unique_session_binding(
        &self,
        chat_session_id: &str,
        current_task_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let target = chat_session_id.trim();
        if target.is_empty() {
            return Ok(());
        }

        if let Some(conflict) = self.storage.list_tasks()?.into_iter().find(|task| {
            let same_session = task.chat_session_id.trim() == target;
            let same_task = current_task_id.is_some_and(|task_id| task.id == task_id);
            same_session && !same_task
        }) {
            anyhow::bail!(
                "chat_session_id '{}' is already bound to task '{}' ({})",
                target,
                conflict.id,
                conflict.name
            );
        }

        Ok(())
    }

    fn resolve_task_session_for_create(
        &self,
        requested_chat_session_id: Option<String>,
        agent_id: &str,
        task_name: &str,
    ) -> anyhow::Result<TaskSessionBinding> {
        if let Some(chat_session_id) = Self::normalize_optional_id(requested_chat_session_id) {
            self.ensure_session_binding(&chat_session_id, agent_id)?;
            self.ensure_unique_session_binding(&chat_session_id, None)?;
            return Ok(TaskSessionBinding {
                session_id: chat_session_id,
                owns_session: false,
            });
        }

        Ok(TaskSessionBinding {
            session_id: self.create_bound_session(agent_id, task_name)?,
            owns_session: true,
        })
    }

    fn resolve_task_session_for_update(
        &self,
        task: &Task,
        requested_chat_session_id: Option<String>,
        next_agent_id: &str,
    ) -> anyhow::Result<TaskSessionBinding> {
        if let Some(chat_session_id) = Self::normalize_optional_id(requested_chat_session_id) {
            self.ensure_session_binding(&chat_session_id, next_agent_id)?;
            self.ensure_unique_session_binding(&chat_session_id, Some(&task.id))?;
            return Ok(TaskSessionBinding {
                session_id: chat_session_id,
                owns_session: false,
            });
        }

        let current_chat_session_id = task.chat_session_id.trim();
        if current_chat_session_id.is_empty() {
            anyhow::bail!("task '{}' is not bound to a chat session", task.id);
        }
        self.ensure_session_binding(current_chat_session_id, next_agent_id)?;
        self.ensure_unique_session_binding(current_chat_session_id, Some(&task.id))?;
        Ok(TaskSessionBinding {
            session_id: current_chat_session_id.to_string(),
            owns_session: task.owns_chat_session,
        })
    }

    fn archive_owned_task_session_if_unused(&self, task: &Task) -> anyhow::Result<()> {
        let session_id = task.chat_session_id.trim();
        if session_id.is_empty() || !task.owns_chat_session {
            return Ok(());
        }

        let session_reused = self
            .storage
            .list_tasks()?
            .into_iter()
            .any(|other| other.id != task.id && other.chat_session_id.trim() == session_id);
        if !session_reused {
            let _ = self.session_service.archive_managed_session(session_id)?;
        }
        Ok(())
    }

    fn archive_created_task_session(&self, session_binding: &TaskSessionBinding) {
        if session_binding.owns_session && !session_binding.session_id.trim().is_empty() {
            let _ = self
                .session_service
                .archive_managed_session(&session_binding.session_id);
        }
    }

    fn create(&self, spec: TaskSpec) -> CommandResult<Task> {
        TaskStorage::validate_create_spec(&spec).map_err(TaskCommandError::from_anyhow)?;
        let session_binding = self
            .resolve_task_session_for_create(
                spec.chat_session_id.clone(),
                &spec.agent_id,
                &spec.name,
            )
            .map_err(TaskCommandError::from_anyhow)?;
        match self
            .storage
            .create_task_from_spec_with_binding(spec, session_binding.clone())
        {
            Ok(task) => Ok(task),
            Err(error) => {
                self.archive_created_task_session(&session_binding);
                Err(TaskCommandError::from_anyhow(error))
            }
        }
    }

    pub fn create_from_spec_direct(&self, spec: TaskSpec) -> CommandResult<Task> {
        self.create(spec)
    }

    #[cfg(test)]
    pub(crate) fn create_session_for_test(
        &self,
        session: ChatSession,
    ) -> anyhow::Result<ChatSession> {
        self.session_service.create_external_session(session)
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
        let task = self
            .storage
            .get_task(id)
            .map_err(TaskCommandError::from_anyhow)?
            .ok_or_else(|| TaskCommandError::not_found(format!("Task {} not found", id)))?;
        let next_agent_id = patch
            .agent_id
            .clone()
            .unwrap_or_else(|| task.agent_id.clone());
        TaskStorage::validate_update_patch_for_task(&task, &patch)
            .map_err(TaskCommandError::from_anyhow)?;
        let session_binding = self
            .resolve_task_session_for_update(&task, patch.chat_session_id.clone(), &next_agent_id)
            .map_err(TaskCommandError::from_anyhow)?;
        match self
            .storage
            .update_task_from_patch_with_binding(id, patch, session_binding.clone())
        {
            Ok(updated) => {
                if updated.chat_session_id != task.chat_session_id
                    || updated.owns_chat_session != task.owns_chat_session
                {
                    self.archive_owned_task_session_if_unused(&task)
                        .map_err(TaskCommandError::from_anyhow)?;
                }
                Ok(updated)
            }
            Err(error) => {
                self.archive_created_task_session(&session_binding);
                Err(TaskCommandError::from_anyhow(error))
            }
        }
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
        let deleted = self
            .storage
            .delete_task_record(id)
            .map_err(TaskCommandError::from_anyhow)?;
        let Some(task) = deleted else {
            return Ok(false);
        };
        self.archive_owned_task_session_if_unused(&task)
            .map_err(TaskCommandError::from_anyhow)?;
        Ok(true)
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
            .control_task(id, action)
            .map_err(TaskCommandError::from_anyhow)
    }

    pub fn progress(&self, id: &str, event_limit: usize) -> CommandResult<TaskProgress> {
        let mut progress = self
            .storage
            .get_task_progress(id, event_limit)
            .map_err(TaskCommandError::from_anyhow)?;
        let transcript = self
            .progress_transcript_preview(id)
            .map_err(TaskCommandError::from_anyhow)?;
        if let Some(stage) = Self::progress_stage_from_transcript(&transcript) {
            progress.stage = Some(stage);
        }
        progress.transcript = Some(transcript);
        Ok(progress)
    }

    fn progress_transcript_preview(&self, id: &str) -> anyhow::Result<TaskTranscriptPreview> {
        let task = self
            .storage
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;
        let chat_session_id = task.chat_session_id.trim();
        if chat_session_id.is_empty() {
            anyhow::bail!("task '{}' is not bound to a chat session", task.id);
        }
        let session = self
            .session_service
            .get_session_view(chat_session_id)?
            .ok_or_else(|| anyhow::anyhow!("chat session '{}' not found", chat_session_id))?;
        Ok(Self::transcript_preview_from_session(&session))
    }

    fn transcript_preview_from_session(session: &ChatSession) -> TaskTranscriptPreview {
        let messages_start = session
            .messages
            .len()
            .saturating_sub(TASK_PROGRESS_MESSAGE_LIMIT);
        let messages = session.messages[messages_start..].to_vec();

        let all_turn_events: Vec<ChatTurnEvent> = session
            .turns
            .iter()
            .flat_map(|turn| turn.events.iter().cloned())
            .collect();
        let turn_events_start = all_turn_events
            .len()
            .saturating_sub(TASK_PROGRESS_TURN_EVENT_LIMIT);
        let turn_events = all_turn_events[turn_events_start..].to_vec();

        TaskTranscriptPreview {
            chat_session_id: session.id.clone(),
            messages,
            turn_events,
            truncated: session.messages.len() > TASK_PROGRESS_MESSAGE_LIMIT
                || all_turn_events.len() > TASK_PROGRESS_TURN_EVENT_LIMIT,
        }
    }

    fn progress_stage_from_transcript(transcript: &TaskTranscriptPreview) -> Option<String> {
        let latest = transcript.turn_events.last()?;
        match &latest.kind {
            ChatTurnEventKind::Progress { .. } => Some("running".to_string()),
            ChatTurnEventKind::Error { .. } => Some("failed".to_string()),
            ChatTurnEventKind::Canceled => Some("interrupted".to_string()),
            _ => None,
        }
    }

    pub fn send_message(
        &self,
        id: &str,
        message: String,
        source: TaskMessageSource,
    ) -> CommandResult<TaskMessage> {
        self.storage
            .send_task_message(id, message, source)
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
        self.agents
            .resolve_existing_agent_id(id_or_alias.trim())
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
        self.validate_create_request(&request)?;
        let request = self.normalize_create_request(request)?;
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
                execution_mode: None,
                timeout_secs: options.timeout_secs,
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
        let mut task = self.create(prepared.spec)?;
        if prepared.run_now {
            task = self
                .storage
                .control_task(&task.id, TaskControlAction::RunNow)
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
    use crate::models::{
        AgentNode, ChatMessage, ChatSession, ChatTurnEventKind, ModelId, TaskSpec,
    };
    use crate::prompt_files;
    use crate::services::session::SessionService;
    use crate::session_log::FileSessionStore;
    use crate::storage::{AgentStorage, ChatSessionStorage, SessionStorage, TaskStorage};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tempfile::tempdir;
    use types::ContractRunSpawnRequest;
    use types::TaskCommandOutcome;
    use types::ToolError;
    use types::assessment::{
        AgentOperationAssessor, OperationAssessment, OperationAssessmentIntent,
    };
    use types::store::{
        AgentCreateRequest, AgentUpdateRequest, TaskControlRequest, TaskConvertSessionRequest,
        TaskCreateRequest, TaskDeleteRequest, TaskUpdateRequest,
    };

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
            request: TaskDeleteRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "delete_task",
                OperationAssessmentIntent::Save,
                vec![types::OperationAssessmentIssue {
                    code: "destructive_delete".to_string(),
                    message: format!("delete guard for {}", request.id),
                    field: Some("id".to_string()),
                    suggestion: Some("Confirm delete".to_string()),
                }],
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
                vec![types::OperationAssessmentIssue {
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
                vec![types::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_task_create(
            &self,
            _request: TaskCreateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "create_task",
                OperationAssessmentIntent::Save,
                vec![types::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_task_convert_session(
            &self,
            _request: TaskConvertSessionRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "convert_session_to_task",
                OperationAssessmentIntent::Save,
                vec![types::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_task_update(
            &self,
            _request: TaskUpdateRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "update_task",
                OperationAssessmentIntent::Save,
                vec![types::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
            ))
        }

        async fn assess_task_delete(
            &self,
            request: TaskDeleteRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "delete_task",
                OperationAssessmentIntent::Save,
                vec![types::OperationAssessmentIssue {
                    code: "destructive_delete".to_string(),
                    message: format!("delete guard for {}", request.id),
                    field: Some("id".to_string()),
                    suggestion: Some("Confirm delete".to_string()),
                }],
            ))
        }

        async fn assess_task_control(
            &self,
            _request: TaskControlRequest,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                "control_task",
                OperationAssessmentIntent::Run,
                vec![types::OperationAssessmentIssue {
                    code: "warn".to_string(),
                    message: "warning".to_string(),
                    field: None,
                    suggestion: None,
                }],
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
                vec![types::OperationAssessmentIssue {
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
            _request: ContractRunSpawnRequest,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                operation,
                OperationAssessmentIntent::Run,
                vec![types::OperationAssessmentIssue {
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
            _requests: Vec<ContractRunSpawnRequest>,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            Ok(OperationAssessment::warning_with_confirmation(
                operation,
                OperationAssessmentIntent::Run,
                vec![types::OperationAssessmentIssue {
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
                vec![types::OperationAssessmentIssue {
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

        async fn assess_subagent_spawn(
            &self,
            _operation: &str,
            _request: ContractRunSpawnRequest,
            _template_mode: bool,
        ) -> std::result::Result<OperationAssessment, ToolError> {
            panic!("subagent spawn should not be called")
        }

        async fn assess_subagent_batch(
            &self,
            _operation: &str,
            _requests: Vec<ContractRunSpawnRequest>,
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

        let task_storage = TaskStorage::new(db.clone()).expect("task storage");
        let agent_storage = AgentStorage::new(db.clone()).expect("agent storage");
        let chat_storage = ChatSessionStorage::new(db.clone()).expect("chat storage");
        let session_storage = SessionStorage::new(chat_storage.clone());
        let session_service = SessionService::new(
            session_storage,
            Some(agent_storage.clone()),
            task_storage.clone(),
        );

        let prev_agents_dir = std::env::var_os(prompt_files::AGENTS_DIR_ENV);
        unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, temp_dir.path().join("agents")) };
        agent_storage
            .create_agent("svc-agent".to_string(), AgentNode::default())
            .expect("create agent");

        let agent_id = agent_storage
            .list_agents()
            .expect("list agents")
            .into_iter()
            .next()
            .expect("agent present")
            .id;
        unsafe {
            match prev_agents_dir {
                Some(value) => std::env::set_var(prompt_files::AGENTS_DIR_ENV, value),
                None => std::env::remove_var(prompt_files::AGENTS_DIR_ENV),
            }
        }
        let mut session = ChatSession::new(agent_id, ModelId::Gpt5.as_serialized_str().to_string())
            .with_name("Convert Me");
        session.add_message(ChatMessage::user("continue this task"));
        chat_storage.create(&session).expect("create session");

        (
            TaskCommandService::new(task_storage, agent_storage, session_service, Some(assessor)),
            session,
            temp_dir,
        )
    }

    fn create_agent_for_test(
        service: &TaskCommandService,
        temp_dir: &tempfile::TempDir,
        name: &str,
    ) -> String {
        let prev_agents_dir = std::env::var_os(prompt_files::AGENTS_DIR_ENV);
        unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, temp_dir.path().join("agents")) };
        let agent = service
            .agents
            .create_agent(name.to_string(), AgentNode::default())
            .expect("create agent");
        unsafe {
            match prev_agents_dir {
                Some(value) => std::env::set_var(prompt_files::AGENTS_DIR_ENV, value),
                None => std::env::remove_var(prompt_files::AGENTS_DIR_ENV),
            }
        }
        agent.id
    }

    #[test]
    fn progress_includes_bound_session_transcript_preview() {
        let (service, mut session, _dir) = setup();
        session.add_message(ChatMessage::assistant("working on it"));
        session.record_turn_user_message("turn-1", "continue this task");
        session.record_turn_event(
            "turn-1",
            ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: "git diff --stat".to_string(),
            },
        );
        service
            .session_service
            .save_existing_session(&session, "test")
            .expect("save session");
        let task = service
            .create_from_spec_direct(TaskSpec {
                name: "Progress Transcript".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: Some(session.id.clone()),
                description: None,
                input: Some("review".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let progress = service.progress(&task.id, 5).expect("progress");
        let transcript = progress.transcript.expect("transcript preview");

        assert_eq!(transcript.chat_session_id, session.id);
        assert_eq!(transcript.messages.len(), 2);
        assert_eq!(transcript.messages[0].content, "continue this task");
        assert_eq!(transcript.messages[1].content, "working on it");
        assert_eq!(transcript.turn_events.len(), 2);
        assert!(!transcript.truncated);
    }

    #[test]
    fn progress_errors_when_bound_session_is_missing() {
        let (service, session, _dir) = setup();
        let mut task = service
            .storage
            .create_task(
                "Missing Transcript".to_string(),
                session.agent_id.clone(),
                crate::models::TaskSchedule::default(),
            )
            .expect("create task");
        task.chat_session_id = "missing-session".to_string();
        service
            .storage
            .update_task(&task)
            .expect("update task with missing session");

        let err = service
            .progress(&task.id, 5)
            .expect_err("progress should fail");
        assert!(
            err.to_string()
                .contains("chat session 'missing-session' not found")
        );
    }

    #[test]
    fn task_command_types_are_canonical() {
        let (service, _session, _temp_dir) = setup();
        let _: &TaskCommandService = &service;
        let _: TaskExecutionMode = TaskExecutionMode::Guarded;
        let _: TaskExecutionMode = TaskExecutionMode::Direct;
    }

    #[tokio::test]
    async fn convert_session_returns_conversion_result() {
        let (service, session, _dir) = setup();
        let result = service
            .convert_session(
                TaskConvertSessionRequest {
                    session_id: session.id.clone(),
                    name: Some("Converted Session".to_string()),
                    schedule: None,
                    input: None,
                    timeout_secs: None,
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
                TaskConvertSessionRequest {
                    session_id: session.id,
                    name: Some("Preview Convert".to_string()),
                    schedule: None,
                    input: None,
                    timeout_secs: None,
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
                assert_eq!(assessment.operation, "convert_session_to_task");
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
                TaskCreateRequest {
                    name: "   ".to_string(),
                    agent_id: "agent-ignored".to_string(),
                    chat_session_id: None,
                    schedule: types::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
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
                TaskCreateRequest {
                    name: "Create Guarded Warning".to_string(),
                    agent_id: session.agent_id,
                    chat_session_id: None,
                    schedule: types::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
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
                assert_eq!(assessment.operation, "create_task");
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
            .create_from_spec_direct(TaskSpec {
                name: "Update Guarded Warning".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("update guarded".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .update_from_request(
                TaskUpdateRequest {
                    id: task.id.clone(),
                    name: Some("Should Not Persist".to_string()),
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
                },
                TaskExecutionMode::Guarded,
            )
            .await
            .expect("update should return confirmation_required");

        match result {
            TaskCommandOutcome::ConfirmationRequired { assessment } => {
                assert_eq!(assessment.operation, "update_task");
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
            .create_from_spec_direct(TaskSpec {
                name: "Control Guarded Warning".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("control guarded".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .control_from_request(
                TaskControlRequest {
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
                assert_eq!(assessment.operation, "control_task");
                assert!(assessment.requires_confirmation);
            }
            other => panic!("expected confirmation_required outcome, got {other:?}"),
        }

        let stored = service
            .storage
            .get_task(&task.id)
            .expect("load task")
            .expect("task should still exist");
        assert_eq!(stored.status, crate::models::TaskStatus::Active);
    }

    #[tokio::test]
    async fn convert_session_requires_confirmation_when_warning_assessment_requires_it() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));

        let result = service
            .convert_session(
                TaskConvertSessionRequest {
                    session_id: session.id.clone(),
                    name: Some("Convert Guarded Warning".to_string()),
                    schedule: None,
                    input: None,
                    timeout_secs: None,
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
                assert_eq!(assessment.operation, "convert_session_to_task");
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
            .create_from_spec_direct(TaskSpec {
                name: "Delete Preview".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("delete preview".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .delete_from_request(
                TaskDeleteRequest {
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
                assert_eq!(assessment.operation, "delete_task");
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
            .create_from_spec_direct(TaskSpec {
                name: "Delete Requires Confirmation".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("delete requires confirmation".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .delete_from_request(
                TaskDeleteRequest {
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
                assert_eq!(assessment.operation, "delete_task");
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
            .create_from_spec_direct(TaskSpec {
                name: "Delete Confirmed".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("delete confirmed".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let preview = service
            .delete_from_request(
                TaskDeleteRequest {
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
                TaskDeleteRequest {
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
            .create_from_spec_direct(TaskSpec {
                name: "Delete Direct".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("delete direct".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .delete_from_request(
                TaskDeleteRequest {
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
        let archived_session = service
            .session_service
            .get_session_view(&task.chat_session_id)
            .expect("load session")
            .expect("session still present");
        assert!(archived_session.is_archived());
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
                TaskCreateRequest {
                    name: "Create Direct Warning".to_string(),
                    agent_id: session.agent_id,
                    chat_session_id: None,
                    schedule: types::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
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
    async fn create_direct_persists_background_session_through_file_store() {
        let (mut service, session, dir) = setup();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        service.session_service = service
            .session_service
            .clone()
            .with_file_sessions(file_store.clone());

        let result = service
            .create_from_request(
                TaskCreateRequest {
                    name: "Create File Session".to_string(),
                    agent_id: session.agent_id,
                    chat_session_id: None,
                    schedule: types::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result)
            .expect("create direct");

        assert!(file_store.get(&result.chat_session_id).unwrap().is_some());
    }

    #[tokio::test]
    async fn create_direct_invalid_spec_does_not_create_file_session() {
        let (mut service, session, dir) = setup();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        service.session_service = service
            .session_service
            .clone()
            .with_file_sessions(file_store.clone());

        let result = service
            .create_from_request(
                TaskCreateRequest {
                    name: "Invalid File Session".to_string(),
                    agent_id: session.agent_id,
                    chat_session_id: None,
                    schedule: types::request::TaskSchedule::default(),
                    input: None,
                    input_template: None,
                    timeout_secs: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result);

        assert!(result.is_err());
        assert!(file_store.list().unwrap().is_empty());
    }

    #[tokio::test]
    async fn update_direct_invalid_rebind_does_not_create_file_session() {
        let (mut service, session, dir) = setup();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        service.session_service = service
            .session_service
            .clone()
            .with_file_sessions(file_store.clone());
        let next_agent_id = create_agent_for_test(&service, &dir, "svc-agent-2");
        let created = service
            .create_from_request(
                TaskCreateRequest {
                    name: "Valid File Session".to_string(),
                    agent_id: session.agent_id,
                    chat_session_id: None,
                    schedule: types::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result)
            .expect("create direct");
        let session_count = file_store.list().unwrap().len();

        let result = service
            .update_from_request(
                TaskUpdateRequest {
                    id: created.id,
                    name: None,
                    description: None,
                    agent_id: Some(next_agent_id),
                    chat_session_id: None,
                    input: Some(" ".to_string()),
                    input_template: None,
                    schedule: None,
                    execution_mode: None,
                    timeout_secs: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result);

        assert!(result.is_err());
        assert_eq!(file_store.list().unwrap().len(), session_count);
    }

    #[tokio::test]
    async fn update_direct_rebind_archives_old_owned_file_session() {
        let (mut service, session, dir) = setup();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        service.session_service = service
            .session_service
            .clone()
            .with_file_sessions(file_store.clone());
        let task = service
            .create_from_request(
                TaskCreateRequest {
                    name: "Rebind File Session".to_string(),
                    agent_id: session.agent_id.clone(),
                    chat_session_id: None,
                    schedule: types::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result)
            .expect("create direct");
        let old_session_id = task.chat_session_id.clone();
        let external_session = service
            .session_service
            .create_external_session(
                ChatSession::new(
                    session.agent_id.clone(),
                    ModelId::Gpt5.as_serialized_str().to_string(),
                )
                .with_name("External Background"),
            )
            .expect("create external session");

        let updated = service
            .update_from_request(
                TaskUpdateRequest {
                    id: task.id,
                    name: None,
                    description: None,
                    agent_id: None,
                    chat_session_id: Some(external_session.id.clone()),
                    input: None,
                    input_template: None,
                    schedule: None,
                    execution_mode: None,
                    timeout_secs: None,
                    resource_limits: None,
                    preview: false,
                    approval_id: None,
                },
                TaskExecutionMode::Direct,
            )
            .await
            .and_then(TaskCommandService::into_direct_result)
            .expect("update direct");

        assert_eq!(updated.chat_session_id, external_session.id);
        assert!(!updated.owns_chat_session);
        assert!(
            file_store
                .get(&old_session_id)
                .unwrap()
                .unwrap()
                .to_chat_session()
                .is_archived()
        );
        assert!(
            !file_store
                .get(&updated.chat_session_id)
                .unwrap()
                .unwrap()
                .to_chat_session()
                .is_archived()
        );
    }

    #[tokio::test]
    async fn update_direct_executes_with_warning_assessment() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));
        let task = service
            .create_from_spec_direct(TaskSpec {
                name: "Update Direct Warning".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("update direct".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .update_from_request(
                TaskUpdateRequest {
                    id: task.id.clone(),
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
            .create_from_spec_direct(TaskSpec {
                name: "Control Direct Warning".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("control direct".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let result = service
            .control_from_request(
                TaskControlRequest {
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
        assert_eq!(result.status, crate::models::TaskStatus::Paused);
    }

    #[tokio::test]
    async fn convert_session_direct_executes_with_warning_assessment() {
        let (service, session, _dir) = setup_with_assessor(Arc::new(WarningAssessor));

        let result = service
            .convert_session(
                TaskConvertSessionRequest {
                    session_id: session.id.clone(),
                    name: Some("Converted Direct Warning".to_string()),
                    schedule: None,
                    input: None,
                    timeout_secs: None,
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
            .create_from_spec_direct(TaskSpec {
                name: "Canonical Task".to_string(),
                agent_id: session.agent_id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("canonical".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
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
                    schedule: types::request::TaskSchedule::default(),
                    input: Some("run".to_string()),
                    input_template: None,
                    timeout_secs: None,
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
                    execution_mode: None,
                    timeout_secs: None,
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
            .create_from_spec_direct(TaskSpec {
                name: "Delete Without Assessor".to_string(),
                agent_id: session.agent_id,
                chat_session_id: None,
                description: None,
                input: Some("delete requires assessor".to_string()),
                input_template: None,
                schedule: crate::models::TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("create task");

        let err = service
            .delete_from_request(
                TaskDeleteRequest {
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
