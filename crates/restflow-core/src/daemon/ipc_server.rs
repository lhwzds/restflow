use super::ipc_protocol::{
    IpcRequest, IpcResponse, MAX_MESSAGE_SIZE, ToolDefinition, ToolExecutionResult,
};
use crate::AppCore;
use crate::auth::{AuthManagerConfig, AuthProfileManager};
use crate::memory::{MemoryExporter, MemoryExporterBuilder, SearchEngineBuilder};
use crate::models::{
    AgentNode, BackgroundAgentStatus, ChatExecutionStatus, ChatMessage, ChatRole,
    ChatSessionSummary, MemoryChunk, MemorySearchQuery, MessageExecution, TerminalSession,
};
use crate::services::tool_registry::create_tool_registry;
use crate::services::{
    agent as agent_service, config as config_service, secrets as secrets_service,
    skills as skills_service,
};
use anyhow::Result;
use chrono::Utc;
use restflow_storage::AuthProfileStorage;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

pub struct IpcServer {
    core: Arc<AppCore>,
    socket_path: PathBuf,
}

impl IpcServer {
    pub fn new(core: Arc<AppCore>, socket_path: PathBuf) -> Self {
        Self { core, socket_path }
    }

    #[cfg(unix)]
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

    #[cfg(not(unix))]
    pub async fn run(&self, _shutdown: broadcast::Receiver<()>) -> Result<()> {
        anyhow::bail!("IPC is not supported on this platform")
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
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
            IpcRequest::ListBackgroundAgents { status } => {
                let result = match status {
                    Some(status) => match parse_background_agent_status(&status) {
                        Ok(status) => core.storage.agent_tasks.list_tasks_by_status(status),
                        Err(err) => return IpcResponse::error(400, err.to_string()),
                    },
                    None => core.storage.agent_tasks.list_tasks(),
                };

                match result {
                    Ok(background_agents) => IpcResponse::success(background_agents),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetBackgroundAgent { id } => match core.storage.agent_tasks.get_task(&id) {
                Ok(Some(background_agent)) => IpcResponse::success(background_agent),
                Ok(None) => IpcResponse::not_found("Background agent"),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
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
            IpcRequest::SearchMemoryRanked {
                query,
                min_score,
                scoring_preset,
            } => {
                let storage = core.storage.memory.clone();
                let mut builder = SearchEngineBuilder::new(storage);
                builder = match scoring_preset.as_deref() {
                    Some("frequency_focused") => builder.frequency_focused(),
                    Some("recency_focused") => builder.recency_focused(),
                    Some("balanced") => builder.balanced(),
                    _ => builder,
                };
                if let Some(min_score) = min_score {
                    builder = builder.min_score(min_score);
                }
                let engine = builder.build();
                match engine.search_ranked(&query) {
                    Ok(result) => IpcResponse::success(result),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetMemoryChunk { id } => match core.storage.memory.get_chunk(&id) {
                Ok(Some(chunk)) => IpcResponse::success(chunk),
                Ok(None) => IpcResponse::not_found("Memory chunk"),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListMemory { agent_id, tag } => {
                let result = match (agent_id, tag) {
                    (Some(agent_id), Some(tag)) => {
                        core.storage.memory.list_chunks(&agent_id).map(|chunks| {
                            chunks
                                .into_iter()
                                .filter(|chunk| chunk.tags.iter().any(|t| t == &tag))
                                .collect::<Vec<_>>()
                        })
                    }
                    (Some(agent_id), None) => core.storage.memory.list_chunks(&agent_id),
                    (None, Some(tag)) => core.storage.memory.list_chunks_by_tag(&tag),
                    (None, None) => return IpcResponse::error(400, "agent_id or tag is required"),
                };
                match result {
                    Ok(chunks) => IpcResponse::success(chunks),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListMemoryBySession { session_id } => {
                match core.storage.memory.list_chunks_for_session(&session_id) {
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
            IpcRequest::CreateMemoryChunk { chunk } => {
                match core.storage.memory.store_chunk(&chunk) {
                    Ok(id) => {
                        if id != chunk.id {
                            match core.storage.memory.get_chunk(&id) {
                                Ok(Some(existing)) => IpcResponse::success(existing),
                                Ok(None) => IpcResponse::error(500, "Stored chunk not found"),
                                Err(err) => IpcResponse::error(500, err.to_string()),
                            }
                        } else {
                            IpcResponse::success(chunk)
                        }
                    }
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
            IpcRequest::ExportMemorySession { session_id } => {
                let exporter = MemoryExporter::new(core.storage.memory.clone());
                match exporter.export_session(&session_id) {
                    Ok(result) => IpcResponse::success(result),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
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
                let storage = core.storage.memory.clone();
                let mut builder = MemoryExporterBuilder::new(storage);

                builder = match preset.as_deref() {
                    Some("minimal") => builder.minimal(),
                    Some("compact") => builder.compact(),
                    _ => builder,
                };

                if let Some(v) = include_metadata {
                    builder = builder.include_metadata(v);
                }
                if let Some(v) = include_timestamps {
                    builder = builder.include_timestamps(v);
                }
                if let Some(v) = include_source {
                    builder = builder.include_source(v);
                }
                if let Some(v) = include_tags {
                    builder = builder.include_tags(v);
                }

                let exporter = builder.build();
                let result = if let Some(session_id) = session_id {
                    exporter.export_session(&session_id)
                } else {
                    exporter.export_agent(&agent_id)
                };
                match result {
                    Ok(result) => IpcResponse::success(result),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetMemorySession { session_id } => {
                match core.storage.memory.get_session(&session_id) {
                    Ok(Some(session)) => IpcResponse::success(session),
                    Ok(None) => IpcResponse::not_found("Memory session"),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListMemorySessions { agent_id } => {
                match core.storage.memory.list_sessions(&agent_id) {
                    Ok(sessions) => IpcResponse::success(sessions),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::CreateMemorySession { session } => {
                match core.storage.memory.create_session(&session) {
                    Ok(_) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteMemorySession {
                session_id,
                delete_chunks,
            } => match core
                .storage
                .memory
                .delete_session(&session_id, delete_chunks)
            {
                Ok(deleted) => IpcResponse::success(serde_json::json!({ "deleted": deleted })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListSessions => match core.storage.chat_sessions.list_summaries() {
                Ok(summaries) => IpcResponse::success(summaries),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListFullSessions => match core.storage.chat_sessions.list() {
                Ok(sessions) => IpcResponse::success(sessions),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListSessionsByAgent { agent_id } => {
                match core.storage.chat_sessions.list_by_agent(&agent_id) {
                    Ok(sessions) => IpcResponse::success(sessions),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListSessionsBySkill { skill_id } => {
                match core.storage.chat_sessions.list_by_skill(&skill_id) {
                    Ok(sessions) => IpcResponse::success(sessions),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::CountSessions => match core.storage.chat_sessions.count() {
                Ok(count) => IpcResponse::success(count),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::DeleteSessionsOlderThan { older_than_ms } => {
                match core.storage.chat_sessions.delete_older_than(older_than_ms) {
                    Ok(deleted) => IpcResponse::success(deleted),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetSession { id } => match core.storage.chat_sessions.get(&id) {
                Ok(Some(session)) => IpcResponse::success(session),
                Ok(None) => IpcResponse::not_found("Session"),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::CreateSession {
                agent_id,
                model,
                name,
                skill_id,
            } => {
                let agent_id = match resolve_agent_id(core, agent_id) {
                    Ok(agent_id) => agent_id,
                    Err(err) => return IpcResponse::error(400, err.to_string()),
                };
                let model = model.unwrap_or_else(|| "default".to_string());
                let mut session = crate::models::ChatSession::new(agent_id, model);
                if let Some(name) = name {
                    session = session.with_name(name);
                }
                if let Some(skill_id) = skill_id {
                    session = session.with_skill(skill_id);
                }
                match core.storage.chat_sessions.create(&session) {
                    Ok(()) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateSession { id, updates } => {
                let mut session = match core.storage.chat_sessions.get(&id) {
                    Ok(Some(session)) => session,
                    Ok(None) => return IpcResponse::not_found("Session"),
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };

                let mut updated = false;
                let mut name_updated = false;

                if let Some(agent_id) = updates.agent_id {
                    session.agent_id = agent_id;
                    updated = true;
                }

                if let Some(model) = updates.model {
                    session.model = model;
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
                }

                IpcResponse::success(session)
            }
            IpcRequest::RenameSession { id, name } => {
                let mut session = match core.storage.chat_sessions.get(&id) {
                    Ok(Some(session)) => session,
                    Ok(None) => return IpcResponse::not_found("Session"),
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                session.rename(name);
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
                session.add_message(message);
                if session.name == "New Chat" && session.messages.len() == 1 {
                    session.auto_name_from_first_message();
                }
                match core.storage.chat_sessions.update(&session) {
                    Ok(()) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::AppendMessage {
                session_id,
                message,
            } => {
                let mut session = match core.storage.chat_sessions.get(&session_id) {
                    Ok(Some(session)) => session,
                    Ok(None) => return IpcResponse::not_found("Session"),
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                let mut message = message;
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
                session.add_message(message);
                if session.name == "New Chat" && session.messages.len() == 1 {
                    session.auto_name_from_first_message();
                }
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
            IpcRequest::ListTerminalSessions => match core.storage.terminal_sessions.list() {
                Ok(sessions) => IpcResponse::success(sessions),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetTerminalSession { id } => {
                match core.storage.terminal_sessions.get(&id) {
                    Ok(Some(session)) => IpcResponse::success(session),
                    Ok(None) => IpcResponse::not_found("Terminal session"),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::CreateTerminalSession => {
                let name = match core.storage.terminal_sessions.get_next_name() {
                    Ok(name) => name,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                let id = format!("terminal-{}", Uuid::new_v4());
                let session = TerminalSession::new(id, name);
                match core.storage.terminal_sessions.create(&session) {
                    Ok(()) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::RenameTerminalSession { id, name } => {
                let mut session = match core.storage.terminal_sessions.get(&id) {
                    Ok(Some(session)) => session,
                    Ok(None) => return IpcResponse::not_found("Terminal session"),
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                session.rename(name);
                match core.storage.terminal_sessions.update(&id, &session) {
                    Ok(()) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateTerminalSession {
                id,
                name,
                working_directory,
                startup_command,
            } => {
                let mut session = match core.storage.terminal_sessions.get(&id) {
                    Ok(Some(session)) => session,
                    Ok(None) => return IpcResponse::not_found("Terminal session"),
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                if let Some(name) = name {
                    session.rename(name);
                }
                session.set_config(working_directory, startup_command);
                match core.storage.terminal_sessions.update(&id, &session) {
                    Ok(()) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::SaveTerminalSession { session } => {
                match core.storage.terminal_sessions.update(&session.id, &session) {
                    Ok(()) => IpcResponse::success(session),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteTerminalSession { id } => {
                match core.storage.terminal_sessions.delete(&id) {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::MarkAllTerminalSessionsStopped => {
                match core.storage.terminal_sessions.mark_all_stopped() {
                    Ok(count) => IpcResponse::success(count),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
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
            IpcRequest::EnableAuthProfile { id } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.enable_profile(&id).await {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DisableAuthProfile { id, reason } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.disable_profile(&id, &reason).await {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
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
            IpcRequest::MarkAuthSuccess { id } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.mark_success(&id).await {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::MarkAuthFailure { id } => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                match manager.mark_failure(&id).await {
                    Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ClearAuthProfiles => {
                let manager = match build_auth_manager(core).await {
                    Ok(manager) => manager,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                manager.clear().await;
                IpcResponse::success(serde_json::json!({ "ok": true }))
            }
            IpcRequest::GetBackgroundAgentHistory { id } => {
                match core.storage.agent_tasks.list_events_for_task(&id) {
                    Ok(events) => IpcResponse::success(events),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::CreateBackgroundAgent { spec } => {
                match core.storage.agent_tasks.create_background_agent(spec) {
                    Ok(task) => IpcResponse::success(task),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateBackgroundAgent { id, patch } => {
                match core.storage.agent_tasks.update_background_agent(&id, patch) {
                    Ok(task) => IpcResponse::success(task),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteBackgroundAgent { id } => {
                match core.storage.agent_tasks.delete_task(&id) {
                    Ok(deleted) => {
                        IpcResponse::success(serde_json::json!({ "deleted": deleted, "id": id }))
                    }
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ControlBackgroundAgent { id, action } => {
                match core
                    .storage
                    .agent_tasks
                    .control_background_agent(&id, action)
                {
                    Ok(task) => IpcResponse::success(task),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetBackgroundAgentProgress { id, event_limit } => {
                match core
                    .storage
                    .agent_tasks
                    .get_background_agent_progress(&id, event_limit.unwrap_or(10))
                {
                    Ok(progress) => IpcResponse::success(progress),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::SendBackgroundAgentMessage {
                id,
                message,
                source,
            } => match core.storage.agent_tasks.send_background_agent_message(
                &id,
                message,
                source.unwrap_or(crate::models::BackgroundMessageSource::User),
            ) {
                Ok(msg) => IpcResponse::success(msg),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListBackgroundAgentMessages { id, limit } => {
                match core
                    .storage
                    .agent_tasks
                    .list_background_agent_messages(&id, limit.unwrap_or(50).max(1))
                {
                    Ok(messages) => IpcResponse::success(messages),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::SubscribeBackgroundAgentEvents {
                background_agent_id: _,
            } => IpcResponse::error(-3, "Background agent event streaming not available via IPC"),
            IpcRequest::GetSystemInfo => IpcResponse::success(serde_json::json!({
                "pid": std::process::id(),
                "python_ready": core.is_python_ready(),
            })),
            IpcRequest::InitPython => match core.get_python_manager().await {
                Ok(_) => IpcResponse::success(serde_json::json!({ "ready": true })),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::GetAvailableModels => IpcResponse::success(Vec::<String>::new()),
            IpcRequest::GetAvailableTools => {
                let registry = create_runtime_tool_registry(core);
                let tools: Vec<String> = registry
                    .list()
                    .iter()
                    .map(|name| name.to_string())
                    .collect();
                IpcResponse::success(tools)
            }
            IpcRequest::GetAvailableToolDefinitions => {
                let registry = create_runtime_tool_registry(core);
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
            IpcRequest::ExecuteTool { name, input } => {
                let registry = create_runtime_tool_registry(core);
                match registry.execute_safe(&name, input).await {
                    Ok(output) => IpcResponse::success(ToolExecutionResult {
                        success: output.success,
                        result: output.result,
                        error: output.error,
                    }),
                    Err(err) => IpcResponse::error(500, err.to_string()),
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

fn create_runtime_tool_registry(core: &Arc<AppCore>) -> restflow_ai::tools::ToolRegistry {
    create_tool_registry(
        core.storage.skills.clone(),
        core.storage.memory.clone(),
        core.storage.chat_sessions.clone(),
        core.storage.shared_space.clone(),
        core.storage.secrets.clone(),
        core.storage.config.clone(),
        core.storage.agents.clone(),
        core.storage.agent_tasks.clone(),
        core.storage.triggers.clone(),
        core.storage.terminal_sessions.clone(),
        None,
        None,
    )
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

fn parse_background_agent_status(status: &str) -> Result<BackgroundAgentStatus> {
    match status.to_lowercase().as_str() {
        "active" => Ok(BackgroundAgentStatus::Active),
        "paused" => Ok(BackgroundAgentStatus::Paused),
        "running" => Ok(BackgroundAgentStatus::Running),
        "completed" => Ok(BackgroundAgentStatus::Completed),
        "failed" => Ok(BackgroundAgentStatus::Failed),
        _ => Err(anyhow::anyhow!("Unknown task status: {}", status)),
    }
}

fn build_agent_system_prompt(core: &Arc<AppCore>, agent_node: AgentNode) -> Result<String> {
    let base = agent_node
        .prompt
        .clone()
        .unwrap_or_else(|| "You are a helpful AI assistant.".to_string());
    let skill_ids = agent_node.skills.unwrap_or_default();
    let skill_vars = agent_node.skill_variables.unwrap_or_default();

    if skill_ids.is_empty() {
        return Ok(base);
    }

    let mut skills = Vec::new();
    for id in skill_ids {
        match core.storage.skills.get(&id)? {
            Some(skill) => {
                let mut content = skill.content.clone();
                if !skill_vars.is_empty() {
                    for (name, value) in &skill_vars {
                        let pattern = format!("{{{{{}}}}}", name);
                        content = content.replace(&pattern, value);
                    }
                }
                skills.push((skill.name.clone(), content));
            }
            None => {
                warn!(skill_id = %id, "Skill not found while building system prompt");
            }
        }
    }

    if skills.is_empty() {
        return Ok(base);
    }

    let mut prompt = base;
    prompt.push_str("\n\n---\n\n# Available Skills\n\n");
    for (name, content) in skills {
        prompt.push_str(&format!("## Skill: {}\n\n{}\n\n", name, content));
    }
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentNode, Skill};
    use tempfile::tempdir;

    #[tokio::test]
    async fn build_agent_system_prompt_injects_skills() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("ipc-server-test.db");
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());

        let skill = Skill::new(
            "skill-1".to_string(),
            "Test Skill".to_string(),
            None,
            None,
            "Hello {{name}}".to_string(),
        );
        core.storage.skills.create(&skill).unwrap();

        let mut variables = std::collections::HashMap::new();
        variables.insert("name".to_string(), "World".to_string());

        let agent_node = AgentNode::new()
            .with_prompt("Base prompt")
            .with_skills(vec![skill.id.clone()])
            .with_skill_variables(variables);

        let prompt = build_agent_system_prompt(&core, agent_node).unwrap();
        assert!(prompt.contains("Base prompt"));
        assert!(prompt.contains("## Skill: Test Skill"));
        assert!(prompt.contains("Hello World"));
    }
}
