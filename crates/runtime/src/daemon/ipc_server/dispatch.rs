use super::runtime::{
    build_agent_system_prompt, cancel_chat_stream, get_runtime_tool_registry, resolve_agent_id,
    steer_chat_stream,
};
use super::*;
use crate::auth::secret_exists;
use crate::daemon::request_mapper::{
    from_contract, invalid_request_response, invalid_validation_response,
};
use crate::daemon::tool_result_mapper::to_tool_execution_result;
use crate::provider_policy::{provider_allows_secret_env, provider_display_order};
use crate::services::execution_console::{ExecutionConsoleService, ExecutionThreadError};
use crate::services::operation_assessment::{
    assess_agent_create, assess_agent_update, assessment_summary,
};
use serde_json::json;
use types::request::{AgentNode as ContractAgentNode, WireModelRef};
use types::store::{AgentCreateRequest, AgentUpdateRequest};
use types::{
    ArchiveResponse, CancelResponse, CleanupReportResponse, DeleteResponse, ErrorKind,
    ModelMetadataDTO, OkResponse, OperationAssessment, PromptResponse, Provider, SecretResponse,
    SteerResponse,
};

fn assessment_details(assessment: &OperationAssessment) -> serde_json::Value {
    json!({ "assessment": assessment })
}

fn blocked_assessment_response(assessment: OperationAssessment) -> IpcResponse {
    IpcResponse::error_payload(types::ErrorPayload::with_kind(
        400,
        ErrorKind::Validation,
        assessment_summary(&assessment),
        Some(assessment_details(&assessment)),
    ))
}

fn is_catalog_model(model: ModelId) -> bool {
    !model.is_opencode_cli() && !model.is_gemini_cli() && !is_legacy_openai_model(model)
}

fn is_legacy_openai_model(model: ModelId) -> bool {
    matches!(
        model,
        ModelId::Gpt5
            | ModelId::Gpt5Mini
            | ModelId::Gpt5Nano
            | ModelId::Gpt5Pro
            | ModelId::Gpt5_1
            | ModelId::Gpt5_2
    )
}

fn available_providers(core: &Arc<AppCore>) -> Vec<Provider> {
    let mut providers = Vec::new();
    for provider in Provider::all().iter().copied() {
        let available = provider == Provider::Codex
            || provider_allows_secret_env(provider)
                && provider
                    .api_key_env_candidates()
                    .any(|key| secret_exists(&core.storage.secrets, key));

        if available {
            providers.push(provider);
        }
    }

    providers.sort_by_key(|provider| provider_display_order(*provider));
    providers
}

fn available_model_catalog(core: &Arc<AppCore>) -> Vec<ModelMetadataDTO> {
    let providers = available_providers(core);
    let mut models = ModelId::all_with_metadata()
        .into_iter()
        .filter(|metadata| is_catalog_model(metadata.model))
        .filter(|metadata| providers.contains(&metadata.provider))
        .collect::<Vec<_>>();

    models.sort_by(|left, right| {
        provider_display_order(left.provider)
            .cmp(&provider_display_order(right.provider))
            .then_with(|| left.name.cmp(&right.name))
    });

    models
}

fn map_execution_thread_response(
    result: std::result::Result<types::ExecutionThread, ExecutionThreadError>,
) -> IpcResponse {
    match result {
        Ok(thread) => IpcResponse::success(thread),
        Err(ExecutionThreadError::InvalidQuery) => {
            IpcResponse::error(400, ExecutionThreadError::InvalidQuery.to_string())
        }
        Err(ExecutionThreadError::RunNotFound(_)) => IpcResponse::not_found("ExecutionThread"),
        Err(ExecutionThreadError::Internal(err)) => IpcResponse::error(500, err.to_string()),
    }
}

