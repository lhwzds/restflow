use super::ipc_protocol::{IpcRequest, IpcResponse, MAX_MESSAGE_SIZE};
use crate::auth::{AuthManagerConfig, AuthProfileManager};
use crate::memory::MemoryExporter;
use crate::models::{
    AgentTaskStatus, ChatMessage, ChatRole, ChatSessionSummary, MemoryChunk, MemorySearchQuery,
};
use crate::services::{
    agent as agent_service, config as config_service, secrets as secrets_service,
    skills as skills_service,
};
use crate::AppCore;
use anyhow::Result;
use restflow_storage::AuthProfileStorage;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info};

pub struct IpcServer {
    core: Arc<AppCore>,
    socket_path: PathBuf,
}

impl IpcServer {
    pub fn new(core: Arc<AppCore>, socket_path: PathBuf) -> Self {
        Self { core, socket_path }
    }

    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }
        let listener = UnixListener::bind(&self.socket_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.socket_path, std::fs::Permissions::from_mode(0o600))?;
        }

        info!(path = %self.socket_path.display(), "IPC server started");

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let core = self.core.clone();
                            tokio::spawn(async move {
                                if let Err(err) = Self::handle_client(stream, core).await {
                                    debug!(error = %err, "Client disconnected");
                                }
                            });
                        }
                        Err(err) => error!(error = %err, "IPC accept error"),
                    }
                }
                _ = shutdown.recv() => {
                    info!("IPC server shutting down");
                    break;
                }
            }
        }

        let _ = std::fs::remove_file(&self.socket_path);
        Ok(())
    }

    async fn handle_client(mut stream: UnixStream, core: Arc<AppCore>) -> Result<()> {
        loop {
            let mut len_buf = [0u8; 4];
            if stream.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            if len > MAX_MESSAGE_SIZE {
                Self::send(&mut stream, &IpcResponse::error(-1, "Message too large")).await?;
                continue;
            }

            let mut buf = vec![0u8; len];
            stream.read_exact(&mut buf).await?;

            let response = match serde_json::from_slice::<IpcRequest>(&buf) {
                Ok(req) => Self::process(&core, req).await,
                Err(err) => IpcResponse::error(-2, format!("Invalid request: {}", err)),
            };

            Self::send(&mut stream, &response).await?;
        }
        Ok(())
    }

    async fn send(stream: &mut UnixStream, response: &IpcResponse) -> Result<()> {
        let json = serde_json::to_vec(response)?;
        stream.write_all(&(json.len() as u32).to_le_bytes()).await?;
        stream.write_all(&json).await?;
        Ok(())
    }

    async fn process(core: &Arc<AppCore>, request: IpcRequest) -> IpcResponse {
        match request {
            IpcRequest::Ping => IpcResponse::Pong,
            IpcRequest::GetStatus => IpcResponse::success(serde_json::json!({
                "status": "running",
                "pid": std::process::id(),
                "uptime_secs": 0,
            })),
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
            IpcRequest::DeleteSkill { id } => match skills_service::delete_skill(core, &id).await {
                Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListTasks => match core.storage.agent_tasks.list_tasks() {
                Ok(tasks) => IpcResponse::success(tasks),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetTask { id } => match core.storage.agent_tasks.get_task(&id) {
                Ok(Some(task)) => IpcResponse::success(task),
                Ok(None) => IpcResponse::not_found("Task"),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::CreateTask {
                name,
                agent_id,
                schedule,
            } => {
                match core
                    .storage
                    .agent_tasks
                    .create_task(name, agent_id, schedule)
                {
                    Ok(task) => IpcResponse::success(task),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateTask { task } => {
                match core.storage.agent_tasks.update_task(&task) {
                    Ok(()) => IpcResponse::success(task),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteTask { id } => match core.storage.agent_tasks.delete_task(&id) {
                Ok(deleted) => IpcResponse::success(serde_json::json!({ "deleted": deleted })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::RunTask { id: _ } => {
                IpcResponse::error(-3, "Task execution not available via IPC")
            }
            IpcRequest::StopTask { id: _ } => {
                IpcResponse::error(-3, "Task execution not available via IPC")
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
            } => {
                let agent_id = match resolve_agent_id(core, agent_id) {
                    Ok(agent_id) => agent_id,
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                };
                let mut search = MemorySearchQuery::new(agent_id);
                if !query.is_empty() {
                    search = search.with_query(query);
                }
                if let Some(limit) = limit {
                    search = search.paginate(limit, 0);
                }
                match core.storage.memory.search(&search) {
                    Ok(result) => IpcResponse::success(result),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListMemory { agent_id, tag } => {
                let result = match (agent_id, tag) {
                    (Some(agent_id), Some(tag)) => core
                        .storage
                        .memory
                        .list_chunks(&agent_id)
                        .map(|chunks| {
                            chunks
                                .into_iter()
                                .filter(|chunk| chunk.tags.iter().any(|t| t == &tag))
                                .collect::<Vec<_>>()
                        }),
                    (Some(agent_id), None) => core.storage.memory.list_chunks(&agent_id),
                    (None, Some(tag)) => core.storage.memory.list_chunks_by_tag(&tag),
                    (None, None) => {
                        return IpcResponse::error(400, "agent_id or tag is required")
                    }
                };
                match result {
                    Ok(chunks) => IpcResponse::success(chunks),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::AddMemory {
                content,
                agent_id,
                tags,
            } => {
                let agent_id = match resolve_agent_id(core, agent_id) {
                    Ok(agent_id) => agent_id,
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                };
                let mut chunk = MemoryChunk::new(agent_id, content);
                if !tags.is_empty() {
                    chunk = chunk.with_tags(tags);
                }
                match core.storage.memory.store_chunk(&chunk) {
                    Ok(id) => IpcResponse::success(serde_json::json!({ "id": id })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteMemory { id } => match core.storage.memory.delete_chunk(&id) {
                Ok(deleted) => IpcResponse::success(serde_json::json!({ "deleted": deleted })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ClearMemory { agent_id } => {
                let agent_id = match resolve_agent_id(core, agent_id) {
                    Ok(agent_id) => agent_id,
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                };
                match core.storage.memory.delete_chunks_for_agent(&agent_id) {
                    Ok(count) => IpcResponse::success(serde_json::json!({ "deleted": count })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetMemoryStats { agent_id } => {
                let agent_id = match resolve_agent_id(core, agent_id) {
                    Ok(agent_id) => agent_id,
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                };
                match core.storage.memory.get_stats(&agent_id) {
                    Ok(stats) => IpcResponse::success(stats),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ExportMemory { agent_id } => {
                let agent_id = match resolve_agent_id(core, agent_id) {
                    Ok(agent_id) => agent_id,
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                };
                let exporter = MemoryExporter::new(core.storage.memory.clone());
                match exporter.export_agent(&agent_id) {
                    Ok(result) => IpcResponse::success(result),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListSessions => match core.storage.chat_sessions.list_summaries() {
                Ok(summaries) => IpcResponse::success(summaries),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetSession { id } => match core.storage.chat_sessions.get(&id) {
                Ok(Some(session)) => IpcResponse::success(session),
                Ok(None) => IpcResponse::not_found("Session"),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::CreateSession { agent_id, model } => {
                let agent_id = match resolve_agent_id(core, agent_id) {
                    Ok(agent_id) => agent_id,
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                };
                let model = model.unwrap_or_else(|| "default".to_string());
                let session = crate::models::ChatSession::new(agent_id, model);
                match core.storage.chat_sessions.create(&session) {
                    Ok(()) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateSession {
                session_id,
                session,
            } => {
                if session.id != session_id {
                    return IpcResponse::error(400, "Session id mismatch");
                }
                match core.storage.chat_sessions.update(&session) {
                    Ok(()) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteSession { id } => match core.storage.chat_sessions.delete(&id) {
                Ok(deleted) => IpcResponse::success(serde_json::json!({ "deleted": deleted })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::SearchSessions { query } => match core.storage.chat_sessions.list() {
                Ok(sessions) => {
                    let query = query.to_lowercase();
                    let matches: Vec<ChatSessionSummary> = sessions
                        .into_iter()
                        .filter(|session| {
                            session.name.to_lowercase().contains(&query)
                                || session.messages.iter().any(|message| {
                                    message.content.to_lowercase().contains(&query)
                                })
                        })
                        .map(|session| ChatSessionSummary::from(&session))
                        .collect();
                    IpcResponse::success(matches)
                }
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::AddMessage {
                session_id,
                role,
                content,
            } => {
                let mut session = match core.storage.chat_sessions.get(&session_id) {
                    Ok(Some(session)) => session,
                    Ok(None) => return IpcResponse::not_found("Session"),
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                let message = match role {
                    ChatRole::User => ChatMessage::user(content),
                    ChatRole::Assistant => ChatMessage::assistant(content),
                    ChatRole::System => ChatMessage::system(content),
                };
                session.add_message(message);
                match core.storage.chat_sessions.update(&session) {
                    Ok(()) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetSessionMessages { session_id, limit } => {
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
            IpcRequest::ListAuthProfiles => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                IpcResponse::success(manager.list_profiles().await)
            }
            IpcRequest::GetAuthProfile { id } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.get_profile(&id).await {
                    Some(profile) => IpcResponse::success(profile),
                    None => IpcResponse::not_found("Auth profile"),
                }
            }
            IpcRequest::AddAuthProfile {
                name,
                credential,
                source,
                provider,
            } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager
                    .add_profile_from_credential(name, credential, source, provider)
                    .await
                {
                    Ok(id) => match manager.get_profile(&id).await {
                        Some(profile) => IpcResponse::success(profile),
                        None => IpcResponse::error(500, "Profile created but not found"),
                    },
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::RemoveAuthProfile { id } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.remove_profile(&id).await {
                    Ok(profile) => IpcResponse::success(profile),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateAuthProfile { id, updates } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.update_profile(&id, updates).await {
                    Ok(profile) => IpcResponse::success(profile),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DiscoverAuth => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.discover().await {
                    Ok(summary) => IpcResponse::success(summary),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetApiKey { provider } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.get_available_profile(provider).await {
                    Some(profile) => match profile.get_api_key(manager.resolver()) {
                        Ok(key) => IpcResponse::success(serde_json::json!({
                            "profile_id": profile.id,
                            "api_key": key,
                        })),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    },
                    None => IpcResponse::not_found("Auth profile"),
                }
            }
            IpcRequest::GetApiKeyForProfile { id } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.get_profile(&id).await {
                    Some(profile) => match profile.get_api_key(manager.resolver()) {
                        Ok(key) => IpcResponse::success(serde_json::json!({
                            "profile_id": profile.id,
                            "api_key": key,
                        })),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    },
                    None => IpcResponse::not_found("Auth profile"),
                }
            }
            IpcRequest::TestAuthProfile { id } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.get_profile(&id).await {
                    Some(profile) => match profile.get_api_key(manager.resolver()) {
                        Ok(_) => IpcResponse::success(serde_json::json!({ "ok": true })),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    },
                    None => IpcResponse::not_found("Auth profile"),
                }
            }
            IpcRequest::PauseTask { id } => match core.storage.agent_tasks.pause_task(&id) {
                Ok(task) => IpcResponse::success(task),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ResumeTask { id } => match core.storage.agent_tasks.resume_task(&id) {
                Ok(task) => IpcResponse::success(task),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListTasksByStatus { status } => {
                let status = match parse_task_status(&status) {
                    Ok(status) => status,
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                };
                match core.storage.agent_tasks.list_tasks_by_status(status) {
                    Ok(tasks) => IpcResponse::success(tasks),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetTaskHistory { id } => match core.storage.agent_tasks.list_events_for_task(&id) {
                Ok(events) => IpcResponse::success(events),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::SubscribeTaskEvents { task_id: _ } => {
                IpcResponse::error(-3, "Task event streaming not available via IPC")
            }
            IpcRequest::ExecuteAgent { .. } => {
                IpcResponse::error(-3, "Agent execution not available via IPC")
            }
            IpcRequest::ExecuteAgentStream { .. } => {
                IpcResponse::error(-3, "Agent execution not available via IPC")
            }
            IpcRequest::CancelExecution { .. } => {
                IpcResponse::error(-3, "Agent execution not available via IPC")
            }
            IpcRequest::GetSystemInfo => IpcResponse::success(serde_json::json!({
                "pid": std::process::id(),
                "python_ready": core.is_python_ready(),
            })),
            IpcRequest::GetAvailableModels => IpcResponse::success(Vec::<String>::new()),
            IpcRequest::GetAvailableTools => IpcResponse::success(Vec::<String>::new()),
            IpcRequest::ListMcpServers => IpcResponse::success(Vec::<String>::new()),
            IpcRequest::Shutdown => {
                IpcResponse::success(serde_json::json!({ "shutting_down": true }))
            }
        }
    }
}

fn resolve_agent_id(core: &Arc<AppCore>, agent_id: Option<String>) -> Result<String> {
    if let Some(agent_id) = agent_id {
        return Ok(agent_id);
    }

    let agents = core.storage.agents.list_agents()?;
    let agent = agents
        .first()
        .ok_or_else(|| anyhow::anyhow!("No agents available"))?;
    Ok(agent.id.clone())
}

async fn build_auth_manager(core: &Arc<AppCore>) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig {
        auto_discover: false,
        ..AuthManagerConfig::default()
    };
    let db = core.storage.get_db();
    let secrets = Arc::new(core.storage.secrets.clone());
    let profile_storage = AuthProfileStorage::new(db)?;
    let manager = AuthProfileManager::with_storage(config, secrets, Some(profile_storage));
    manager.initialize().await?;
    Ok(manager)
}

fn parse_task_status(status: &str) -> Result<AgentTaskStatus> {
    match status.to_lowercase().as_str() {
        "active" => Ok(AgentTaskStatus::Active),
        "paused" => Ok(AgentTaskStatus::Paused),
        "running" => Ok(AgentTaskStatus::Running),
        "completed" => Ok(AgentTaskStatus::Completed),
        "failed" => Ok(AgentTaskStatus::Failed),
        _ => Err(anyhow::anyhow!("Unknown task status: {}", status)),
    }
}
