use super::ipc_protocol::{IpcRequest, IpcResponse, MAX_MESSAGE_SIZE};
use crate::AppCore;
use crate::auth::{AuthManagerConfig, AuthProfileManager, CredentialSource};
use crate::models::chat_session::{ChatRole, ChatSession, ChatSessionSummary};
use crate::models::memory::MemorySearchQuery;
use crate::services::{
    agent as agent_service, config as config_service, secrets as secrets_service,
    skills as skills_service,
};
use anyhow::{Result, anyhow};
use restflow_storage::AuthProfileStorage;
use serde::Serialize;
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
            IpcRequest::RunTask { id: _ } => {
                IpcResponse::error(-3, "Task execution not available via IPC")
            }
            IpcRequest::StopTask { id: _ } => {
                IpcResponse::error(-3, "Task execution not available via IPC")
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
                match core.storage.agent_tasks.list_tasks_by_status(status) {
                    Ok(tasks) => IpcResponse::success(tasks),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::SearchMemory {
                query,
                agent_id,
                limit,
            } => match search_memory(core, &query, agent_id, limit) {
                Ok(results) => IpcResponse::success(results),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListMemory { agent_id, tag } => match list_memory(core, agent_id, tag) {
                Ok(chunks) => IpcResponse::success(chunks),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ClearMemory { agent_id } => match clear_memory(core, agent_id) {
                Ok(deleted) => IpcResponse::success(serde_json::json!({
                    "deleted": deleted,
                })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetMemoryStats { agent_id } => match get_memory_stats(core, agent_id) {
                Ok(stats) => IpcResponse::success(stats),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListSessions => match list_sessions(core) {
                Ok(sessions) => IpcResponse::success(sessions),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetSession { id } => match core.storage.chat_sessions.get(&id) {
                Ok(Some(session)) => IpcResponse::success(session),
                Ok(None) => IpcResponse::not_found("Session"),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::CreateSession { agent_id, model } => {
                match create_session(core, agent_id, model) {
                    Ok(session) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteSession { id } => match core.storage.chat_sessions.delete(&id) {
                Ok(deleted) => IpcResponse::success(serde_json::json!({
                    "deleted": deleted,
                    "id": id,
                })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::SearchSessions { query, agent_id } => {
                match search_sessions(core, &query, agent_id.as_deref()) {
                    Ok(results) => IpcResponse::success(results),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListAuthProfiles => match list_auth_profiles(core).await {
                Ok(profiles) => IpcResponse::success(profiles),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetAuthProfile { id } => match get_auth_profile(core, &id).await {
                Ok(profile) => IpcResponse::success(profile),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::AddAuthProfile {
                name,
                credential,
                provider,
            } => match add_auth_profile(core, name, credential, provider).await {
                Ok(profile) => IpcResponse::success(profile),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::RemoveAuthProfile { id } => match remove_auth_profile(core, &id).await {
                Ok(removed) => IpcResponse::success(removed),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::DiscoverAuth => match discover_auth(core).await {
                Ok(summary) => IpcResponse::success(summary),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ExecuteAgent { .. } => {
                IpcResponse::error(-3, "Agent execution not available via IPC")
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
            IpcRequest::Shutdown => {
                IpcResponse::success(serde_json::json!({ "shutting_down": true }))
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct SessionSearchResult {
    id: String,
    name: String,
    agent_id: String,
    model: String,
    updated_at: i64,
    match_count: usize,
    preview: Option<String>,
}

fn search_memory(
    core: &Arc<AppCore>,
    query: &str,
    agent_id: Option<String>,
    limit: Option<u32>,
) -> Result<crate::models::memory::MemorySearchResult> {
    let agent_id = agent_id.ok_or_else(|| anyhow!("Agent id is required"))?;
    let mut search = MemorySearchQuery::new(agent_id).with_query(query.to_string());
    if let Some(limit) = limit {
        search = search.paginate(limit, 0);
    }
    core.storage.memory.search(&search)
}

fn list_memory(
    core: &Arc<AppCore>,
    agent_id: Option<String>,
    tag: Option<String>,
) -> Result<Vec<crate::models::memory::MemoryChunk>> {
    match (agent_id, tag) {
        (Some(agent_id), Some(tag)) => Ok(core
            .storage
            .memory
            .list_chunks(&agent_id)?
            .into_iter()
            .filter(|chunk| chunk.tags.iter().any(|value| value == &tag))
            .collect()),
        (Some(agent_id), None) => core.storage.memory.list_chunks(&agent_id),
        (None, Some(tag)) => core.storage.memory.list_chunks_by_tag(&tag),
        (None, None) => Err(anyhow!("Agent id or tag is required")),
    }
}

fn clear_memory(core: &Arc<AppCore>, agent_id: Option<String>) -> Result<u32> {
    if let Some(agent_id) = agent_id {
        return core.storage.memory.delete_chunks_for_agent(&agent_id);
    }

    let agents = core.storage.agents.list_agents()?;
    if agents.is_empty() {
        return Err(anyhow!("No agents available"));
    }

    let mut deleted = 0u32;
    for agent in agents {
        deleted += core.storage.memory.delete_chunks_for_agent(&agent.id)?;
    }
    Ok(deleted)
}

fn get_memory_stats(
    core: &Arc<AppCore>,
    agent_id: Option<String>,
) -> Result<Vec<crate::models::memory::MemoryStats>> {
    if let Some(agent_id) = agent_id {
        return Ok(vec![core.storage.memory.get_stats(&agent_id)?]);
    }

    let agents = core.storage.agents.list_agents()?;
    if agents.is_empty() {
        return Err(anyhow!("No agents available"));
    }

    let mut stats = Vec::new();
    for agent in agents {
        stats.push(core.storage.memory.get_stats(&agent.id)?);
    }
    Ok(stats)
}

fn list_sessions(core: &Arc<AppCore>) -> Result<Vec<ChatSessionSummary>> {
    core.storage.chat_sessions.list_summaries()
}

fn create_session(
    core: &Arc<AppCore>,
    agent_id: Option<String>,
    model: Option<String>,
) -> Result<ChatSession> {
    let agent_id = agent_id.ok_or_else(|| anyhow!("Agent id is required"))?;
    let model = model.ok_or_else(|| anyhow!("Model is required"))?;
    let session = ChatSession::new(agent_id, model);
    core.storage.chat_sessions.create(&session)?;
    Ok(session)
}

fn search_sessions(
    core: &Arc<AppCore>,
    query: &str,
    agent_id: Option<&str>,
) -> Result<Vec<SessionSearchResult>> {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(anyhow!("Search query cannot be empty"));
    }

    let sessions = if let Some(agent_id) = agent_id {
        core.storage.chat_sessions.list_by_agent(agent_id)?
    } else {
        core.storage.chat_sessions.list()?
    };

    let mut results = Vec::new();
    for session in sessions {
        let (match_count, preview) = count_matches(&session, &normalized);
        if match_count > 0 {
            results.push(SessionSearchResult {
                id: session.id,
                name: session.name,
                agent_id: session.agent_id,
                model: session.model,
                updated_at: session.updated_at,
                match_count,
                preview,
            });
        }
    }

    results.sort_by(|a, b| b.match_count.cmp(&a.match_count));
    Ok(results)
}

fn count_matches(session: &ChatSession, query: &str) -> (usize, Option<String>) {
    let mut count = 0;
    let mut preview = None;

    for message in &session.messages {
        let message_text = message.content.to_lowercase();
        let matches = message_text.matches(query).count();
        if matches > 0 {
            count += matches;
            if preview.is_none() {
                let role = match message.role {
                    ChatRole::User => "User",
                    ChatRole::Assistant => "Assistant",
                    ChatRole::System => "System",
                };
                preview = Some(format!("{}: {}", role, truncate_preview(&message.content, 80)));
            }
        }
    }

    (count, preview)
}

fn truncate_preview(content: &str, limit: usize) -> String {
    if content.len() <= limit {
        return content.to_string();
    }
    let mut trimmed = content.chars().take(limit).collect::<String>();
    trimmed.push_str("...");
    trimmed
}

async fn auth_manager(core: &Arc<AppCore>) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig::default();
    let db = core.storage.get_db();
    let secrets = Arc::new(core.storage.secrets.clone());
    let profile_storage = AuthProfileStorage::new(db)?;

    let manager = AuthProfileManager::with_storage(config, secrets, Some(profile_storage));
    manager.initialize().await?;
    Ok(manager)
}

async fn list_auth_profiles(core: &Arc<AppCore>) -> Result<Vec<crate::auth::AuthProfile>> {
    let manager = auth_manager(core).await?;
    let mut profiles = manager.list_profiles().await;
    profiles.sort_by(|a, b| {
        a.provider
            .to_string()
            .cmp(&b.provider.to_string())
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(profiles)
}

async fn get_auth_profile(
    core: &Arc<AppCore>,
    id: &str,
) -> Result<crate::auth::AuthProfile> {
    let manager = auth_manager(core).await?;
    manager
        .get_profile(id)
        .await
        .ok_or_else(|| anyhow!("Profile not found: {}", id))
}

async fn add_auth_profile(
    core: &Arc<AppCore>,
    name: String,
    credential: crate::auth::Credential,
    provider: crate::auth::AuthProvider,
) -> Result<crate::auth::AuthProfile> {
    let manager = auth_manager(core).await?;
    let id = manager
        .add_profile_from_credential(name, credential, CredentialSource::Manual, provider)
        .await?;

    manager
        .get_profile(&id)
        .await
        .ok_or_else(|| anyhow!("Profile not found after creation: {}", id))
}

async fn remove_auth_profile(
    core: &Arc<AppCore>,
    id: &str,
) -> Result<crate::auth::AuthProfile> {
    let manager = auth_manager(core).await?;
    manager.remove_profile(id).await
}

async fn discover_auth(core: &Arc<AppCore>) -> Result<crate::auth::DiscoverySummary> {
    let manager = auth_manager(core).await?;
    manager.discover().await
}