fn message_for_role(role: ChatRole, content: String) -> ChatMessage {
    let mut message = match role {
        ChatRole::User => ChatMessage::user(content),
        ChatRole::Assistant => ChatMessage::assistant(content),
        ChatRole::System => ChatMessage::system(content),
    };
    if message.role == ChatRole::Assistant && message.execution.is_none() {
        message.execution = Some(MessageExecution {
            steps: Vec::new(),
            duration_ms: 0,
            tokens_used: 0,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            status: ChatExecutionStatus::Completed,
        });
    }
    hydrate_voice_message_metadata(&mut message);
    message
}

fn append_message_to_session(
    storage: &crate::storage::Storage,
    session: &mut ChatSession,
    mut message: ChatMessage,
) -> IpcResponse {
    if message.role == ChatRole::Assistant && message.execution.is_none() {
        message.execution = Some(MessageExecution {
            steps: Vec::new(),
            duration_ms: 0,
            tokens_used: 0,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            status: ChatExecutionStatus::Completed,
        });
    }
    hydrate_voice_message_metadata(&mut message);
    session.add_message(message);
    if session.name == "New Chat" && session.messages.len() == 1 {
        session.auto_name_from_first_message();
    }
    match SessionService::from_storage(storage).save_existing_session(session, "ipc") {
        Ok(()) => IpcResponse::success(session.clone()),
        Err(err) => IpcResponse::error(500, err.to_string()),
    }
}

impl IpcServer {
    pub(super) async fn handle_ping() -> IpcResponse {
        IpcResponse::Pong
    }

    pub(super) async fn handle_get_status() -> IpcResponse {
        IpcResponse::success(build_daemon_status())
    }

