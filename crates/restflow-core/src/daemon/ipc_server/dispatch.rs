#[path = "dispatch/auth.rs"]
mod auth;
#[path = "dispatch/background_agents.rs"]
mod background_agents;
#[path = "dispatch/memory.rs"]
mod memory;
#[path = "dispatch/sessions.rs"]
mod sessions;
#[path = "dispatch/terminals.rs"]
mod terminals;

use super::runtime::{build_agent_system_prompt, get_runtime_tool_registry, sample_hook_context};
use super::*;

impl IpcServer {
    pub(super) async fn process(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
        request: IpcRequest,
    ) -> IpcResponse {
        match request {
            IpcRequest::Ping => IpcResponse::Pong,
            IpcRequest::GetStatus => IpcResponse::success(build_daemon_status()),
            IpcRequest::ListAgents => match agent_service::list_agents(core).await {
                Ok(agents) => IpcResponse::success(agents),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetAgent { id } => match agent_service::get_agent(core, &id).await {
                Ok(agent) => IpcResponse::success(agent),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::CreateAgent { name, agent } => {
                match agent_service::create_agent(core, name, agent).await {
                    Ok(agent) => IpcResponse::success(agent),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateAgent { id, name, agent } => {
                match agent_service::update_agent(core, &id, name, agent).await {
                    Ok(agent) => IpcResponse::success(agent),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteAgent { id } => match agent_service::delete_agent(core, &id).await {
                Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListSkills => match skills_service::list_skills(core).await {
                Ok(skills) => IpcResponse::success(skills),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetSkill { id } => match skills_service::get_skill(core, &id).await {
                Ok(Some(skill)) => IpcResponse::success(skill),
                Ok(None) => IpcResponse::not_found("Skill"),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::CreateSkill { skill } => {
                match skills_service::create_skill(core, skill).await {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateSkill { id, skill } => {
                match skills_service::update_skill(core, &id, &skill).await {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetSkillReference { skill_id, ref_id } => {
                match skills_service::get_skill_reference(core, &skill_id, &ref_id).await {
                    Ok(Some(content)) => IpcResponse::success(content),
                    Ok(None) => IpcResponse::not_found("Skill reference"),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteSkill { id } => match skills_service::delete_skill(core, &id).await {
                Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListWorkItems { query } => {
                match core.storage.work_items.list_notes(query) {
                    Ok(items) => IpcResponse::success(items),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListWorkItemFolders => match core.storage.work_items.list_folders() {
                Ok(folders) => IpcResponse::success(folders),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetWorkItem { id } => match core.storage.work_items.get_note(&id) {
                Ok(Some(item)) => IpcResponse::success(item),
                Ok(None) => IpcResponse::not_found("Work item"),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::CreateWorkItem { spec } => {
                match core.storage.work_items.create_note(spec) {
                    Ok(item) => IpcResponse::success(item),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateWorkItem { id, patch } => {
                match core.storage.work_items.update_note(&id, patch) {
                    Ok(item) => IpcResponse::success(item),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteWorkItem { id } => match core.storage.work_items.delete_note(&id) {
                Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListBackgroundAgents { status } => {
                Self::handle_list_background_agents(core, status).await
            }
            IpcRequest::ListRunnableBackgroundAgents { current_time } => {
                Self::handle_list_runnable_background_agents(core, current_time).await
            }
            IpcRequest::GetBackgroundAgent { id } => {
                Self::handle_get_background_agent(core, id).await
            }
            IpcRequest::ListHooks => match core.storage.hooks.list() {
                Ok(hooks) => IpcResponse::success(hooks),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::CreateHook { hook } => match core.storage.hooks.create(&hook) {
                Ok(()) => IpcResponse::success(hook),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::UpdateHook { id, hook } => match core.storage.hooks.update(&id, &hook) {
                Ok(()) => IpcResponse::success(hook),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::DeleteHook { id } => match core.storage.hooks.delete(&id) {
                Ok(deleted) => IpcResponse::success(serde_json::json!({ "deleted": deleted })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::TestHook { id } => {
                let hook = match core.storage.hooks.get(&id) {
                    Ok(Some(hook)) => hook,
                    Ok(None) => return IpcResponse::not_found("Hook"),
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                let scheduler = Arc::new(crate::hooks::BackgroundAgentHookScheduler::new(
                    core.storage.background_agents.clone(),
                ));
                let executor = crate::hooks::HookExecutor::with_storage(core.storage.hooks.clone())
                    .with_task_scheduler(scheduler);
                let context = sample_hook_context(&hook.event);
                match executor.execute_hook(&hook, &context).await {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListSecrets => match secrets_service::list_secrets(core).await {
                Ok(secrets) => IpcResponse::success(secrets),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetSecret { key } => match secrets_service::get_secret(core, &key).await {
                Ok(Some(value)) => IpcResponse::success(serde_json::json!({ "value": value })),
                Ok(None) => IpcResponse::not_found("Secret"),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::SetSecret {
                key,
                value,
                description,
            } => match secrets_service::set_secret(core, &key, &value, description).await {
                Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::CreateSecret {
                key,
                value,
                description,
            } => match secrets_service::create_secret(core, &key, &value, description).await {
                Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::UpdateSecret {
                key,
                value,
                description,
            } => match secrets_service::update_secret(core, &key, &value, description).await {
                Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::DeleteSecret { key } => {
                match secrets_service::delete_secret(core, &key).await {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetConfig => match config_service::get_config(core).await {
                Ok(config) => IpcResponse::success(config),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetGlobalConfig => match config_service::get_global_config(core).await {
                Ok(config) => IpcResponse::success(config),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::SetConfig { config } => {
                match config_service::update_config(core, config).await {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::SearchMemory {
                query,
                agent_id,
                limit,
            } => Self::handle_search_memory(core, query, agent_id, limit).await,
            IpcRequest::SearchMemoryRanked {
                query,
                min_score,
                scoring_preset,
            } => Self::handle_search_memory_ranked(core, query, min_score, scoring_preset).await,
            IpcRequest::GetMemoryChunk { id } => Self::handle_get_memory_chunk(core, id).await,
            IpcRequest::ListMemory { agent_id, tag } => {
                Self::handle_list_memory(core, agent_id, tag).await
            }
            IpcRequest::ListMemoryBySession { session_id } => {
                Self::handle_list_memory_by_session(core, session_id).await
            }
            IpcRequest::AddMemory {
                content,
                agent_id,
                tags,
            } => Self::handle_add_memory(core, content, agent_id, tags).await,
            IpcRequest::CreateMemoryChunk { chunk } => {
                Self::handle_create_memory_chunk(core, chunk).await
            }
            IpcRequest::DeleteMemory { id } => Self::handle_delete_memory(core, id).await,
            IpcRequest::ClearMemory { agent_id } => Self::handle_clear_memory(core, agent_id).await,
            IpcRequest::GetMemoryStats { agent_id } => {
                Self::handle_get_memory_stats(core, agent_id).await
            }
            IpcRequest::ExportMemory { agent_id } => {
                Self::handle_export_memory(core, agent_id).await
            }
            IpcRequest::ExportMemorySession { session_id } => {
                Self::handle_export_memory_session(core, session_id).await
            }
            IpcRequest::ExportMemoryAdvanced {
                agent_id,
                session_id,
                preset,
                include_metadata,
                include_timestamps,
                include_source,
                include_tags,
            } => {
                Self::handle_export_memory_advanced(
                    core,
                    agent_id,
                    session_id,
                    preset,
                    include_metadata,
                    include_timestamps,
                    include_source,
                    include_tags,
                )
                .await
            }
            IpcRequest::GetMemorySession { session_id } => {
                Self::handle_get_memory_session(core, session_id).await
            }
            IpcRequest::ListMemorySessions { agent_id } => {
                Self::handle_list_memory_sessions(core, agent_id).await
            }
            IpcRequest::CreateMemorySession { session } => {
                Self::handle_create_memory_session(core, session).await
            }
            IpcRequest::DeleteMemorySession {
                session_id,
                delete_chunks,
            } => Self::handle_delete_memory_session(core, session_id, delete_chunks).await,
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
            IpcRequest::UpdateSession { id, updates } => {
                Self::handle_update_session(core, id, updates).await
            }
            IpcRequest::RenameSession { id, name } => {
                Self::handle_rename_session(core, id, name).await
            }
            IpcRequest::ArchiveSession { id } => Self::handle_archive_session(core, id).await,
            IpcRequest::DeleteSession { id } => Self::handle_delete_session(core, id).await,
            IpcRequest::RebuildExternalSession { id } => {
                Self::handle_rebuild_external_session(core, id).await
            }
            IpcRequest::SearchSessions { query } => Self::handle_search_sessions(core, query).await,
            IpcRequest::AddMessage {
                session_id,
                role,
                content,
            } => Self::handle_add_message(core, session_id, role, content).await,
            IpcRequest::AppendMessage {
                session_id,
                message,
            } => Self::handle_append_message(core, session_id, message).await,
            IpcRequest::ExecuteChatSession {
                session_id,
                user_input,
            } => Self::handle_execute_chat_session(core, session_id, user_input).await,
            IpcRequest::ExecuteChatSessionStream { .. } => {
                IpcResponse::error(-3, "Chat session streaming requires direct stream handler")
            }
            IpcRequest::SteerChatSessionStream {
                session_id,
                instruction,
            } => Self::handle_steer_chat_session_stream(session_id, instruction).await,
            IpcRequest::CancelChatSessionStream { stream_id } => {
                Self::handle_cancel_chat_session_stream(stream_id).await
            }
            IpcRequest::GetSessionMessages { session_id, limit } => {
                Self::handle_get_session_messages(core, session_id, limit).await
            }
            IpcRequest::ListToolTraces {
                session_id,
                turn_id,
                limit,
            } => Self::handle_list_tool_traces(core, session_id, turn_id, limit).await,
            IpcRequest::QueryExecutionTraces { query } => {
                Self::handle_query_execution_traces(core, query).await
            }
            IpcRequest::GetExecutionTraceStats { task_id } => {
                Self::handle_get_execution_trace_stats(core, task_id).await
            }
            IpcRequest::GetExecutionTraceById { id } => {
                Self::handle_get_execution_trace_by_id(core, id).await
            }
            IpcRequest::ListTerminalSessions => Self::handle_list_terminal_sessions(core).await,
            IpcRequest::GetTerminalSession { id } => {
                Self::handle_get_terminal_session(core, id).await
            }
            IpcRequest::CreateTerminalSession => Self::handle_create_terminal_session(core).await,
            IpcRequest::RenameTerminalSession { id, name } => {
                Self::handle_rename_terminal_session(core, id, name).await
            }
            IpcRequest::UpdateTerminalSession {
                id,
                name,
                working_directory,
                startup_command,
            } => {
                Self::handle_update_terminal_session(
                    core,
                    id,
                    name,
                    working_directory,
                    startup_command,
                )
                .await
            }
            IpcRequest::SaveTerminalSession { session } => {
                Self::handle_save_terminal_session(core, session).await
            }
            IpcRequest::DeleteTerminalSession { id } => {
                Self::handle_delete_terminal_session(core, id).await
            }
            IpcRequest::MarkAllTerminalSessionsStopped => {
                Self::handle_mark_all_terminal_sessions_stopped(core).await
            }
            IpcRequest::ListAuthProfiles => Self::handle_list_auth_profiles(core).await,
            IpcRequest::GetAuthProfile { id } => Self::handle_get_auth_profile(core, id).await,
            IpcRequest::AddAuthProfile {
                name,
                credential,
                source,
                provider,
            } => Self::handle_add_auth_profile(core, name, credential, source, provider).await,
            IpcRequest::RemoveAuthProfile { id } => {
                Self::handle_remove_auth_profile(core, id).await
            }
            IpcRequest::UpdateAuthProfile { id, updates } => {
                Self::handle_update_auth_profile(core, id, updates).await
            }
            IpcRequest::DiscoverAuth => Self::handle_discover_auth(core).await,
            IpcRequest::EnableAuthProfile { id } => {
                Self::handle_enable_auth_profile(core, id).await
            }
            IpcRequest::DisableAuthProfile { id, reason } => {
                Self::handle_disable_auth_profile(core, id, reason).await
            }
            IpcRequest::GetApiKey { provider } => Self::handle_get_api_key(core, provider).await,
            IpcRequest::GetApiKeyForProfile { id } => {
                Self::handle_get_api_key_for_profile(core, id).await
            }
            IpcRequest::TestAuthProfile { id } => Self::handle_test_auth_profile(core, id).await,
            IpcRequest::MarkAuthSuccess { id } => Self::handle_mark_auth_success(core, id).await,
            IpcRequest::MarkAuthFailure { id } => Self::handle_mark_auth_failure(core, id).await,
            IpcRequest::ClearAuthProfiles => Self::handle_clear_auth_profiles(core).await,
            IpcRequest::GetBackgroundAgentHistory { id } => {
                Self::handle_get_background_agent_history(core, id).await
            }
            IpcRequest::CreateBackgroundAgent { spec } => {
                Self::handle_create_background_agent(core, spec).await
            }
            IpcRequest::UpdateBackgroundAgent { id, patch } => {
                Self::handle_update_background_agent(core, id, patch).await
            }
            IpcRequest::DeleteBackgroundAgent { id } => {
                Self::handle_delete_background_agent(core, id).await
            }
            IpcRequest::ControlBackgroundAgent { id, action } => {
                Self::handle_control_background_agent(core, id, action).await
            }
            IpcRequest::GetBackgroundAgentProgress { id, event_limit } => {
                Self::handle_get_background_agent_progress(core, id, event_limit).await
            }
            IpcRequest::SendBackgroundAgentMessage {
                id,
                message,
                source,
            } => Self::handle_send_background_agent_message(core, id, message, source).await,
            IpcRequest::HandleBackgroundAgentApproval { id, approved } => {
                Self::handle_background_agent_approval(core, id, approved).await
            }
            IpcRequest::ListBackgroundAgentMessages { id, limit } => {
                Self::handle_list_background_agent_messages(core, id, limit).await
            }
            IpcRequest::SubscribeBackgroundAgentEvents {
                background_agent_id: _,
            } => {
                // Stream requests are handled in `handle_client` before dispatching
                // into `process`, so this branch should only be reached if the
                // request is routed through the non-stream path by mistake.
                IpcResponse::error(-3, "Background agent event streaming requires stream mode")
            }
            IpcRequest::SubscribeSessionEvents => {
                IpcResponse::error(-3, "Session event streaming requires stream mode")
            }
            IpcRequest::GetSystemInfo => IpcResponse::success(serde_json::json!({
                "pid": std::process::id(),
            })),
            IpcRequest::GetAvailableModels => IpcResponse::success(Vec::<String>::new()),
            IpcRequest::GetAvailableTools => {
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
            IpcRequest::GetAvailableToolDefinitions => {
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
            IpcRequest::ExecuteTool { name, input } => {
                match get_runtime_tool_registry(core, runtime_tool_registry) {
                    Ok(registry) => match registry.execute_safe(&name, input).await {
                        Ok(output) => IpcResponse::success(ToolExecutionResult {
                            success: output.success,
                            result: output.result,
                            error: output.error,
                            error_category: output.error_category,
                            retryable: output.retryable,
                            retry_after_ms: output.retry_after_ms,
                        }),
                        Err(err) => ipc_error_with_optional_json_details(500, err.to_string()),
                    },
                    Err(err) => ipc_error_with_optional_json_details(500, err.to_string()),
                }
            }
            IpcRequest::ListMcpServers => IpcResponse::success(Vec::<String>::new()),
            IpcRequest::BuildAgentSystemPrompt { agent_node } => {
                match build_agent_system_prompt(core, agent_node) {
                    Ok(prompt) => IpcResponse::success(serde_json::json!({ "prompt": prompt })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::Shutdown => {
                IpcResponse::success(serde_json::json!({ "shutting_down": true }))
            }
        }
    }
}
