use super::super::runtime::{
    cancel_chat_stream, execute_chat_session, resolve_agent_id, steer_chat_stream,
};
use super::super::*;
use crate::services::execution_console::{ExecutionConsoleService, ExecutionThreadError};
use crate::telemetry::{
    get_execution_metrics, get_execution_timeline, get_provider_health, query_execution_logs,
};
use restflow_contracts::{ArchiveResponse, CancelResponse, DeleteResponse, SteerResponse};
use uuid::Uuid;

impl IpcServer {
    pub(super) async fn handle_list_execution_containers(core: &Arc<AppCore>) -> IpcResponse {
        let service = ExecutionConsoleService::from_storage(&core.storage);
        match service.list_execution_containers() {
            Ok(containers) => IpcResponse::success(containers),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_execution_sessions(
        core: &Arc<AppCore>,
        query: crate::models::ExecutionSessionListQuery,
    ) -> IpcResponse {
        let service = ExecutionConsoleService::from_storage(&core.storage);
        match service.list_execution_sessions(&query) {
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

    pub(super) async fn handle_list_child_execution_sessions(
        core: &Arc<AppCore>,
        query: crate::models::ChildExecutionSessionQuery,
    ) -> IpcResponse {
        let service = ExecutionConsoleService::from_storage(&core.storage);
        match service.list_child_execution_sessions(&query.parent_run_id) {
            Ok(sessions) => IpcResponse::success(sessions),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_sessions(core: &Arc<AppCore>) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match core.storage.chat_sessions.list() {
            Ok(mut sessions) => {
                for session in &mut sessions {
                    if let Err(err) = session_service.apply_effective_source(session) {
                        return IpcResponse::error(500, err.to_string());
                    }
                }
                let summaries = sessions
                    .iter()
                    .map(ChatSessionSummary::from)
                    .collect::<Vec<_>>();
                IpcResponse::success(summaries)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_full_sessions(core: &Arc<AppCore>) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match core.storage.chat_sessions.list() {
            Ok(mut sessions) => {
                for session in &mut sessions {
                    if let Err(err) = session_service.apply_effective_source(session) {
                        return IpcResponse::error(500, err.to_string());
                    }
                }
                IpcResponse::success(sessions)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_sessions_by_agent(
        core: &Arc<AppCore>,
        agent_id: String,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match core.storage.chat_sessions.list_by_agent(&agent_id) {
            Ok(mut sessions) => {
                for session in &mut sessions {
                    if let Err(err) = session_service.apply_effective_source(session) {
                        return IpcResponse::error(500, err.to_string());
                    }
                }
                IpcResponse::success(sessions)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_sessions_by_skill(
        core: &Arc<AppCore>,
        skill_id: String,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match core.storage.chat_sessions.list_by_skill(&skill_id) {
            Ok(mut sessions) => {
                for session in &mut sessions {
                    if let Err(err) = session_service.apply_effective_source(session) {
                        return IpcResponse::error(500, err.to_string());
                    }
                }
                IpcResponse::success(sessions)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_count_sessions(core: &Arc<AppCore>) -> IpcResponse {
        match core.storage.chat_sessions.count() {
            Ok(count) => IpcResponse::success(count),
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
        match core.storage.chat_sessions.get(&id) {
            Ok(Some(mut session)) => {
                if let Err(err) = session_service.apply_effective_source(&mut session) {
                    return IpcResponse::error(500, err.to_string());
                }
                IpcResponse::success(session)
            }
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
                    .model
                    .map(|m| m.as_serialized_str().to_string())
                    .unwrap_or_else(|| ModelId::Gpt5.as_serialized_str().to_string()),
                Ok(None) => ModelId::Gpt5.as_serialized_str().to_string(),
                Err(err) => return IpcResponse::error(500, err.to_string()),
            },
        };
        let mut session = crate::models::ChatSession::new(agent_id, model);
        session.source_channel = Some(ChatSessionSource::Workspace);
        if let Some(name) = name {
            session = session.with_name(name);
        }
        if let Some(skill_id) = skill_id {
            session = session.with_skill(skill_id);
        }
        match core.storage.chat_sessions.create(&session) {
            Ok(()) => {
                publish_session_event(ChatSessionEvent::Created {
                    session_id: session.id.clone(),
                });
                IpcResponse::success(session)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_session(
        core: &Arc<AppCore>,
        id: String,
        updates: crate::models::ChatSessionUpdate,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        let validated_updates = crate::models::ChatSessionUpdate {
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

    pub(super) async fn handle_rebuild_external_session(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match session_service.rebuild_external_session(&id) {
            Ok(Some(rebuilt)) => IpcResponse::success(rebuilt),
            Ok(None) => IpcResponse::not_found("Session"),
            Err(err) => ipc_session_lifecycle_error(err),
        }
    }

    pub(super) async fn handle_search_sessions(core: &Arc<AppCore>, query: String) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        match core.storage.chat_sessions.list() {
            Ok(mut sessions) => {
                let query = query.to_lowercase();
                for session in &mut sessions {
                    if let Err(err) = session_service.apply_effective_source(session) {
                        return IpcResponse::error(500, err.to_string());
                    }
                }
                let matches: Vec<ChatSessionSummary> = sessions
                    .into_iter()
                    .filter(|session| {
                        session.name.to_lowercase().contains(&query)
                            || session
                                .messages
                                .iter()
                                .any(|message| message.content.to_lowercase().contains(&query))
                    })
                    .map(|session| ChatSessionSummary::from(&session))
                    .collect();
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
        let mut session = match core.storage.chat_sessions.get(&session_id) {
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
        let mut session = match core.storage.chat_sessions.get(&session_id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("Session"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        append_message_to_session(&core.storage, &mut session, message)
    }

    pub(super) async fn handle_execute_chat_session(
        core: &Arc<AppCore>,
        session_id: String,
        user_input: Option<String>,
    ) -> IpcResponse {
        match execute_chat_session(
            core,
            session_id,
            user_input,
            Uuid::new_v4().to_string(),
            None,
            None,
            None,
        )
        .await
        {
            Ok(session) => IpcResponse::success(session),
            Err(err) => IpcResponse::error(err.status_code(), err.to_string()),
        }
    }

    pub(super) async fn handle_steer_chat_session_stream(
        session_id: String,
        instruction: String,
    ) -> IpcResponse {
        let steered = steer_chat_stream(&session_id, &instruction).await;
        IpcResponse::success(SteerResponse { steered })
    }

    pub(super) async fn handle_cancel_chat_session_stream(stream_id: String) -> IpcResponse {
        let canceled = cancel_chat_stream(&stream_id).await;
        IpcResponse::success(CancelResponse { canceled })
    }

    pub(super) async fn handle_get_session_messages(
        core: &Arc<AppCore>,
        session_id: String,
        limit: Option<usize>,
    ) -> IpcResponse {
        let session = match core.storage.chat_sessions.get(&session_id) {
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

    pub(super) async fn handle_query_execution_traces(
        core: &Arc<AppCore>,
        query: crate::models::ExecutionTraceQuery,
    ) -> IpcResponse {
        match core.storage.execution_traces.query(&query) {
            Ok(events) => IpcResponse::success(events),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_execution_timeline(
        core: &Arc<AppCore>,
        query: crate::models::ExecutionTraceQuery,
    ) -> IpcResponse {
        match get_execution_timeline(&core.storage.execution_traces, &query) {
            Ok(timeline) => IpcResponse::success(timeline),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_execution_metrics(
        core: &Arc<AppCore>,
        query: crate::models::ExecutionMetricQuery,
    ) -> IpcResponse {
        match get_execution_metrics(&core.storage.telemetry_metric_samples, &query) {
            Ok(response) => IpcResponse::success(response),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_provider_health(
        core: &Arc<AppCore>,
        query: crate::models::ProviderHealthQuery,
    ) -> IpcResponse {
        match get_provider_health(&core.storage.provider_health_snapshots, &query) {
            Ok(response) => IpcResponse::success(response),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_query_execution_logs(
        core: &Arc<AppCore>,
        query: crate::models::ExecutionLogQuery,
    ) -> IpcResponse {
        match query_execution_logs(&core.storage.structured_execution_logs, &query) {
            Ok(response) => IpcResponse::success(response),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_execution_trace_stats(
        core: &Arc<AppCore>,
        task_id: Option<String>,
    ) -> IpcResponse {
        match core.storage.execution_traces.stats(task_id.as_deref()) {
            Ok(stats) => IpcResponse::success(stats),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_execution_trace_by_id(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        match core.storage.execution_traces.get_by_id(&id) {
            Ok(Some(event)) => IpcResponse::success(event),
            Ok(None) => IpcResponse::not_found("Execution trace"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}

fn map_execution_thread_response(
    result: std::result::Result<crate::models::ExecutionThread, ExecutionThreadError>,
) -> IpcResponse {
    match result {
        Ok(thread) => IpcResponse::success(thread),
        Err(ExecutionThreadError::InvalidQuery) => {
            IpcResponse::error(400, ExecutionThreadError::InvalidQuery.to_string())
        }
        Err(
            ExecutionThreadError::SessionNotFound(_)
            | ExecutionThreadError::SessionHasNoRuns(_)
            | ExecutionThreadError::RunNotFound(_),
        ) => IpcResponse::not_found("ExecutionThread"),
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
