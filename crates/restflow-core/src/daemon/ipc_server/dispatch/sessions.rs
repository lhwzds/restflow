use super::super::runtime::{
    cancel_chat_stream, execute_chat_session, resolve_agent_id, steer_chat_stream,
};
use super::super::*;
use uuid::Uuid;

impl IpcServer {
    pub(super) async fn handle_list_sessions(core: &Arc<AppCore>) -> IpcResponse {
        match core.storage.chat_sessions.list() {
            Ok(mut sessions) => {
                for session in &mut sessions {
                    if let Err(err) = apply_effective_session_source(&core.storage, session) {
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
        match core.storage.chat_sessions.list() {
            Ok(mut sessions) => {
                for session in &mut sessions {
                    if let Err(err) = apply_effective_session_source(&core.storage, session) {
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
        match core.storage.chat_sessions.list_by_agent(&agent_id) {
            Ok(mut sessions) => {
                for session in &mut sessions {
                    if let Err(err) = apply_effective_session_source(&core.storage, session) {
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
        match core.storage.chat_sessions.list_by_skill(&skill_id) {
            Ok(mut sessions) => {
                for session in &mut sessions {
                    if let Err(err) = apply_effective_session_source(&core.storage, session) {
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
        match core.storage.chat_sessions.list_all() {
            Ok(sessions) => {
                let mut deleted = 0usize;
                for session in sessions
                    .into_iter()
                    .filter(|session| session.updated_at < older_than_ms)
                {
                    let workspace_managed =
                        match is_workspace_managed_session(&core.storage, &session) {
                            Ok(value) => value,
                            Err(error) => return IpcResponse::error(500, error.to_string()),
                        };
                    if !workspace_managed {
                        continue;
                    }

                    match session_service.delete_workspace_session(&session.id) {
                        Ok(true) => {
                            deleted += 1;
                        }
                        Ok(false) => {}
                        Err(error) => {
                            if let Some(lifecycle_error) =
                                error.downcast_ref::<SessionLifecycleError>()
                                && matches!(
                                    lifecycle_error,
                                    SessionLifecycleError::BoundToBackgroundTask { .. }
                                )
                            {
                                continue;
                            }
                            return IpcResponse::error(500, error.to_string());
                        }
                    }
                }
                IpcResponse::success(deleted)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_session(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match core.storage.chat_sessions.get(&id) {
            Ok(Some(mut session)) => {
                if let Err(err) = apply_effective_session_source(&core.storage, &mut session) {
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
                    .unwrap_or_else(|| AIModel::Gpt5.as_serialized_str().to_string()),
                Ok(None) => AIModel::Gpt5.as_serialized_str().to_string(),
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
        let mut session = match core.storage.chat_sessions.get(&id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("Session"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };

        let workspace_managed = match is_workspace_managed_session(&core.storage, &session) {
            Ok(value) => value,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        if !workspace_managed {
            return IpcResponse::error(
                403,
                format!(
                    "Session {} is managed by {:?} and cannot be updated from workspace",
                    session.id,
                    session_owner_for_error(&core.storage, &session),
                ),
            );
        }

        let mut updated = false;
        let mut name_updated = false;

        if let Some(agent_id) = updates.agent_id {
            let resolved_agent_id = match core.storage.agents.resolve_existing_agent_id(&agent_id) {
                Ok(resolved) => resolved,
                Err(err) => return IpcResponse::error(400, err.to_string()),
            };
            session.agent_id = resolved_agent_id;
            updated = true;
        }

        if let Some(model) = updates.model {
            let normalized = match normalize_model_input(&model) {
                Ok(normalized) => normalized,
                Err(err) => return IpcResponse::error(400, err.to_string()),
            };
            session.model = normalized;
            updated = true;
        }

        if let Some(name) = updates.name {
            session.rename(name);
            updated = true;
            name_updated = true;
        }

        if updated {
            if !name_updated {
                session.updated_at = Utc::now().timestamp_millis();
            }

            if let Err(err) = core.storage.chat_sessions.update(&session) {
                return IpcResponse::error(500, err.to_string());
            }
            publish_session_event(ChatSessionEvent::Updated {
                session_id: session.id.clone(),
            });
        }

        IpcResponse::success(session)
    }

    pub(super) async fn handle_rename_session(
        core: &Arc<AppCore>,
        id: String,
        name: String,
    ) -> IpcResponse {
        let mut session = match core.storage.chat_sessions.get(&id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("Session"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let workspace_managed = match is_workspace_managed_session(&core.storage, &session) {
            Ok(value) => value,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        if !workspace_managed {
            return IpcResponse::error(
                403,
                format!(
                    "Session {} is managed by {:?} and cannot be renamed from workspace",
                    session.id,
                    session_owner_for_error(&core.storage, &session),
                ),
            );
        }
        session.rename(name);
        match core.storage.chat_sessions.update(&session) {
            Ok(()) => {
                publish_session_event(ChatSessionEvent::Updated {
                    session_id: session.id.clone(),
                });
                IpcResponse::success(session)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_archive_session(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        let session = match core.storage.chat_sessions.get(&id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::success(serde_json::json!({ "archived": false })),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let workspace_managed = match is_workspace_managed_session(&core.storage, &session) {
            Ok(value) => value,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        if !workspace_managed {
            return IpcResponse::error(
                403,
                format!(
                    "Session {} is managed by {:?} and cannot be archived from workspace",
                    session.id,
                    session_owner_for_error(&core.storage, &session),
                ),
            );
        }

        match session_service.archive_workspace_session(&id) {
            Ok(archived) => {
                if archived {
                    publish_session_event(ChatSessionEvent::Updated {
                        session_id: id.clone(),
                    });
                }
                IpcResponse::success(serde_json::json!({ "archived": archived }))
            }
            Err(err) => ipc_session_lifecycle_error(err),
        }
    }

    pub(super) async fn handle_delete_session(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let session_service = SessionService::from_storage(&core.storage);
        let session = match core.storage.chat_sessions.get(&id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::success(serde_json::json!({ "deleted": false })),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let workspace_managed = match is_workspace_managed_session(&core.storage, &session) {
            Ok(value) => value,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        if !workspace_managed {
            return IpcResponse::error(
                403,
                format!(
                    "Session {} is managed by {:?} and cannot be deleted from workspace",
                    session.id,
                    session_owner_for_error(&core.storage, &session),
                ),
            );
        }

        match session_service.delete_workspace_session(&id) {
            Ok(deleted) => {
                if deleted {
                    publish_session_event(ChatSessionEvent::Deleted {
                        session_id: id.clone(),
                    });
                }
                IpcResponse::success(serde_json::json!({ "deleted": deleted }))
            }
            Err(err) => ipc_session_lifecycle_error(err),
        }
    }

    pub(super) async fn handle_rebuild_external_session(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        let session = match core.storage.chat_sessions.get(&id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("Session"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let (source_channel, conversation_id) =
            match resolve_external_session_route(&core.storage, &session) {
                Ok(route) => route,
                Err(err) => return IpcResponse::error(400, err.to_string()),
            };
        let rebuilt =
            match build_rebuilt_external_session(&session, source_channel, &conversation_id) {
                Ok(rebuilt) => rebuilt,
                Err(err) => return IpcResponse::error(400, err.to_string()),
            };

        if let Err(err) = core.storage.chat_sessions.create(&rebuilt) {
            return IpcResponse::error(500, err.to_string());
        }

        let deleted_old = match core.storage.chat_sessions.delete(&id) {
            Ok(deleted) => deleted,
            Err(err) => {
                let _ = core.storage.chat_sessions.delete(&rebuilt.id);
                return IpcResponse::error(500, err.to_string());
            }
        };
        if !deleted_old {
            let _ = core.storage.chat_sessions.delete(&rebuilt.id);
            return IpcResponse::not_found("Session");
        }

        if let Err(err) = rebind_external_session_routes(&core.storage, &id, &rebuilt.id) {
            let _ = core.storage.chat_sessions.delete(&rebuilt.id);
            return IpcResponse::error(500, err.to_string());
        }

        if let Err(error) = core.storage.tool_traces.delete_by_session(&id) {
            warn!(
                session_id = %id,
                error = %error,
                "Failed to clean up chat execution events after external session rebuild"
            );
        }

        publish_session_event(ChatSessionEvent::Deleted {
            session_id: id.clone(),
        });
        publish_session_event(ChatSessionEvent::Created {
            session_id: rebuilt.id.clone(),
        });

        IpcResponse::success(rebuilt)
    }

    pub(super) async fn handle_search_sessions(core: &Arc<AppCore>, query: String) -> IpcResponse {
        match core.storage.chat_sessions.list() {
            Ok(mut sessions) => {
                let query = query.to_lowercase();
                for session in &mut sessions {
                    if let Err(err) = apply_effective_session_source(&core.storage, session) {
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
            Err(err) => {
                let message = err.to_string();
                if message.contains("Session not found") {
                    IpcResponse::not_found("Session")
                } else if message.contains("No user message found") {
                    IpcResponse::error(400, message)
                } else {
                    IpcResponse::error(500, message)
                }
            }
        }
    }

    pub(super) async fn handle_steer_chat_session_stream(
        session_id: String,
        instruction: String,
    ) -> IpcResponse {
        let steered = steer_chat_stream(&session_id, &instruction).await;
        IpcResponse::success(serde_json::json!({ "steered": steered }))
    }

    pub(super) async fn handle_cancel_chat_session_stream(stream_id: String) -> IpcResponse {
        let canceled = cancel_chat_stream(&stream_id).await;
        IpcResponse::success(serde_json::json!({ "canceled": canceled }))
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

    pub(super) async fn handle_list_tool_traces(
        core: &Arc<AppCore>,
        session_id: String,
        turn_id: Option<String>,
        limit: Option<usize>,
    ) -> IpcResponse {
        let result = match turn_id {
            Some(turn_id) => {
                core.storage
                    .tool_traces
                    .list_by_session_turn(&session_id, &turn_id, limit)
            }
            None => core.storage.tool_traces.list_by_session(&session_id, limit),
        };
        match result {
            Ok(events) => IpcResponse::success(events),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
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

fn session_owner_for_error(
    storage: &crate::storage::Storage,
    session: &ChatSession,
) -> ChatSessionSource {
    session_management_owner(storage, session)
        .ok()
        .flatten()
        .or(match session.source_channel {
            Some(ChatSessionSource::Workspace) | None => None,
            Some(source) => Some(source),
        })
        .unwrap_or(ChatSessionSource::ExternalLegacy)
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
    match storage.chat_sessions.update(session) {
        Ok(()) => {
            publish_session_event(ChatSessionEvent::MessageAdded {
                session_id: session.id.clone(),
                source: "ipc".to_string(),
            });
            IpcResponse::success(session.clone())
        }
        Err(err) => IpcResponse::error(500, err.to_string()),
    }
}
