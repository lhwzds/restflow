use crate::boundary::background_agent::{
    convert_session_request_to_options, create_request_to_spec, parse_control_action,
    update_request_to_patch,
};
use crate::daemon::request_mapper::to_contract;
use crate::models::{
    BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentConversionResult,
    BackgroundAgentPatch, BackgroundAgentSpec, BackgroundMessage, BackgroundMessageSource,
    BackgroundProgress,
};
use crate::services::background_agent_conversion::{
    ConvertSessionSpecOptions, build_convert_session_spec,
};
use crate::services::operation_assessment::{
    assessment_requires_confirmation, ensure_assessment_confirmed,
};
use crate::services::session::SessionService;
use crate::storage::{AgentStorage, BackgroundAgentStorage, Storage};
use restflow_contracts::{DeleteWithIdResponse, ErrorKind, ErrorPayload};
use restflow_tools::ToolError;
use restflow_traits::store::{
    BackgroundAgentControlRequest, BackgroundAgentConvertSessionRequest,
    BackgroundAgentCreateRequest, BackgroundAgentDeleteRequest, BackgroundAgentUpdateRequest,
};
use restflow_traits::{
    AgentOperationAssessor, BackgroundAgentCommandOutcome, OperationAssessment,
    OperationAssessmentIntent, OperationAssessmentIssue,
};
use std::sync::Arc;

