#[path = "dispatch/agents.rs"]
mod agents;
#[path = "dispatch/auth.rs"]
mod auth;
#[path = "dispatch/background_agents.rs"]
mod background_agents;
#[path = "dispatch/config.rs"]
mod config;
#[path = "dispatch/hooks.rs"]
mod hooks;
#[path = "dispatch/memory.rs"]
mod memory;
#[path = "dispatch/runtime_tools.rs"]
mod runtime_tools;
#[path = "dispatch/secrets.rs"]
mod secrets;
#[path = "dispatch/sessions.rs"]
mod sessions;
#[path = "dispatch/skills.rs"]
mod skills;
#[path = "dispatch/system.rs"]
mod system;
#[path = "dispatch/terminals.rs"]
mod terminals;
#[path = "dispatch/work_items.rs"]
mod work_items;

use super::*;
use crate::boundary::background_agent::{contract_patch_to_core, contract_spec_to_core};
use crate::daemon::request_mapper::{
    from_contract, invalid_request_response, invalid_validation_response,
};

impl IpcServer {
    pub(crate) async fn process(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
        request: IpcRequest,
    ) -> IpcResponse {
        match request {
            IpcRequest::Ping => Self::handle_ping().await,
            IpcRequest::GetStatus => Self::handle_get_status().await,
            IpcRequest::ListAgents => Self::handle_list_agents(core).await,
            IpcRequest::GetAgent { id } => Self::handle_get_agent(core, id).await,
            IpcRequest::CreateAgent {
                name,
                agent,
                preview,
                confirmation_token,
            } => match crate::models::AgentNode::try_from(agent) {
                Ok(agent) => {
                    Self::handle_create_agent(core, name, agent, preview, confirmation_token).await
                }
                Err(errors) => invalid_validation_response(errors),
            },
            IpcRequest::UpdateAgent {
                id,
                name,
                agent,
                preview,
                confirmation_token,
            } => {
                let agent = match agent.map(crate::models::AgentNode::try_from).transpose() {
                    Ok(agent) => agent,
                    Err(errors) => return invalid_validation_response(errors),
                };
                Self::handle_update_agent(core, id, name, agent, preview, confirmation_token).await
            }
            IpcRequest::DeleteAgent { id } => Self::handle_delete_agent(core, id).await,
            IpcRequest::ListSkills => Self::handle_list_skills(core).await,
            IpcRequest::GetSkill { id } => Self::handle_get_skill(core, id).await,
            IpcRequest::CreateSkill { skill } => match from_contract(skill) {
                Ok(skill) => Self::handle_create_skill(core, skill).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::UpdateSkill { id, skill } => match from_contract(skill) {
                Ok(skill) => Self::handle_update_skill(core, id, skill).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetSkillReference { skill_id, ref_id } => {
                Self::handle_get_skill_reference(core, skill_id, ref_id).await
            }
            IpcRequest::DeleteSkill { id } => Self::handle_delete_skill(core, id).await,
            IpcRequest::ListWorkItems { query } => match from_contract(query) {
                Ok(query) => Self::handle_list_work_items(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::ListWorkItemFolders => Self::handle_list_work_item_folders(core).await,
            IpcRequest::GetWorkItem { id } => Self::handle_get_work_item(core, id).await,
            IpcRequest::CreateWorkItem { spec } => match from_contract(spec) {
                Ok(spec) => Self::handle_create_work_item(core, spec).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::UpdateWorkItem { id, patch } => match from_contract(patch) {
                Ok(patch) => Self::handle_update_work_item(core, id, patch).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::DeleteWorkItem { id } => Self::handle_delete_work_item(core, id).await,
            IpcRequest::ListBackgroundAgents { status } => {
                Self::handle_list_background_agents(core, status).await
            }
            IpcRequest::ListRunnableBackgroundAgents { current_time } => {
                Self::handle_list_runnable_background_agents(core, current_time).await
            }
            IpcRequest::GetBackgroundAgent { id } => {
                Self::handle_get_background_agent(core, id).await
            }
            IpcRequest::ListHooks => Self::handle_list_hooks(core).await,
            IpcRequest::CreateHook { hook } => match from_contract(hook) {
                Ok(hook) => Self::handle_create_hook(core, hook).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::UpdateHook { id, hook } => match from_contract(hook) {
                Ok(hook) => Self::handle_update_hook(core, id, hook).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::DeleteHook { id } => Self::handle_delete_hook(core, id).await,
            IpcRequest::TestHook { id } => Self::handle_test_hook(core, id).await,
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
            IpcRequest::SearchMemory {
                query,
                agent_id,
                limit,
            } => Self::handle_search_memory(core, query, agent_id, limit).await,
            IpcRequest::SearchMemoryRanked {
                query,
                min_score,
                scoring_preset,
            } => match from_contract(query) {
                Ok(query) => {
                    Self::handle_search_memory_ranked(core, query, min_score, scoring_preset).await
                }
                Err(err) => invalid_request_response(err),
            },
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
            IpcRequest::CreateMemoryChunk { chunk } => match from_contract(chunk) {
                Ok(chunk) => Self::handle_create_memory_chunk(core, chunk).await,
                Err(err) => invalid_request_response(err),
            },
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
            IpcRequest::CreateMemorySession { session } => match from_contract(session) {
                Ok(session) => Self::handle_create_memory_session(core, session).await,
                Err(err) => invalid_request_response(err),
            },
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
            IpcRequest::UpdateSession { id, updates } => match from_contract(updates) {
                Ok(updates) => Self::handle_update_session(core, id, updates).await,
                Err(err) => invalid_request_response(err),
            },
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
            IpcRequest::ExecuteChatSession {
                session_id,
                user_input,
            } => Self::handle_execute_chat_session(core, session_id, user_input).await,
            IpcRequest::ExecuteChatSessionStream { .. } => {
                Self::handle_execute_chat_session_stream_unsupported().await
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
            IpcRequest::ListExecutionContainers => {
                Self::handle_list_execution_containers(core).await
            }
            IpcRequest::ListExecutionSessions { query } => match from_contract(query) {
                Ok(query) => Self::handle_list_execution_sessions(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetExecutionRunThread { run_id } => {
                Self::handle_get_execution_run_thread(core, run_id).await
            }
            IpcRequest::ListChildExecutionSessions { query } => match from_contract(query) {
                Ok(query) => Self::handle_list_child_execution_sessions(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::QueryExecutionTraces { query } => match from_contract(query) {
                Ok(query) => Self::handle_query_execution_traces(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetExecutionTimeline { query } => match from_contract(query) {
                Ok(query) => Self::handle_get_execution_timeline(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetExecutionMetrics { query } => match from_contract(query) {
                Ok(query) => Self::handle_get_execution_metrics(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetProviderHealth { query } => match from_contract(query) {
                Ok(query) => Self::handle_get_provider_health(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::QueryExecutionLogs { query } => match from_contract(query) {
                Ok(query) => Self::handle_query_execution_logs(core, query).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetExecutionTraceStats { run_id } => {
                Self::handle_get_execution_trace_stats(core, run_id).await
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
            IpcRequest::SaveTerminalSession { session } => match from_contract(session) {
                Ok(session) => Self::handle_save_terminal_session(core, session).await,
                Err(err) => invalid_request_response(err),
            },
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
            } => {
                let credential = match from_contract(credential) {
                    Ok(credential) => credential,
                    Err(err) => return invalid_request_response(err),
                };
                let source = match from_contract(source) {
                    Ok(source) => source,
                    Err(err) => return invalid_request_response(err),
                };
                let provider = match from_contract(provider) {
                    Ok(provider) => provider,
                    Err(err) => return invalid_request_response(err),
                };
                Self::handle_add_auth_profile(core, name, credential, source, provider).await
            }
            IpcRequest::RemoveAuthProfile { id } => {
                Self::handle_remove_auth_profile(core, id).await
            }
            IpcRequest::UpdateAuthProfile { id, updates } => match from_contract(updates) {
                Ok(updates) => Self::handle_update_auth_profile(core, id, updates).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::DiscoverAuth => Self::handle_discover_auth(core).await,
            IpcRequest::EnableAuthProfile { id } => {
                Self::handle_enable_auth_profile(core, id).await
            }
            IpcRequest::DisableAuthProfile { id, reason } => {
                Self::handle_disable_auth_profile(core, id, reason).await
            }
            IpcRequest::GetApiKey { provider } => match from_contract(provider) {
                Ok(provider) => Self::handle_get_api_key(core, provider).await,
                Err(err) => invalid_request_response(err),
            },
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
            IpcRequest::CreateBackgroundAgent {
                spec,
                preview,
                confirmation_token,
            } => match contract_spec_to_core(spec) {
                Ok(spec) => {
                    Self::handle_create_background_agent(core, spec, preview, confirmation_token)
                        .await
                }
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::UpdateBackgroundAgent {
                id,
                patch,
                preview,
                confirmation_token,
            } => match contract_patch_to_core(patch) {
                Ok(patch) => {
                    Self::handle_update_background_agent(
                        core,
                        id,
                        patch,
                        preview,
                        confirmation_token,
                    )
                    .await
                }
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::DeleteBackgroundAgent { id } => {
                Self::handle_delete_background_agent(core, id).await
            }
            IpcRequest::ControlBackgroundAgent {
                id,
                action,
                preview,
                confirmation_token,
            } => match from_contract(action) {
                Ok(action) => {
                    Self::handle_control_background_agent(
                        core,
                        id,
                        action,
                        preview,
                        confirmation_token,
                    )
                    .await
                }
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetBackgroundAgentProgress { id, event_limit } => {
                Self::handle_get_background_agent_progress(core, id, event_limit).await
            }
            IpcRequest::SendBackgroundAgentMessage {
                id,
                message,
                source,
            } => {
                let source = match source.map(from_contract).transpose() {
                    Ok(source) => source,
                    Err(err) => return invalid_request_response(err),
                };
                Self::handle_send_background_agent_message(core, id, message, source).await
            }
            IpcRequest::HandleBackgroundAgentApproval { id, approved } => {
                Self::handle_background_agent_approval(core, id, approved).await
            }
            IpcRequest::ListBackgroundAgentMessages { id, limit } => {
                Self::handle_list_background_agent_messages(core, id, limit).await
            }
            IpcRequest::SubscribeBackgroundAgentEvents {
                background_agent_id: _,
            } => Self::handle_subscribe_background_agent_events_unsupported().await,
            IpcRequest::SubscribeSessionEvents => {
                Self::handle_subscribe_session_events_unsupported().await
            }
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
                match crate::models::AgentNode::try_from(agent_node) {
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
