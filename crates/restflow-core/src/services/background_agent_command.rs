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
use anyhow::{Result, anyhow};
use restflow_traits::store::{
    BackgroundAgentControlRequest, BackgroundAgentConvertSessionRequest,
    BackgroundAgentCreateRequest, BackgroundAgentUpdateRequest,
};
use restflow_traits::{AgentOperationAssessor, BackgroundAgentCommandOutcome, OperationAssessment};
use std::sync::Arc;

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

    fn create(&self, spec: BackgroundAgentSpec) -> Result<BackgroundAgent> {
        self.storage.create_background_agent(spec)
    }

    pub async fn create_from_request(
        &self,
        request: BackgroundAgentCreateRequest,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let request = self.normalize_create_request(request)?;
        let assessment = self
            .assessor()?
            .assess_background_agent_create(request.clone())
            .await
            .map_err(|error| anyhow!(error.to_string()))?;
        let preview = request.preview;
        let confirmation_token = request.confirmation_token.clone();
        let spec = create_request_to_spec(request).map_err(|error| anyhow!(error.to_string()))?;
        self.finish_mutation(assessment, preview, confirmation_token.as_deref(), || {
            self.create(spec)
        })
    }

    fn update(&self, id: &str, patch: BackgroundAgentPatch) -> Result<BackgroundAgent> {
        self.storage.update_background_agent(id, patch)
    }

    pub async fn update_from_request(
        &self,
        request: BackgroundAgentUpdateRequest,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let request = self.normalize_update_request(request)?;
        let resolved_id = request.id.clone();
        let assessment = self
            .assessor()?
            .assess_background_agent_update(request.clone())
            .await
            .map_err(|error| anyhow!(error.to_string()))?;
        let preview = request.preview;
        let confirmation_token = request.confirmation_token.clone();
        let patch = update_request_to_patch(request).map_err(|error| anyhow!(error.to_string()))?;
        self.finish_mutation(assessment, preview, confirmation_token.as_deref(), || {
            self.update(&resolved_id, patch)
        })
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        self.storage.delete_task(id)
    }

    fn control(&self, id: &str, action: BackgroundAgentControlAction) -> Result<BackgroundAgent> {
        self.storage.control_background_agent(id, action)
    }

    pub fn progress(&self, id: &str, event_limit: usize) -> Result<BackgroundProgress> {
        self.storage.get_background_agent_progress(id, event_limit)
    }

    pub fn send_message(
        &self,
        id: &str,
        message: String,
        source: BackgroundMessageSource,
    ) -> Result<BackgroundMessage> {
        self.storage
            .send_background_agent_message(id, message, source)
    }

    pub async fn control_from_request(
        &self,
        request: BackgroundAgentControlRequest,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let (request, action) = self.normalize_control_request(request)?;
        let resolved_id = request.id.clone();
        let assessment = self
            .assessor()?
            .assess_background_agent_control(request.clone())
            .await
            .map_err(|error| anyhow!(error.to_string()))?;
        let preview = request.preview;
        let confirmation_token = request.confirmation_token.clone();
        self.finish_mutation(assessment, preview, confirmation_token.as_deref(), || {
            self.control(&resolved_id, action)
        })
    }

    pub async fn convert_session(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgentConversionResult>> {
        let request = self.normalize_convert_session_request(request);
        let session_id = request.session_id.clone();
        if session_id.is_empty() {
            return Err(anyhow!("session_id must not be empty"));
        }

        let assessment = self
            .assessor()?
            .assess_background_agent_convert_session(request.clone())
            .await
            .map_err(|error| anyhow!(error.to_string()))?;
        let preview = request.preview;
        let confirmation_token = request.confirmation_token.clone();

        let session = self
            .session_service
            .get_session_view(&session_id)?
            .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;
        let options = convert_session_request_to_options(request)
            .map_err(|error| anyhow!(error.to_string()))?;
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
        .map_err(|error| anyhow!(error))?;

        self.finish_mutation(assessment, preview, confirmation_token.as_deref(), || {
            let mut task = self.storage.create_background_agent(spec)?;
            if options.run_now {
                task = self
                    .storage
                    .control_background_agent(&task.id, BackgroundAgentControlAction::RunNow)?;
            }
            Ok(BackgroundAgentConversionResult {
                task,
                source_session_id: session.id,
                source_session_agent_id: session.agent_id,
                run_now: options.run_now,
            })
        })
    }

    pub fn resolve_default_or_existing_agent_id(&self, id_or_alias: &str) -> Result<String> {
        crate::boundary::background_agent::resolve_agent_id_alias(
            id_or_alias,
            || self.agents.resolve_default_agent_id(),
            |trimmed| self.agents.resolve_existing_agent_id(trimmed),
        )
    }

    fn assessor(&self) -> Result<Arc<dyn AgentOperationAssessor>> {
        self.assessor.clone().ok_or_else(|| {
            anyhow!("Background-agent capability assessment is unavailable in this runtime.")
        })
    }

    fn normalize_create_request(
        &self,
        mut request: BackgroundAgentCreateRequest,
    ) -> Result<BackgroundAgentCreateRequest> {
        request.agent_id = self.resolve_default_or_existing_agent_id(&request.agent_id)?;
        Ok(request)
    }

    fn normalize_update_request(
        &self,
        mut request: BackgroundAgentUpdateRequest,
    ) -> Result<BackgroundAgentUpdateRequest> {
        request.id = self.storage.resolve_existing_task_id(&request.id)?;
        if let Some(agent_id) = request.agent_id.clone() {
            request.agent_id = Some(self.resolve_default_or_existing_agent_id(&agent_id)?);
        }
        Ok(request)
    }

    fn normalize_control_request(
        &self,
        mut request: BackgroundAgentControlRequest,
    ) -> Result<(BackgroundAgentControlRequest, BackgroundAgentControlAction)> {
        request.id = self.storage.resolve_existing_task_id(&request.id)?;
        let action =
            parse_control_action(&request.action).map_err(|error| anyhow!(error.to_string()))?;
        request.action = to_contract(action.clone()).map_err(|error| anyhow!(error.to_string()))?;
        Ok((request, action))
    }

    fn normalize_convert_session_request(
        &self,
        mut request: BackgroundAgentConvertSessionRequest,
    ) -> BackgroundAgentConvertSessionRequest {
        request.session_id = request.session_id.trim().to_string();
        request.run_now = Some(request.run_now.unwrap_or(false));
        request
    }

    fn finish_mutation<T>(
        &self,
        assessment: OperationAssessment,
        preview: bool,
        confirmation_token: Option<&str>,
        execute: impl FnOnce() -> Result<T>,
    ) -> Result<BackgroundAgentCommandOutcome<T>> {
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
}

#[cfg(test)]
mod tests {
    use super::BackgroundAgentCommandService;
    use crate::models::{AgentNode, ChatMessage, ChatSession, ModelId};
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
        BackgroundAgentUpdateRequest,
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
    async fn create_fails_without_schedule() {
        let (service, _session, _dir) = setup();
        let err = service
            .create_from_request(BackgroundAgentCreateRequest {
                name: "Missing Schedule".to_string(),
                agent_id: "default".to_string(),
                chat_session_id: None,
                schedule: None,
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
            .expect_err("missing schedule should fail");
        assert!(err.to_string().contains("Missing required field: schedule"));
    }
}
