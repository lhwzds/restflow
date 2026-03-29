use crate::boundary::background_agent::{
    convert_session_request_to_options, create_request_to_spec, update_request_to_patch,
};
use crate::models::{
    BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentConversionResult,
    BackgroundAgentPatch, BackgroundAgentSpec, BackgroundMessage, BackgroundMessageSource,
    BackgroundProgress,
};
use crate::services::background_agent_conversion::{
    ConvertSessionSpecOptions, build_convert_session_spec,
};
use crate::services::session::SessionService;
use crate::storage::{AgentStorage, BackgroundAgentStorage, Storage};
use anyhow::{Result, anyhow};
use restflow_traits::store::{
    BackgroundAgentConvertSessionRequest, BackgroundAgentCreateRequest,
    BackgroundAgentUpdateRequest,
};

#[derive(Clone)]
pub struct BackgroundAgentCommandService {
    storage: BackgroundAgentStorage,
    agents: AgentStorage,
    session_service: SessionService,
}

impl BackgroundAgentCommandService {
    pub fn new(
        storage: BackgroundAgentStorage,
        agents: AgentStorage,
        session_service: SessionService,
    ) -> Self {
        Self {
            storage,
            agents,
            session_service,
        }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        Self::new(
            storage.background_agents.clone(),
            storage.agents.clone(),
            SessionService::from_storage(storage),
        )
    }

    pub fn create(&self, spec: BackgroundAgentSpec) -> Result<BackgroundAgent> {
        self.storage.create_background_agent(spec)
    }

    pub fn create_from_request(
        &self,
        request: BackgroundAgentCreateRequest,
        resolved_agent_id: String,
    ) -> Result<BackgroundAgent> {
        let spec =
            create_request_to_spec(request, resolved_agent_id).map_err(|error| anyhow!(error.to_string()))?;
        self.create(spec)
    }

    pub fn update(&self, id: &str, patch: BackgroundAgentPatch) -> Result<BackgroundAgent> {
        self.storage.update_background_agent(id, patch)
    }

    pub fn update_from_request(
        &self,
        request: BackgroundAgentUpdateRequest,
        resolved_agent_id: Option<String>,
    ) -> Result<BackgroundAgent> {
        let resolved_id = request.id.clone();
        let patch =
            update_request_to_patch(request, resolved_agent_id).map_err(|error| anyhow!(error.to_string()))?;
        self.update(&resolved_id, patch)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        self.storage.delete_task(id)
    }

    pub fn control(&self, id: &str, action: BackgroundAgentControlAction) -> Result<BackgroundAgent> {
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
        self.storage.send_background_agent_message(id, message, source)
    }

    pub fn convert_session(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> Result<BackgroundAgentConversionResult> {
        let session_id = request.session_id.trim().to_string();
        if session_id.is_empty() {
            return Err(anyhow!("session_id must not be empty"));
        }

        let session = self
            .session_service
            .get_session_view(&session_id)?
            .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;
        let options =
            convert_session_request_to_options(request).map_err(|error| anyhow!(error.to_string()))?;
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
    }

    pub fn resolve_default_or_existing_agent_id(&self, id_or_alias: &str) -> Result<String> {
        crate::boundary::background_agent::resolve_agent_id_alias(
            id_or_alias,
            || self.agents.resolve_default_agent_id(),
            |trimmed| self.agents.resolve_existing_agent_id(trimmed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::BackgroundAgentCommandService;
    use crate::models::{AgentNode, ChatMessage, ChatSession, ModelId};
    use crate::services::session::SessionService;
    use crate::storage::{
        AgentStorage, BackgroundAgentStorage, ChannelSessionBindingStorage, ChatSessionStorage,
        ExecutionTraceStorage, MemoryStorage, SessionStorage,
    };
    use crate::prompt_files;
    use restflow_traits::store::BackgroundAgentConvertSessionRequest;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (BackgroundAgentCommandService, ChatSession, tempfile::TempDir) {
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
        let session_storage = SessionStorage::new(chat_storage.clone(), binding_storage, trace_storage);
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
        let mut session =
            ChatSession::new(agent_id, ModelId::Gpt5.as_serialized_str().to_string())
                .with_name("Convert Me");
        session.add_message(ChatMessage::user("continue this task"));
        chat_storage.create(&session).expect("create session");

        (
            BackgroundAgentCommandService::new(background_storage, agent_storage, session_service),
            session,
            temp_dir,
        )
    }

    #[test]
    fn convert_session_returns_conversion_result() {
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
            })
            .expect("convert session");

        assert_eq!(result.source_session_id, session.id);
        assert_eq!(result.source_session_agent_id, session.agent_id);
        assert_eq!(result.task.chat_session_id, result.source_session_id);
        assert_eq!(result.task.name, "Converted Session");
        assert!(!result.run_now);
    }
}
