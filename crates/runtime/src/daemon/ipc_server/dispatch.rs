#[path = "dispatch/agents.rs"]
mod agents;
#[path = "dispatch/config.rs"]
mod config;
#[path = "dispatch/execution.rs"]
mod execution;
#[path = "dispatch/maintenance.rs"]
mod maintenance;
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
#[path = "dispatch/tasks.rs"]
mod tasks;

use super::*;
use crate::boundary::task::{
    contract_convert_request_to_store, contract_patch_to_core, contract_spec_to_core,
};
use crate::daemon::request_mapper::{
    from_contract, invalid_request_response, invalid_validation_response,
};

impl IpcServer {
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
            IpcRequest::CreateAgent { name, agent } => {
                match crate::models::AgentNode::try_from(agent) {
                    Ok(agent) => Self::handle_create_agent(core, name, agent).await,
                    Err(errors) => invalid_validation_response(errors),
                }
            }
            IpcRequest::UpdateAgent { id, name, agent } => {
                let agent = match agent.map(crate::models::AgentNode::try_from).transpose() {
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
            IpcRequest::ListTasks { status } => Self::handle_list_tasks(core, status).await,
            IpcRequest::ListRunnableTasks { current_time } => {
                Self::handle_list_runnable_tasks(core, current_time).await
            }
            IpcRequest::GetTask { id } => Self::handle_get_task(core, id).await,
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
            IpcRequest::GetTaskHistory { id } => Self::handle_get_task_history(core, id).await,
            IpcRequest::CreateTask { spec } => match contract_spec_to_core(spec) {
                Ok(spec) => Self::handle_create_task(core, spec).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::CreateTaskFromSession { request } => {
                match contract_convert_request_to_store(request) {
                    Ok(request) => Self::handle_create_task_from_session(core, request).await,
                    Err(err) => invalid_request_response(err),
                }
            }
            IpcRequest::UpdateTask { id, patch } => match contract_patch_to_core(patch) {
                Ok(patch) => Self::handle_update_task(core, id, patch).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::DeleteTask { id, approval_id } => {
                Self::handle_delete_task(core, id, approval_id).await
            }
            IpcRequest::ControlTask {
                id,
                action,
                approval_id,
            } => match from_contract(action) {
                Ok(action) => Self::handle_control_task(core, id, action, approval_id).await,
                Err(err) => invalid_request_response(err),
            },
            IpcRequest::GetTaskProgress { id, event_limit } => {
                Self::handle_get_task_progress(core, id, event_limit).await
            }
            IpcRequest::SendTaskMessage {
                id,
                message,
                source,
            } => {
                let source = match source.map(from_contract).transpose() {
                    Ok(source) => source,
                    Err(err) => return invalid_request_response(err),
                };
                Self::handle_send_task_message(core, id, message, source).await
            }
            IpcRequest::HandleTaskApproval { id, approved } => {
                Self::handle_task_approval(core, id, approved).await
            }
            IpcRequest::ListTaskMessages { id, limit } => {
                Self::handle_list_task_messages(core, id, limit).await
            }
            IpcRequest::SubscribeTaskEvents { .. } => {
                Self::handle_subscribe_task_events_unsupported().await
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