    pub(super) async fn handle_list_agents(core: &Arc<AppCore>) -> IpcResponse {
        match agent_service::list_agents(core).await {
            Ok(agents) => IpcResponse::success(agents),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_agent(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match agent_service::get_agent(core, &id).await {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_agent(
        core: &Arc<AppCore>,
        name: String,
        agent: types::AgentNode,
    ) -> IpcResponse {
        let assessment = match assess_agent_create(
            core,
            AgentCreateRequest {
                name: name.clone(),
                agent: ContractAgentNode::from(agent.clone()),
            },
        )
        .await
        {
            Ok(assessment) => assessment,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        if !assessment.blockers.is_empty() {
            return blocked_assessment_response(assessment);
        }

        match agent_service::create_agent(core, name, agent).await {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_agent(
        core: &Arc<AppCore>,
        id: String,
        name: Option<String>,
        agent: Option<types::AgentNode>,
    ) -> IpcResponse {
        let assessment = match assess_agent_update(
            core,
            AgentUpdateRequest {
                id: id.clone(),
                name: name.clone(),
                agent: agent.clone().map(ContractAgentNode::from),
            },
        )
        .await
        {
            Ok(assessment) => assessment,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        if !assessment.blockers.is_empty() {
            return blocked_assessment_response(assessment);
        }

        match agent_service::update_agent(core, &id, name, agent).await {
            Ok(agent) => IpcResponse::success(agent),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_agent(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match agent_service::delete_agent(core, &id).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_skills(core: &Arc<AppCore>) -> IpcResponse {
        match skills_service::list_skills(core).await {
            Ok(skills) => IpcResponse::success(skills),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_skill(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match skills_service::get_skill(core, &id).await {
            Ok(Some(skill)) => IpcResponse::success(skill),
            Ok(None) => IpcResponse::not_found("Skill"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_skill_reference(
        core: &Arc<AppCore>,
        skill_id: String,
        ref_id: String,
    ) -> IpcResponse {
        match skills_service::get_skill_reference(core, &skill_id, &ref_id).await {
            Ok(Some(content)) => IpcResponse::success(content),
            Ok(None) => IpcResponse::not_found("Skill reference"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_run_cleanup(core: &Arc<AppCore>) -> IpcResponse {
        match crate::services::cleanup::run_cleanup(core).await {
            Ok(report) => IpcResponse::success(CleanupReportResponse {
                chat_sessions: report.chat_sessions,
                tasks: report.tasks,
                audit_events: report.audit_events,
                daemon_log_files: report.daemon_log_files,
            }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_secrets(core: &Arc<AppCore>) -> IpcResponse {
        match secrets_service::list_secrets(core).await {
            Ok(secrets) => IpcResponse::success(secrets),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_secret(core: &Arc<AppCore>, key: String) -> IpcResponse {
        match secrets_service::get_secret(core, &key).await {
            Ok(Some(value)) => IpcResponse::success(SecretResponse { value: Some(value) }),
            Ok(None) => IpcResponse::not_found("Secret"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_set_secret(
        core: &Arc<AppCore>,
        key: String,
        value: String,
        description: Option<String>,
    ) -> IpcResponse {
        match secrets_service::set_secret(core, &key, &value, description).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_secret(
        core: &Arc<AppCore>,
        key: String,
        value: String,
        description: Option<String>,
    ) -> IpcResponse {
        match secrets_service::create_secret(core, &key, &value, description).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_secret(
        core: &Arc<AppCore>,
        key: String,
        value: String,
        description: Option<String>,
    ) -> IpcResponse {
        match secrets_service::update_secret(core, &key, &value, description).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_secret(core: &Arc<AppCore>, key: String) -> IpcResponse {
        match secrets_service::delete_secret(core, &key).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_config(core: &Arc<AppCore>) -> IpcResponse {
        match config_service::get_config(core).await {
            Ok(config) => IpcResponse::success(config),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_global_config(core: &Arc<AppCore>) -> IpcResponse {
        match config_service::get_global_config(core).await {
            Ok(config) => IpcResponse::success(config),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_set_config(
        core: &Arc<AppCore>,
        config: crate::storage::SystemConfig,
    ) -> IpcResponse {
        match config_service::update_config(core, config).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_execution_containers(core: &Arc<AppCore>) -> IpcResponse {
        let service = ExecutionConsoleService::from_storage(&core.storage);
        match service.list_execution_containers() {
            Ok(containers) => IpcResponse::success(containers),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_runs(
        core: &Arc<AppCore>,
        query: types::RunListQuery,
    ) -> IpcResponse {
        let service = ExecutionConsoleService::from_storage(&core.storage);
        match service.list_runs(&query) {
            Ok(sessions) => IpcResponse::success(sessions),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_execution_run_thread(
        core: &Arc<AppCore>,
        run_id: String,
    ) -> IpcResponse {
        let run_id = run_id.trim().to_string();
        if run_id.is_empty() {
            return IpcResponse::error(400, "run_id is required");
        }

        let service = ExecutionConsoleService::from_storage(&core.storage);
        map_execution_thread_response(service.get_execution_run_thread(&run_id))
    }

    pub(super) async fn handle_list_child_runs(
        core: &Arc<AppCore>,
        query: types::ChildRunListQuery,
    ) -> IpcResponse {
        let parent_run_id = query.parent_run_id.trim().to_string();
        if parent_run_id.is_empty() {
            return IpcResponse::error(400, "parent_run_id is required");
        }

        let service = ExecutionConsoleService::from_storage(&core.storage);
        match service.list_child_runs(&parent_run_id) {
            Ok(sessions) => IpcResponse::success(sessions),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_sessions(core: &Arc<AppCore>) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.list_session_summaries(None, None, false) {
            Ok(summaries) => IpcResponse::success(summaries),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_full_sessions(core: &Arc<AppCore>) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.list_session_views(None, None, false) {
            Ok(sessions) => IpcResponse::success(sessions),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_sessions_by_agent(
        core: &Arc<AppCore>,
        agent_id: String,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.list_session_views(Some(&agent_id), None, false) {
            Ok(sessions) => IpcResponse::success(sessions),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_sessions_by_skill(
        core: &Arc<AppCore>,
        skill_id: String,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.list_session_views(None, Some(&skill_id), false) {
            Ok(sessions) => IpcResponse::success(sessions),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_count_sessions(core: &Arc<AppCore>) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.list_session_summaries(None, None, false) {
            Ok(sessions) => IpcResponse::success(sessions.len()),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_sessions_older_than(
        core: &Arc<AppCore>,
        older_than_ms: i64,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.cleanup_workspace_sessions_older_than(older_than_ms) {
            Ok(stats) => IpcResponse::success(stats.deleted),
            Err(err) => ipc_session_lifecycle_error(err),
        }
    }

    pub(super) async fn handle_get_session(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.get_session_view(&id) {
            Ok(Some(session)) => IpcResponse::success(session),
            Ok(None) => IpcResponse::not_found("Session"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_session(
        core: &Arc<AppCore>,
        agent_id: Option<String>,
        model: Option<String>,
        name: Option<String>,
        skill_id: Option<String>,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        let agent_id = match resolve_agent_id(core, agent_id) {
            Ok(agent_id) => agent_id,
            Err(err) => return IpcResponse::error(400, err.to_string()),
        };
        let model = match model {
            Some(model) => match normalize_model_input(&model) {
                Ok(normalized) => normalized,
                Err(err) => return IpcResponse::error(400, err.to_string()),
            },
            None => match core.storage.agents.get_agent(agent_id.clone()) {
                Ok(Some(agent)) => agent
                    .agent
                    .resolved_model_ref()
                    .map(|model_ref| model_ref.model.as_serialized_str().to_string())
                    .unwrap_or_else(|| ModelId::Gpt5_4.as_serialized_str().to_string()),
                Ok(None) => ModelId::Gpt5_4.as_serialized_str().to_string(),
                Err(err) => return IpcResponse::error(500, err.to_string()),
            },
        };
        match session_service.create_workspace_session(agent_id, model, name, skill_id, None) {
            Ok(session) => IpcResponse::success(session),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_session(
        core: &Arc<AppCore>,
        id: String,
        updates: types::ChatSessionUpdate,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        let validated_updates = types::ChatSessionUpdate {
            agent_id: match updates.agent_id {
                Some(agent_id) => match core.storage.agents.resolve_existing_agent_id(&agent_id) {
                    Ok(resolved) => Some(resolved),
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                },
                None => None,
            },
            model: match updates.model {
                Some(model) => match normalize_model_input(&model) {
                    Ok(normalized) => Some(normalized),
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                },
                None => None,
            },
            name: updates.name,
        };
        match session_service.update_session(&id, validated_updates) {
            Ok(Some(session)) => IpcResponse::success(session),
            Ok(None) => IpcResponse::not_found("Session"),
            Err(err) => ipc_session_lifecycle_error(err),
        }
    }

    pub(super) async fn handle_rename_session(
        core: &Arc<AppCore>,
        id: String,
        name: String,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.rename_session(&id, name) {
            Ok(Some(session)) => IpcResponse::success(session),
            Ok(None) => IpcResponse::not_found("Session"),
            Err(err) => ipc_session_lifecycle_error(err),
        }
    }

    pub(super) async fn handle_archive_session(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.archive_session(&id) {
            Ok(archived) => IpcResponse::success(ArchiveResponse { archived }),
            Err(err) => ipc_session_lifecycle_error(err),
        }
    }

    pub(super) async fn handle_delete_session(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.delete_session(&id) {
            Ok(deleted) => IpcResponse::success(DeleteResponse { deleted }),
            Err(err) => ipc_session_lifecycle_error(err),
        }
    }

    pub(super) async fn handle_search_sessions(
        core: &Arc<AppCore>,
        query: String,
        agent_id: Option<String>,
        limit: Option<usize>,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.search_session_views(
            &query,
            agent_id.as_deref(),
            None,
            false,
            limit.unwrap_or(20).max(1),
        ) {
            Ok(sessions) => {
                let matches: Vec<ChatSessionSummary> =
                    sessions.iter().map(ChatSessionSummary::from).collect();
                IpcResponse::success(matches)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_add_message(
        core: &Arc<AppCore>,
        session_id: String,
        role: ChatRole,
        content: String,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        let mut session = match session_service.get_session_view(&session_id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("Session"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let message = message_for_role(role, content);
        append_message_to_session(&core.storage, &mut session, message)
    }

    pub(super) async fn handle_append_message(
        core: &Arc<AppCore>,
        session_id: String,
        message: ChatMessage,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        let mut session = match session_service.get_session_view(&session_id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("Session"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        append_message_to_session(&core.storage, &mut session, message)
    }

    pub(super) async fn handle_execute_chat_session_stream_unsupported() -> IpcResponse {
        IpcResponse::error(-3, "Chat session streaming requires direct stream handler")
    }

    pub(super) async fn handle_steer_chat_session_stream(
        core: &Arc<AppCore>,
        session_id: String,
        instruction: String,
        scope: Option<ExecutionScope>,
    ) -> IpcResponse {
        let steered = steer_chat_stream(core, &session_id, &instruction, scope.as_ref()).await;
        IpcResponse::success(SteerResponse { steered })
    }

    pub(super) async fn handle_cancel_chat_session_stream(
        core: &Arc<AppCore>,
        stream_id: String,
    ) -> IpcResponse {
        let canceled = cancel_chat_stream(core, &stream_id).await;
        IpcResponse::success(CancelResponse { canceled })
    }

    pub(super) async fn handle_get_session_messages(
        core: &Arc<AppCore>,
        session_id: String,
        limit: Option<usize>,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        let session = match session_service.get_session_view(&session_id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("Session"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let count = limit.unwrap_or(session.messages.len());
        let messages = session
            .messages
            .iter()
            .cloned()
            .rev()
            .take(count)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        IpcResponse::success(messages)
    }

    pub(super) async fn handle_get_execution_run_timeline(
        core: &Arc<AppCore>,
        run_id: String,
    ) -> IpcResponse {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return IpcResponse::error(400, "run_id is required");
        }
        let service = ExecutionConsoleService::from_storage(&core.storage);
        match service.get_execution_run_timeline(run_id) {
            Ok(timeline) => IpcResponse::success(timeline),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_subscribe_session_events_unsupported() -> IpcResponse {
        IpcResponse::error(-3, "Session event streaming requires stream mode")
    }

    pub(super) async fn handle_list_run_artifacts(
        _core: &Arc<AppCore>,
        run_id: Option<String>,
        task_id: Option<String>,
    ) -> IpcResponse {
        if run_id.is_none() && task_id.is_none() {
            return IpcResponse::error(400, "ListRunArtifacts requires run_id or task_id");
        }
        IpcResponse::success(Vec::<types::RunArtifact>::new())
    }

    pub(super) async fn handle_switch_session_model(
        core: &Arc<AppCore>,
        session_id: String,
        model_ref: WireModelRef,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.switch_session_model(&session_id, model_ref.provider, model_ref.model)
        {
            Ok(Some(session)) => IpcResponse::success(session),
            Ok(None) => IpcResponse::not_found("session"),
            Err(error) => ipc_session_lifecycle_error(error),
        }
    }

    pub(super) async fn handle_get_system_info() -> IpcResponse {
        IpcResponse::success(serde_json::json!({
            "pid": std::process::id(),
        }))
    }

    pub(super) async fn handle_get_available_models(core: &Arc<AppCore>) -> IpcResponse {
        IpcResponse::success(available_model_catalog(core))
    }

    pub(super) async fn handle_get_available_tools(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<ai::tools::ToolRegistry>,
    ) -> IpcResponse {
        match get_runtime_tool_registry(core, runtime_tool_registry) {
            Ok(registry) => {
                let tools: Vec<String> = registry
                    .list()
                    .iter()
                    .map(|name| name.to_string())
                    .collect();
                IpcResponse::success(tools)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_available_tool_definitions(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<ai::tools::ToolRegistry>,
    ) -> IpcResponse {
        match get_runtime_tool_registry(core, runtime_tool_registry) {
            Ok(registry) => {
                let tools: Vec<ToolDefinition> = registry
                    .schemas()
                    .into_iter()
                    .map(|schema| ToolDefinition {
                        name: schema.name,
                        description: schema.description,
                        parameters: schema.parameters,
                    })
                    .collect();
                IpcResponse::success(tools)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_execute_tool(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<ai::tools::ToolRegistry>,
        name: String,
        input: serde_json::Value,
    ) -> IpcResponse {
        match get_runtime_tool_registry(core, runtime_tool_registry) {
            Ok(registry) => match registry.execute_safe(&name, input).await {
                Ok(output) => IpcResponse::success(to_tool_execution_result(output)),
                Err(err) => ipc_error_with_optional_json_details(500, err.to_string()),
            },
            Err(err) => ipc_error_with_optional_json_details(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_mcp_servers() -> IpcResponse {
        IpcResponse::success(Vec::<String>::new())
    }

    pub(super) async fn handle_build_agent_system_prompt(
        core: &Arc<AppCore>,
        agent_node: types::AgentNode,
    ) -> IpcResponse {
        match build_agent_system_prompt(core, agent_node) {
            Ok(prompt) => IpcResponse::success(PromptResponse { prompt }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_shutdown() -> IpcResponse {
        IpcResponse::success(serde_json::json!({ "shutting_down": true }))
    }

    pub(crate) async fn process(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<ai::tools::ToolRegistry>,
        request: IpcRequest,
    ) -> IpcResponse {
        match request {
            IpcRequest::Ping => Self::handle_ping().await,
            IpcRequest::GetStatus => Self::handle_get_status().await,
            IpcRequest::ListAgents => Self::handle_list_agents(core).await,
            IpcRequest::GetAgent { id } => Self::handle_get_agent(core, id).await,
            IpcRequest::CreateAgent { name, agent } => match types::AgentNode::try_from(agent) {
                Ok(agent) => Self::handle_create_agent(core, name, agent).await,
                Err(errors) => invalid_validation_response(errors),
            },
            IpcRequest::UpdateAgent { id, name, agent } => {
                let agent = match agent.map(types::AgentNode::try_from).transpose() {
                    Ok(agent) => agent,
                    Err(errors) => return invalid_validation_response(errors),
                };
                Self::handle_update_agent(core, id, name, agent).await
            }
            IpcRequest::DeleteAgent { id } => Self::handle_delete_agent(core, id).await,
            IpcRequest::ListSkills => Self::handle_list_skills(core).await,
            IpcRequest::GetSkill { id } => Self::handle_get_skill(core, id).await,
            IpcRequest::GetSkillReference { skill_id, ref_id } => {
                Self::handle_get_skill_reference(core, skill_id, ref_id).await
            }
            IpcRequest::RunCleanup => Self::handle_run_cleanup(core).await,
            IpcRequest::ListSecrets => Self::handle_list_secrets(core).await,
            IpcRequest::GetSecret { key } => Self::handle_get_secret(core, key).await,
            IpcRequest::SetSecret {
                key,
                value,
                description,
            } => Self::handle_set_secret(core, key, value, description).await,
            IpcRequest::CreateSecret {
                key,
                value,
                description,
            } => Self::handle_create_secret(core, key, value, description).await,
            IpcRequest::UpdateSecret {
                key,
                value,
                description,
            } => Self::handle_update_secret(core, key, value, description).await,
            IpcRequest::DeleteSecret { key } => Self::handle_delete_secret(core, key).await,
            IpcRequest::GetConfig => Self::handle_get_config(core).await,
            IpcRequest::GetGlobalConfig => Self::handle_get_global_config(core).await,
            IpcRequest::SetConfig { config } => match from_contract(config) {
                Ok(config) => Self::handle_set_config(core, config).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::ListSessions => Self::handle_list_sessions(core).await,
            IpcRequest::ListFullSessions => Self::handle_list_full_sessions(core).await,
            IpcRequest::ListSessionsByAgent { agent_id } => {
                Self::handle_list_sessions_by_agent(core, agent_id).await
            }
            IpcRequest::ListSessionsBySkill { skill_id } => {
                Self::handle_list_sessions_by_skill(core, skill_id).await
            }
            IpcRequest::CountSessions => Self::handle_count_sessions(core).await,
            IpcRequest::DeleteSessionsOlderThan { older_than_ms } => {
                Self::handle_delete_sessions_older_than(core, older_than_ms).await
            }
            IpcRequest::GetSession { id } => Self::handle_get_session(core, id).await,
            IpcRequest::CreateSession {
                agent_id,
                model,
                name,
                skill_id,
            } => Self::handle_create_session(core, agent_id, model, name, skill_id).await,
            IpcRequest::UpdateSession { id, updates } => match from_contract(updates) {
                Ok(updates) => Self::handle_update_session(core, id, updates).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::RenameSession { id, name } => {
                Self::handle_rename_session(core, id, name).await
            }
            IpcRequest::ArchiveSession { id } => Self::handle_archive_session(core, id).await,
            IpcRequest::DeleteSession { id } => Self::handle_delete_session(core, id).await,
            IpcRequest::SearchSessions {
                query,
                agent_id,
                limit,
            } => Self::handle_search_sessions(core, query, agent_id, limit).await,
            IpcRequest::AddMessage {
                session_id,
                role,
                content,
            } => match from_contract(role) {
                Ok(role) => Self::handle_add_message(core, session_id, role, content).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::AppendMessage {
                session_id,
                message,
            } => match from_contract(message) {
                Ok(message) => Self::handle_append_message(core, session_id, message).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::ExecuteChatSession { .. } => {
                IpcResponse::error(-3, "Foreground chat execution runs in the TUI process")
            }
            IpcRequest::ExecuteChatSessionStream { .. } => {
                Self::handle_execute_chat_session_stream_unsupported().await
            }
            IpcRequest::SteerChatSessionStream {
                session_id,
                instruction,
                scope,
            } => Self::handle_steer_chat_session_stream(core, session_id, instruction, scope).await,
            IpcRequest::CancelChatSessionStream { stream_id } => {
                Self::handle_cancel_chat_session_stream(core, stream_id).await
            }
            IpcRequest::GetSessionMessages { session_id, limit } => {
                Self::handle_get_session_messages(core, session_id, limit).await
            }
            IpcRequest::ListExecutionContainers => {
                Self::handle_list_execution_containers(core).await
            }
            IpcRequest::ListRuns { query } => match from_contract(query) {
                Ok(query) => Self::handle_list_runs(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetExecutionRunThread { run_id } => {
                Self::handle_get_execution_run_thread(core, run_id).await
            }
            IpcRequest::ListChildRuns { query } => match from_contract(query) {
                Ok(query) => Self::handle_list_child_runs(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetExecutionRunTimeline { run_id } => {
                Self::handle_get_execution_run_timeline(core, run_id).await
            }
            IpcRequest::SubscribeSessionEvents => {
                Self::handle_subscribe_session_events_unsupported().await
            }
            IpcRequest::ListRunArtifacts { run_id, task_id } => {
                Self::handle_list_run_artifacts(core, run_id, task_id).await
            }
            IpcRequest::SwitchSessionModel {
                session_id,
                model_ref,
                reason: _,
            } => Self::handle_switch_session_model(core, session_id, model_ref).await,
            IpcRequest::GetSystemInfo => Self::handle_get_system_info().await,
            IpcRequest::GetAvailableModels => Self::handle_get_available_models(core).await,
            IpcRequest::GetAvailableTools => {
                Self::handle_get_available_tools(core, runtime_tool_registry).await
            }
            IpcRequest::GetAvailableToolDefinitions => {
                Self::handle_get_available_tool_definitions(core, runtime_tool_registry).await
            }
            IpcRequest::ExecuteTool { name, input } => {
                Self::handle_execute_tool(core, runtime_tool_registry, name, input).await
            }
            IpcRequest::ListMcpServers => Self::handle_list_mcp_servers().await,
            IpcRequest::BuildAgentSystemPrompt { agent_node } => {
                match types::AgentNode::try_from(agent_node) {
                    Ok(agent_node) => {
                        Self::handle_build_agent_system_prompt(core, agent_node).await
                    }
                    Err(errors) => invalid_validation_response(errors),
                }
            }
            IpcRequest::Shutdown => Self::handle_shutdown().await,
        }
    }
}