type CommandResult<T> = std::result::Result<T, BackgroundAgentCommandError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundAgentCommandError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl std::fmt::Display for BackgroundAgentCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Internal(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for BackgroundAgentCommandError {}

impl From<BackgroundAgentCommandError> for ToolError {
    fn from(error: BackgroundAgentCommandError) -> Self {
        ToolError::Tool(error.to_string())
    }
}

impl BackgroundAgentCommandError {
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
pub struct BackgroundAgentCommandService {
    storage: BackgroundAgentStorage,
    agents: AgentStorage,
    session_service: SessionService,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
}

impl BackgroundAgentCommandService {
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

    fn create(&self, spec: BackgroundAgentSpec) -> CommandResult<BackgroundAgent> {
        self.storage
            .create_background_agent(spec)
            .map_err(BackgroundAgentCommandError::from_anyhow)
    }

    pub async fn create_from_request(
        &self,
        request: BackgroundAgentCreateRequest,
    ) -> CommandResult<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let request = self.normalize_create_request(request)?;
        self.validate_create_request(&request)?;
        let assessment = self
            .assessor()?
            .assess_background_agent_create(request.clone())
            .await
            .map_err(BackgroundAgentCommandError::from_tool_error)?;
        let preview = request.preview;
        let confirmation_token = request.confirmation_token.clone();
        let spec = create_request_to_spec(request)
            .map_err(BackgroundAgentCommandError::from_tool_error)?;
        self.finish_mutation(assessment, preview, confirmation_token.as_deref(), || {
            self.create(spec)
        })
    }

    fn update(&self, id: &str, patch: BackgroundAgentPatch) -> CommandResult<BackgroundAgent> {
        self.storage
            .update_background_agent(id, patch)
            .map_err(BackgroundAgentCommandError::from_anyhow)
    }

    pub async fn update_from_request(
        &self,
        request: BackgroundAgentUpdateRequest,
    ) -> CommandResult<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let request = self.normalize_update_request(request)?;
        self.validate_update_request(&request)?;
        let resolved_id = request.id.clone();
        let assessment = self
            .assessor()?
            .assess_background_agent_update(request.clone())
            .await
            .map_err(BackgroundAgentCommandError::from_tool_error)?;
        let preview = request.preview;
        let confirmation_token = request.confirmation_token.clone();
        let patch = update_request_to_patch(request)
            .map_err(BackgroundAgentCommandError::from_tool_error)?;
        self.finish_mutation(assessment, preview, confirmation_token.as_deref(), || {
            self.update(&resolved_id, patch)
        })
    }

    pub fn delete(&self, id: &str) -> CommandResult<bool> {
        self.storage
            .delete_task(id)
            .map_err(BackgroundAgentCommandError::from_anyhow)
    }

    pub async fn delete_from_request(
        &self,
        request: BackgroundAgentDeleteRequest,
    ) -> CommandResult<BackgroundAgentCommandOutcome<DeleteWithIdResponse>> {
        let request = self.normalize_delete_request(request)?;
        self.validate_delete_request(&request)?;
        let resolved_id = request.id.clone();
        let preview = request.preview;
        let confirmation_token = request.confirmation_token.clone();
        let assessment = Self::build_delete_assessment(&resolved_id);
        self.finish_mutation(assessment, preview, confirmation_token.as_deref(), || {
            let deleted = self.delete(&resolved_id)?;
            Ok(DeleteWithIdResponse {
                id: resolved_id,
                deleted,
            })
        })
    }

    fn control(
        &self,
        id: &str,
        action: BackgroundAgentControlAction,
    ) -> CommandResult<BackgroundAgent> {
        self.storage
            .control_background_agent(id, action)
            .map_err(BackgroundAgentCommandError::from_anyhow)
    }

    pub fn progress(&self, id: &str, event_limit: usize) -> CommandResult<BackgroundProgress> {
        self.storage
            .get_background_agent_progress(id, event_limit)
            .map_err(BackgroundAgentCommandError::from_anyhow)
    }

    pub fn send_message(
        &self,
        id: &str,
        message: String,
        source: BackgroundMessageSource,
    ) -> CommandResult<BackgroundMessage> {
        self.storage
            .send_background_agent_message(id, message, source)
            .map_err(BackgroundAgentCommandError::from_anyhow)
    }

    pub async fn control_from_request(
        &self,
        request: BackgroundAgentControlRequest,
    ) -> CommandResult<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let (request, action) = self.normalize_control_request(request)?;
        self.validate_control_request(&request)?;
        let resolved_id = request.id.clone();
        let assessment = self
            .assessor()?
            .assess_background_agent_control(request.clone())
            .await
            .map_err(BackgroundAgentCommandError::from_tool_error)?;
        let preview = request.preview;
        let confirmation_token = request.confirmation_token.clone();
        self.finish_mutation(assessment, preview, confirmation_token.as_deref(), || {
            self.control(&resolved_id, action)
        })
    }

    pub async fn convert_session(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> CommandResult<BackgroundAgentCommandOutcome<BackgroundAgentConversionResult>> {
        let request = self.normalize_convert_session_request(request);
        self.validate_convert_session_request(&request)?;
        let session_id = request.session_id.clone();

        let assessment = self
            .assessor()?
            .assess_background_agent_convert_session(request.clone())
            .await
            .map_err(BackgroundAgentCommandError::from_tool_error)?;
        let preview = request.preview;
        let confirmation_token = request.confirmation_token.clone();

        let session = self
            .session_service
            .get_session_view(&session_id)
            .map_err(BackgroundAgentCommandError::from_anyhow)?
            .ok_or_else(|| {
                BackgroundAgentCommandError::not_found(format!("Session not found: {}", session_id))
            })?;
        let options = convert_session_request_to_options(request)
            .map_err(BackgroundAgentCommandError::from_tool_error)?;
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
        .map_err(|error| BackgroundAgentCommandError::internal(error.to_string()))?;

        self.finish_mutation(assessment, preview, confirmation_token.as_deref(), || {
            let mut task = self
                .storage
                .create_background_agent(spec)
                .map_err(BackgroundAgentCommandError::from_anyhow)?;
            if options.run_now {
                task = self
                    .storage
                    .control_background_agent(&task.id, BackgroundAgentControlAction::RunNow)
                    .map_err(BackgroundAgentCommandError::from_anyhow)?;
            }
            Ok(BackgroundAgentConversionResult {
                task,
                source_session_id: session.id,
                source_session_agent_id: session.agent_id,
                run_now: options.run_now,
            })
        })
    }

    pub fn resolve_default_or_existing_agent_id(&self, id_or_alias: &str) -> CommandResult<String> {
        crate::boundary::background_agent::resolve_agent_id_alias(
            id_or_alias,
            || self.agents.resolve_default_agent_id(),
            |trimmed| self.agents.resolve_existing_agent_id(trimmed),
        )
        .map_err(BackgroundAgentCommandError::from_anyhow)
    }

    fn assessor(&self) -> CommandResult<Arc<dyn AgentOperationAssessor>> {
        self.assessor.clone().ok_or_else(|| {
            BackgroundAgentCommandError::internal(
                "Background-agent capability assessment is unavailable in this runtime.",
            )
        })
    }

    fn normalize_create_request(
        &self,
        mut request: BackgroundAgentCreateRequest,
    ) -> CommandResult<BackgroundAgentCreateRequest> {
        if request.agent_id.trim().is_empty() {
            return Err(BackgroundAgentCommandError::validation(
                "agent_id must not be empty",
            ));
        }
        request.agent_id = self.resolve_default_or_existing_agent_id(&request.agent_id)?;
        Ok(request)
    }

    fn normalize_update_request(
        &self,
        mut request: BackgroundAgentUpdateRequest,
    ) -> CommandResult<BackgroundAgentUpdateRequest> {
        if request.id.trim().is_empty() {
            return Err(BackgroundAgentCommandError::validation(
                "id must not be empty",
            ));
        }
        request.id = self
            .storage
            .resolve_existing_task_id(&request.id)
            .map_err(BackgroundAgentCommandError::from_anyhow)?;
        if let Some(agent_id) = request.agent_id.clone() {
            if agent_id.trim().is_empty() {
                return Err(BackgroundAgentCommandError::validation(
                    "agent_id must not be empty",
                ));
            }
            request.agent_id = Some(self.resolve_default_or_existing_agent_id(&agent_id)?);
        }
        Ok(request)
    }

    fn normalize_control_request(
        &self,
        mut request: BackgroundAgentControlRequest,
    ) -> CommandResult<(BackgroundAgentControlRequest, BackgroundAgentControlAction)> {
        if request.id.trim().is_empty() {
            return Err(BackgroundAgentCommandError::validation(
                "id must not be empty",
            ));
        }
        request.id = self
            .storage
            .resolve_existing_task_id(&request.id)
            .map_err(BackgroundAgentCommandError::from_anyhow)?;
        let action = parse_control_action(&request.action)
            .map_err(BackgroundAgentCommandError::from_tool_error)?;
        request.action =
            to_contract(action.clone()).map_err(BackgroundAgentCommandError::from_anyhow)?;
        Ok((request, action))
    }

    fn normalize_delete_request(
        &self,
        mut request: BackgroundAgentDeleteRequest,
    ) -> CommandResult<BackgroundAgentDeleteRequest> {
        if request.id.trim().is_empty() {
            return Err(BackgroundAgentCommandError::validation(
                "id must not be empty",
            ));
        }
        request.id = self
            .storage
            .resolve_existing_task_id(&request.id)
            .map_err(BackgroundAgentCommandError::from_anyhow)?;
        Ok(request)
    }

    fn normalize_convert_session_request(
        &self,
        mut request: BackgroundAgentConvertSessionRequest,
    ) -> BackgroundAgentConvertSessionRequest {
        request.session_id = request.session_id.trim().to_string();
        request.run_now = Some(request.run_now.unwrap_or(false));
        request
    }

    fn validate_create_request(&self, request: &BackgroundAgentCreateRequest) -> CommandResult<()> {
        if request.name.trim().is_empty() {
            return Err(BackgroundAgentCommandError::validation(
                "name must not be empty",
            ));
        }
        Ok(())
    }

    fn validate_update_request(&self, request: &BackgroundAgentUpdateRequest) -> CommandResult<()> {
        if let Some(name) = request.name.as_deref()
            && name.trim().is_empty()
        {
            return Err(BackgroundAgentCommandError::validation(
                "name must not be empty",
            ));
        }
        Ok(())
    }

    fn validate_control_request(
        &self,
        request: &BackgroundAgentControlRequest,
    ) -> CommandResult<()> {
        if request.action.trim().is_empty() {
            return Err(BackgroundAgentCommandError::validation(
                "action must not be empty",
            ));
        }
        Ok(())
    }

    fn validate_delete_request(&self, request: &BackgroundAgentDeleteRequest) -> CommandResult<()> {
        if request.id.trim().is_empty() {
            return Err(BackgroundAgentCommandError::validation(
                "id must not be empty",
            ));
        }
        Ok(())
    }

    fn validate_convert_session_request(
        &self,
        request: &BackgroundAgentConvertSessionRequest,
    ) -> CommandResult<()> {
        if request.session_id.is_empty() {
            return Err(BackgroundAgentCommandError::validation(
                "session_id must not be empty",
            ));
        }
        if let Some(name) = request.name.as_deref()
            && name.trim().is_empty()
        {
            return Err(BackgroundAgentCommandError::validation(
                "name must not be empty",
            ));
        }
        Ok(())
    }

    fn finish_mutation<T>(
        &self,
        assessment: OperationAssessment,
        preview: bool,
        confirmation_token: Option<&str>,
        execute: impl FnOnce() -> CommandResult<T>,
    ) -> CommandResult<BackgroundAgentCommandOutcome<T>> {
        if preview {
            return Ok(BackgroundAgentCommandOutcome::Preview { assessment });
        }
        if !assessment.blockers.is_empty() {
            return Ok(BackgroundAgentCommandOutcome::Blocked { assessment });
        }
        if assessment_requires_confirmation(&assessment)
            && ensure_assessment_confirmed(&assessment, confirmation_token).is_err()
        {
            return Ok(BackgroundAgentCommandOutcome::ConfirmationRequired { assessment });
        }
        Ok(BackgroundAgentCommandOutcome::Executed { result: execute()? })
    }

    fn build_delete_assessment(id: &str) -> OperationAssessment {
        OperationAssessment::warning_with_confirmation(
            "delete_background_agent",
            OperationAssessmentIntent::Save,
            vec![OperationAssessmentIssue {
                code: "destructive_delete".to_string(),
                message: format!(
                    "Deleting background agent '{id}' removes its persisted definition and run history."
                ),
                field: Some("id".to_string()),
                suggestion: Some(
                    "Confirm the deletion only if you intend to permanently remove this background agent."
                        .to_string(),
                ),
            }],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::BackgroundAgentCommandService;
    use crate::models::{AgentNode, BackgroundAgentSpec, ChatMessage, ChatSession, ModelId};
    use crate::prompt_files;
    use crate::services::session::SessionService;
    use crate::storage::{
        AgentStorage, BackgroundAgentStorage, ChannelSessionBindingStorage, ChatSessionStorage,
        ExecutionTraceStorage, MemoryStorage, SessionStorage,
    };
    use async_trait::async_trait;
    use restflow_traits::BackgroundAgentCommandOutcome;
    use restflow_traits::ContractSubagentSpawnRequest;
    use restflow_traits::ToolError;
    use restflow_traits::assessment::{
        AgentOperationAssessor, OperationAssessment, OperationAssessmentIntent,
    };
    use restflow_traits::store::{
        AgentCreateRequest, AgentUpdateRequest, BackgroundAgentControlRequest,
        BackgroundAgentConvertSessionRequest, BackgroundAgentCreateRequest,
        BackgroundAgentDeleteRequest, BackgroundAgentUpdateRequest,
    };
    use std::sync::Arc;
    use tempfile::tempdir;

    struct MockAssessor;

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

    fn setup() -> (
        BackgroundAgentCommandService,
        ChatSession,
        tempfile::TempDir,
    ) {
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
            BackgroundAgentCommandService::new(
                background_storage,
                agent_storage,
                session_service,
                Some(Arc::new(MockAssessor)),
            ),
            session,
            temp_dir,
        )
    }

    #[tokio::test]
    async fn convert_session_returns_conversion_result() {
        let (service, session, _dir) = setup();
        let result = service
            .convert_session(BackgroundAgentConvertSessionRequest {
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
                confirmation_token: None,
            })
            .await
            .expect("convert session");

        match result {
            BackgroundAgentCommandOutcome::Executed { result } => {
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
            .convert_session(BackgroundAgentConvertSessionRequest {
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
                confirmation_token: None,
            })
            .await
            .expect("preview convert");

        match result {
            BackgroundAgentCommandOutcome::Preview { assessment } => {
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
            .create_from_request(BackgroundAgentCreateRequest {
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
                confirmation_token: None,
            })
            .await
            .expect_err("blank name should fail");
        assert!(err.to_string().contains("name must not be empty"));
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
            .delete_from_request(BackgroundAgentDeleteRequest {
                id: task.id.clone(),
                preview: true,
                confirmation_token: None,
            })
            .await
            .expect("delete preview");

        match result {
            BackgroundAgentCommandOutcome::Preview { assessment } => {
                assert_eq!(assessment.operation, "delete_background_agent");
                assert!(assessment.requires_confirmation);
                assert!(assessment.confirmation_token.is_some());
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
            .delete_from_request(BackgroundAgentDeleteRequest {
                id: task.id.clone(),
                preview: false,
                confirmation_token: None,
            })
            .await
            .expect("delete should return confirmation_required");

        match result {
            BackgroundAgentCommandOutcome::ConfirmationRequired { assessment } => {
                assert_eq!(assessment.operation, "delete_background_agent");
                assert!(assessment.requires_confirmation);
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
    async fn delete_executes_when_confirmation_token_matches() {
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
            .delete_from_request(BackgroundAgentDeleteRequest {
                id: task.id.clone(),
                preview: true,
                confirmation_token: None,
            })
            .await
            .expect("delete preview");

        let token = match preview {
            BackgroundAgentCommandOutcome::Preview { assessment } => assessment
                .confirmation_token
                .expect("delete preview should carry confirmation token"),
            other => panic!("expected preview outcome, got {other:?}"),
        };

        let result = service
            .delete_from_request(BackgroundAgentDeleteRequest {
                id: task.id.clone(),
                preview: false,
                confirmation_token: Some(token),
            })
            .await
            .expect("delete confirmed");

        match result {
            BackgroundAgentCommandOutcome::Executed { result } => {
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
}
