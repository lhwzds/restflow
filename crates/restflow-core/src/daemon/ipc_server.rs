use super::ipc_protocol::{
    IPC_PROTOCOL_VERSION, IpcDaemonStatus, IpcRequest, IpcResponse, MAX_MESSAGE_SIZE, StreamFrame,
    ToolDefinition, ToolExecutionResult,
};
use super::session_events::{ChatSessionEvent, publish_session_event, subscribe_session_events};
use super::subscribe_background_events;
use crate::AppCore;
use crate::auth::{AuthManagerConfig, AuthProfileManager};
use crate::memory::{MemoryExporter, MemoryExporterBuilder, SearchEngineBuilder};
use crate::models::{
    AIModel, AgentNode, BackgroundAgentStatus, ChannelSessionBinding, ChatExecutionStatus,
    ChatMessage, ChatRole, ChatSession, ChatSessionSource, ChatSessionSummary, HookContext,
    HookEvent, MemoryChunk, MemorySearchQuery, MessageExecution, SteerMessage, SteerSource,
    TerminalSession,
};
use crate::process::ProcessRegistry;
use crate::runtime::background_agent::{AgentRuntimeExecutor, SessionInputMode};
use crate::runtime::channel::{
    build_turn_persistence_payload, hydrate_voice_message_metadata,
    replace_latest_user_message_content,
};
use crate::runtime::orchestrator::{AgentOrchestratorImpl, InteractiveSessionRequest};
use crate::runtime::subagent::StorageBackedSubagentLookup;
use crate::runtime::trace::{RestflowTrace, TraceEvent, append_trace_event};
use crate::services::tool_registry::create_tool_registry;
use crate::services::{
    agent as agent_service, config as config_service, secrets as secrets_service,
    session::SessionService, session_lifecycle::SessionLifecycleError, skills as skills_service,
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use restflow_ai::agent::StreamEmitter;
use restflow_ai::agent::{SubagentConfig, SubagentTracker};
use restflow_storage::{AgentDefaults, AuthProfileStorage};
use restflow_traits::DEFAULT_CHAT_MAX_SESSION_HISTORY;
use restflow_traits::store::ReplySender;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

pub struct IpcServer {
    core: Arc<AppCore>,
    socket_path: PathBuf,
    runtime_tool_registry: Arc<OnceLock<restflow_ai::tools::ToolRegistry>>,
}

fn active_chat_streams() -> &'static Mutex<HashMap<String, JoinHandle<()>>> {
    static STREAMS: OnceLock<Mutex<HashMap<String, JoinHandle<()>>>> = OnceLock::new();
    STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn active_chat_stream_sessions() -> &'static Mutex<HashMap<String, String>> {
    static SESSION_STREAMS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    SESSION_STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn active_chat_stream_steers() -> &'static Mutex<HashMap<String, mpsc::Sender<SteerMessage>>> {
    static STEERS: OnceLock<Mutex<HashMap<String, mpsc::Sender<SteerMessage>>>> = OnceLock::new();
    STEERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn daemon_started_at_ms() -> i64 {
    static STARTED_AT_MS: OnceLock<i64> = OnceLock::new();
    *STARTED_AT_MS.get_or_init(|| Utc::now().timestamp_millis())
}

const UNKNOWN_TRACE_ACTOR_ID: &str = "unknown";

fn build_chat_stream_trace(
    session_id: &str,
    stream_id: &str,
    actor_id: impl Into<String>,
) -> RestflowTrace {
    RestflowTrace::new(
        stream_id.to_string(),
        session_id.to_string(),
        session_id.to_string(),
        actor_id,
    )
}

fn resolve_chat_stream_trace(core: &AppCore, session_id: &str, stream_id: &str) -> RestflowTrace {
    let actor_id = match core.storage.chat_sessions.get(session_id) {
        Ok(Some(session)) => session.agent_id,
        Ok(None) => {
            warn!(
                session_id = %session_id,
                stream_id = %stream_id,
                "Chat session missing while building stream trace; using fallback actor"
            );
            UNKNOWN_TRACE_ACTOR_ID.to_string()
        }
        Err(error) => {
            warn!(
                session_id = %session_id,
                stream_id = %stream_id,
                error = %error,
                "Failed to load chat session while building stream trace; using fallback actor"
            );
            UNKNOWN_TRACE_ACTOR_ID.to_string()
        }
    };

    build_chat_stream_trace(session_id, stream_id, actor_id)
}

fn build_daemon_status() -> IpcDaemonStatus {
    let started_at_ms = daemon_started_at_ms();
    let now_ms = Utc::now().timestamp_millis();
    let uptime_secs = ((now_ms - started_at_ms).max(0) / 1000) as u64;

    IpcDaemonStatus {
        status: "running".to_string(),
        protocol_version: IPC_PROTOCOL_VERSION.to_string(),
        daemon_version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        started_at_ms,
        uptime_secs,
    }
}

struct IpcStreamEmitter {
    tx: mpsc::UnboundedSender<StreamFrame>,
    has_text_streamed: Arc<AtomicBool>,
}

impl IpcStreamEmitter {
    fn new(tx: mpsc::UnboundedSender<StreamFrame>, has_text_streamed: Arc<AtomicBool>) -> Self {
        Self {
            tx,
            has_text_streamed,
        }
    }
}

struct SessionReplySender {
    buffered_messages: Arc<Mutex<VecDeque<String>>>,
    stream_tx: Option<mpsc::UnboundedSender<StreamFrame>>,
}

impl SessionReplySender {
    fn new(
        buffered_messages: Arc<Mutex<VecDeque<String>>>,
        stream_tx: Option<mpsc::UnboundedSender<StreamFrame>>,
    ) -> Self {
        Self {
            buffered_messages,
            stream_tx,
        }
    }
}

impl ReplySender for SessionReplySender {
    fn send(&self, message: String) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        let buffered_messages = self.buffered_messages.clone();
        let stream_tx = self.stream_tx.clone();

        Box::pin(async move {
            buffered_messages.lock().await.push_back(message.clone());

            if let Some(tx) = stream_tx {
                let _ = tx.send(StreamFrame::Ack {
                    content: message.clone(),
                });
            }

            Ok(())
        })
    }
}

fn parse_tool_arguments(arguments: &str) -> serde_json::Value {
    if arguments.trim().is_empty() {
        return serde_json::Value::Null;
    }
    match serde_json::from_str::<serde_json::Value>(arguments) {
        Ok(value) => value,
        Err(_) => serde_json::Value::String(arguments.to_string()),
    }
}

fn normalize_model_input(model: &str) -> Result<String> {
    AIModel::normalize_model_id(model)
        .ok_or_else(|| anyhow::anyhow!("Unsupported model identifier: {}", model))
}

fn parse_binding_channel_source(channel: &str) -> Option<ChatSessionSource> {
    match channel.trim().to_ascii_lowercase().as_str() {
        "telegram" => Some(ChatSessionSource::Telegram),
        "discord" => Some(ChatSessionSource::Discord),
        "slack" => Some(ChatSessionSource::Slack),
        _ => None,
    }
}

fn channel_key_from_source(source: ChatSessionSource) -> Option<&'static str> {
    match source {
        ChatSessionSource::Telegram => Some("telegram"),
        ChatSessionSource::Discord => Some("discord"),
        ChatSessionSource::Slack => Some("slack"),
        ChatSessionSource::Workspace | ChatSessionSource::ExternalLegacy => None,
    }
}

fn resolve_legacy_external_route(session: &ChatSession) -> Option<(ChatSessionSource, String)> {
    let source = match session.source_channel {
        Some(ChatSessionSource::Workspace) | None => return None,
        Some(source) => source,
    };
    let conversation_id = session
        .source_conversation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    Some((source, conversation_id))
}

fn ensure_binding_from_legacy_source(
    storage: &crate::storage::Storage,
    session: &ChatSession,
) -> Result<Option<(ChatSessionSource, String)>> {
    let Some((source, conversation_id)) = resolve_legacy_external_route(session) else {
        return Ok(None);
    };

    if let Some(channel_key) = channel_key_from_source(source) {
        let binding =
            ChannelSessionBinding::new(channel_key, None, conversation_id.clone(), &session.id);
        storage.channel_session_bindings.upsert(&binding)?;
    }

    Ok(Some((source, conversation_id)))
}

fn apply_effective_session_source(
    storage: &crate::storage::Storage,
    session: &mut ChatSession,
) -> Result<()> {
    let bindings = storage
        .channel_session_bindings
        .list_by_session(&session.id)?;
    if let Some(binding) = bindings.first() {
        let effective_source = parse_binding_channel_source(&binding.channel)
            .unwrap_or(ChatSessionSource::ExternalLegacy);
        session.source_channel = Some(effective_source);
        session.source_conversation_id = Some(binding.conversation_id.clone());
        return Ok(());
    }

    if let Some((source, conversation_id)) = ensure_binding_from_legacy_source(storage, session)? {
        session.source_channel = Some(source);
        session.source_conversation_id = Some(conversation_id);
        return Ok(());
    }

    session.source_channel = Some(ChatSessionSource::Workspace);
    session.source_conversation_id = None;
    Ok(())
}

fn session_management_owner(
    storage: &crate::storage::Storage,
    session: &ChatSession,
) -> Result<Option<ChatSessionSource>> {
    SessionService::from_storage(storage).management_owner(session)
}

fn is_workspace_managed_session(
    storage: &crate::storage::Storage,
    session: &ChatSession,
) -> Result<bool> {
    SessionService::from_storage(storage).is_workspace_managed(session)
}

fn resolve_external_session_route(
    storage: &crate::storage::Storage,
    source: &ChatSession,
) -> Result<(ChatSessionSource, String)> {
    let bindings = storage
        .channel_session_bindings
        .list_by_session(&source.id)?;
    if let Some(binding) = bindings.first() {
        let source_channel = parse_binding_channel_source(&binding.channel)
            .unwrap_or(ChatSessionSource::ExternalLegacy);
        return Ok((source_channel, binding.conversation_id.trim().to_string()));
    }
    ensure_binding_from_legacy_source(storage, source)?
        .ok_or_else(|| anyhow::anyhow!("Session is not externally managed"))
}

fn build_rebuilt_external_session(
    source: &ChatSession,
    source_channel: ChatSessionSource,
    conversation_id: &str,
) -> Result<ChatSession> {
    let conversation_id = conversation_id.trim();
    if conversation_id.is_empty() {
        anyhow::bail!("External session is missing conversation_id");
    }
    if source_channel == ChatSessionSource::Workspace {
        anyhow::bail!("Session is not externally managed");
    }

    let mut rebuilt = ChatSession::new(source.agent_id.clone(), source.model.clone())
        .with_name(format!("channel:{}", conversation_id))
        .with_source(source_channel, conversation_id.to_string());

    if let Some(skill_id) = source.skill_id.clone() {
        rebuilt = rebuilt.with_skill(skill_id);
    }
    if let Some(retention) = source.retention.clone() {
        rebuilt = rebuilt.with_retention(retention);
    }

    Ok(rebuilt)
}

fn rebind_external_session_routes(
    storage: &crate::storage::Storage,
    from_session_id: &str,
    to_session_id: &str,
) -> Result<()> {
    let bindings = storage
        .channel_session_bindings
        .list_by_session(from_session_id)?;
    for binding in bindings {
        let rebound = ChannelSessionBinding::new(
            binding.channel,
            binding.account_id,
            binding.conversation_id,
            to_session_id,
        );
        storage.channel_session_bindings.upsert(&rebound)?;
    }
    Ok(())
}

fn ipc_session_lifecycle_error(error: anyhow::Error) -> IpcResponse {
    if let Some(lifecycle_error) = error.downcast_ref::<SessionLifecycleError>() {
        let status_code = i32::from(lifecycle_error.status_code());
        return IpcResponse::error_with_details(
            status_code,
            lifecycle_error.to_string(),
            Some(serde_json::json!({
                "error_kind": "session_lifecycle",
                "status_code": status_code,
            })),
        );
    }
    IpcResponse::error(500, error.to_string())
}

fn ipc_error_with_optional_json_details(code: i32, message: String) -> IpcResponse {
    let details = serde_json::from_str::<serde_json::Value>(&message).ok();
    IpcResponse::error_with_details(code, message, details)
}

#[async_trait]
impl StreamEmitter for IpcStreamEmitter {
    async fn emit_text_delta(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.has_text_streamed.store(true, Ordering::Relaxed);
        let _ = self.tx.send(StreamFrame::Data {
            content: text.to_string(),
        });
    }

    async fn emit_thinking_delta(&mut self, _text: &str) {}

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        let _ = self.tx.send(StreamFrame::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: parse_tool_arguments(arguments),
        });
    }

    async fn emit_tool_call_result(&mut self, id: &str, _name: &str, result: &str, success: bool) {
        let _ = self.tx.send(StreamFrame::ToolResult {
            id: id.to_string(),
            result: result.to_string(),
            success,
        });
    }

    async fn emit_complete(&mut self) {}
}

impl IpcServer {
    pub fn new(core: Arc<AppCore>, socket_path: PathBuf) -> Self {
        Self {
            core,
            socket_path,
            runtime_tool_registry: Arc::new(OnceLock::new()),
        }
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
                            let runtime_tool_registry = self.runtime_tool_registry.clone();
                            tokio::spawn(async move {
                                if let Err(err) =
                                    Self::handle_client(stream, core, runtime_tool_registry).await
                                {
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
    async fn handle_client(
        mut stream: UnixStream,
        core: Arc<AppCore>,
        runtime_tool_registry: Arc<OnceLock<restflow_ai::tools::ToolRegistry>>,
    ) -> Result<()> {
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

            match serde_json::from_slice::<IpcRequest>(&buf) {
                Ok(IpcRequest::ExecuteChatSessionStream {
                    session_id,
                    user_input,
                    stream_id,
                }) => {
                    if let Err(err) = Self::handle_execute_chat_session_stream(
                        &mut stream,
                        core.clone(),
                        session_id,
                        user_input,
                        stream_id,
                    )
                    .await
                    {
                        let frame = StreamFrame::Error {
                            code: 500,
                            message: err.to_string(),
                        };
                        let _ = Self::send_stream_frame(&mut stream, &frame).await;
                    }
                }
                Ok(IpcRequest::SubscribeBackgroundAgentEvents {
                    background_agent_id,
                }) => {
                    if let Err(err) = Self::handle_subscribe_background_agent_events(
                        &mut stream,
                        background_agent_id,
                    )
                    .await
                    {
                        let frame = StreamFrame::Error {
                            code: 500,
                            message: err.to_string(),
                        };
                        let _ = Self::send_stream_frame(&mut stream, &frame).await;
                    }
                }
                Ok(IpcRequest::SubscribeSessionEvents) => {
                    if let Err(err) = Self::handle_subscribe_session_events(&mut stream).await {
                        let frame = StreamFrame::Error {
                            code: 500,
                            message: err.to_string(),
                        };
                        let _ = Self::send_stream_frame(&mut stream, &frame).await;
                    }
                }
                Ok(req) => {
                    let response = Self::process(&core, runtime_tool_registry.as_ref(), req).await;
                    Self::send(&mut stream, &response).await?;
                }
                Err(err) => {
                    let response = IpcResponse::error(-2, format!("Invalid request: {}", err));
                    Self::send(&mut stream, &response).await?;
                }
            }
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

    #[cfg(unix)]
    async fn send_stream_frame(stream: &mut UnixStream, frame: &StreamFrame) -> Result<()> {
        let json = serde_json::to_vec(frame)?;
        stream.write_all(&(json.len() as u32).to_le_bytes()).await?;
        stream.write_all(&json).await?;
        Ok(())
    }

    #[cfg(unix)]
    async fn handle_execute_chat_session_stream(
        stream: &mut UnixStream,
        core: Arc<AppCore>,
        session_id: String,
        user_input: Option<String>,
        stream_id: String,
    ) -> Result<()> {
        let stream_id = if stream_id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            stream_id
        };

        // Abort an existing stream with the same ID to avoid duplicate workers.
        if let Some(existing) = active_chat_streams().lock().await.remove(&stream_id) {
            existing.abort();
            let trace = resolve_chat_stream_trace(&core, &session_id, &stream_id);
            append_trace_event(
                &core.storage.tool_traces,
                Some(&core.storage.execution_traces),
                &TraceEvent::run_interrupted(
                    trace,
                    "replaced by a newer stream with the same stream_id",
                    None,
                ),
            );
        }
        active_chat_stream_steers().lock().await.remove(&stream_id);

        // Ensure at most one active stream per session.
        if let Some(previous_stream_id) = active_chat_stream_sessions()
            .lock()
            .await
            .insert(session_id.clone(), stream_id.clone())
            && previous_stream_id != stream_id
        {
            if let Some(previous) = active_chat_streams()
                .lock()
                .await
                .remove(&previous_stream_id)
            {
                previous.abort();
                let trace = resolve_chat_stream_trace(&core, &session_id, &previous_stream_id);
                append_trace_event(
                    &core.storage.tool_traces,
                    Some(&core.storage.execution_traces),
                    &TraceEvent::run_interrupted(
                        trace,
                        "replaced by a newer stream for the same session",
                        None,
                    ),
                );
            }
            active_chat_stream_steers()
                .lock()
                .await
                .remove(&previous_stream_id);
        }

        Self::send_stream_frame(
            stream,
            &StreamFrame::Start {
                stream_id: stream_id.clone(),
            },
        )
        .await?;

        let (tx, mut rx) = mpsc::unbounded_channel::<StreamFrame>();
        let (steer_tx, steer_rx) = mpsc::channel::<SteerMessage>(64);
        let worker_stream_id = stream_id.clone();
        let worker_turn_id = stream_id.clone();
        let worker_session_id = session_id.clone();
        let worker_session_registry_id = session_id.clone();
        let worker_user_input = user_input.clone();
        let worker_core = core.clone();
        let handle = tokio::spawn(async move {
            let has_text_streamed = Arc::new(AtomicBool::new(false));
            let emitter = IpcStreamEmitter::new(tx.clone(), has_text_streamed.clone());
            let result = execute_chat_session(
                &worker_core,
                worker_session_id,
                worker_user_input,
                worker_turn_id,
                Some(tx.clone()),
                Some(Box::new(emitter)),
                Some(steer_rx),
            )
            .await;

            match result {
                Ok(session) => {
                    if let Some((content, total_tokens)) = latest_assistant_payload(&session) {
                        if !has_text_streamed.load(Ordering::Relaxed) && !content.is_empty() {
                            let _ = tx.send(StreamFrame::Data { content });
                        }
                        let _ = tx.send(StreamFrame::Done { total_tokens });
                    } else {
                        let _ = tx.send(StreamFrame::Error {
                            code: 500,
                            message: "Assistant response missing after execution".to_string(),
                        });
                    }
                }
                Err(err) => {
                    let _ = tx.send(StreamFrame::Error {
                        code: 500,
                        message: err.to_string(),
                    });
                }
            }

            let mut streams = active_chat_streams().lock().await;
            streams.remove(&worker_stream_id);
            active_chat_stream_steers()
                .lock()
                .await
                .remove(&worker_stream_id);
            let mut session_streams = active_chat_stream_sessions().lock().await;
            if session_streams.get(&worker_session_registry_id) == Some(&worker_stream_id) {
                session_streams.remove(&worker_session_registry_id);
            }
        });

        active_chat_streams()
            .lock()
            .await
            .insert(stream_id.clone(), handle);
        active_chat_stream_steers()
            .lock()
            .await
            .insert(stream_id.clone(), steer_tx);

        let mut reached_terminal = false;
        while let Some(frame) = rx.recv().await {
            reached_terminal =
                matches!(frame, StreamFrame::Done { .. } | StreamFrame::Error { .. });
            Self::send_stream_frame(stream, &frame).await?;
            if reached_terminal {
                break;
            }
        }

        if !reached_terminal {
            // Worker stopped unexpectedly (usually interrupted).
            let trace = resolve_chat_stream_trace(&core, &session_id, &stream_id);
            append_trace_event(
                &core.storage.tool_traces,
                Some(&core.storage.execution_traces),
                &TraceEvent::run_interrupted(trace, "chat stream interrupted", None),
            );
            let _ = Self::send_stream_frame(
                stream,
                &StreamFrame::Error {
                    code: 499,
                    message: "Chat stream interrupted".to_string(),
                },
            )
            .await;
        }

        if let Some(handle) = active_chat_streams().lock().await.remove(&stream_id)
            && !handle.is_finished()
        {
            handle.abort();
        }
        active_chat_stream_steers().lock().await.remove(&stream_id);
        let mut session_streams = active_chat_stream_sessions().lock().await;
        if session_streams.get(&session_id) == Some(&stream_id) {
            session_streams.remove(&session_id);
        }

        Ok(())
    }

    #[cfg(unix)]
    async fn handle_subscribe_background_agent_events(
        stream: &mut UnixStream,
        background_agent_id: String,
    ) -> Result<()> {
        let stream_id = format!("background-agent-{}", Uuid::new_v4());
        Self::send_stream_frame(
            stream,
            &StreamFrame::Start {
                stream_id: stream_id.clone(),
            },
        )
        .await?;

        let include_all = background_agent_id.trim().is_empty() || background_agent_id == "*";
        let mut receiver = subscribe_background_events();

        loop {
            let event = match receiver.recv().await {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(
                        skipped,
                        background_agent_id = %background_agent_id,
                        "Background event stream lagged; dropping oldest events"
                    );
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    let _ = Self::send_stream_frame(
                        stream,
                        &StreamFrame::Error {
                            code: 500,
                            message: "Background event stream closed".to_string(),
                        },
                    )
                    .await;
                    break;
                }
            };

            if !include_all && event.task_id != background_agent_id {
                continue;
            }

            let frame = StreamFrame::BackgroundAgentEvent { event };
            if let Err(err) = Self::send_stream_frame(stream, &frame).await {
                debug!(error = %err, "Background event subscriber disconnected");
                break;
            }
        }

        debug!(stream_id = %stream_id, "Background event subscription ended");
        Ok(())
    }

    #[cfg(unix)]
    async fn handle_subscribe_session_events(stream: &mut UnixStream) -> Result<()> {
        let stream_id = format!("session-events-{}", Uuid::new_v4());
        Self::send_stream_frame(
            stream,
            &StreamFrame::Start {
                stream_id: stream_id.clone(),
            },
        )
        .await?;

        let mut receiver = subscribe_session_events();

        loop {
            let event = match receiver.recv().await {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(
                        skipped,
                        "Session event stream lagged; dropping oldest events"
                    );
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    let _ = Self::send_stream_frame(
                        stream,
                        &StreamFrame::Error {
                            code: 500,
                            message: "Session event stream closed".to_string(),
                        },
                    )
                    .await;
                    break;
                }
            };

            let frame = StreamFrame::SessionEvent { event };
            if let Err(err) = Self::send_stream_frame(stream, &frame).await {
                debug!(error = %err, "Session event subscriber disconnected");
                break;
            }
        }

        debug!(stream_id = %stream_id, "Session event subscription ended");
        Ok(())
    }

    async fn process(
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
                let result = match status {
                    Some(status) => match parse_background_agent_status(&status) {
                        Ok(status) => core.storage.background_agents.list_tasks_by_status(status),
                        Err(err) => return IpcResponse::error(400, err.to_string()),
                    },
                    None => core.storage.background_agents.list_tasks(),
                };

                match result {
                    Ok(background_agents) => IpcResponse::success(background_agents),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListRunnableBackgroundAgents { current_time } => {
                let now = current_time.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                match core.storage.background_agents.list_runnable_tasks(now) {
                    Ok(background_agents) => IpcResponse::success(background_agents),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetBackgroundAgent { id } => {
                let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id)
                {
                    Ok(id) => id,
                    Err(_) => return IpcResponse::not_found("Background agent"),
                };
                match core.storage.background_agents.get_task(&resolved_id) {
                    Ok(Some(background_agent)) => IpcResponse::success(background_agent),
                    Ok(None) => IpcResponse::not_found("Background agent"),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
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
            IpcRequest::ListSessions => match core.storage.chat_sessions.list() {
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
            },
            IpcRequest::ListFullSessions => match core.storage.chat_sessions.list() {
                Ok(mut sessions) => {
                    for session in &mut sessions {
                        if let Err(err) = apply_effective_session_source(&core.storage, session) {
                            return IpcResponse::error(500, err.to_string());
                        }
                    }
                    IpcResponse::success(sessions)
                }
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::ListSessionsByAgent { agent_id } => {
                match core.storage.chat_sessions.list_by_agent(&agent_id) {
                    Ok(mut sessions) => {
                        for session in &mut sessions {
                            if let Err(err) = apply_effective_session_source(&core.storage, session)
                            {
                                return IpcResponse::error(500, err.to_string());
                            }
                        }
                        IpcResponse::success(sessions)
                    }
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListSessionsBySkill { skill_id } => {
                match core.storage.chat_sessions.list_by_skill(&skill_id) {
                    Ok(mut sessions) => {
                        for session in &mut sessions {
                            if let Err(err) = apply_effective_session_source(&core.storage, session)
                            {
                                return IpcResponse::error(500, err.to_string());
                            }
                        }
                        IpcResponse::success(sessions)
                    }
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::CountSessions => match core.storage.chat_sessions.count() {
                Ok(count) => IpcResponse::success(count),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            IpcRequest::DeleteSessionsOlderThan { older_than_ms } => {
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
                                    Err(error) => {
                                        return IpcResponse::error(500, error.to_string());
                                    }
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
            IpcRequest::GetSession { id } => match core.storage.chat_sessions.get(&id) {
                Ok(Some(mut session)) => {
                    if let Err(err) = apply_effective_session_source(&core.storage, &mut session) {
                        return IpcResponse::error(500, err.to_string());
                    }
                    IpcResponse::success(session)
                }
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
            IpcRequest::UpdateSession { id, updates } => {
                let mut session = match core.storage.chat_sessions.get(&id) {
                    Ok(Some(session)) => session,
                    Ok(None) => return IpcResponse::not_found("Session"),
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };

                let workspace_managed = match is_workspace_managed_session(&core.storage, &session)
                {
                    Ok(value) => value,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                if !workspace_managed {
                    let owner = session_management_owner(&core.storage, &session)
                        .ok()
                        .flatten()
                        .or(match session.source_channel {
                            Some(ChatSessionSource::Workspace) | None => None,
                            Some(source) => Some(source),
                        })
                        .unwrap_or(ChatSessionSource::ExternalLegacy);
                    return IpcResponse::error(
                        403,
                        format!(
                            "Session {} is managed by {:?} and cannot be updated from workspace",
                            session.id, owner,
                        ),
                    );
                }

                let mut updated = false;
                let mut name_updated = false;

                if let Some(agent_id) = updates.agent_id {
                    let resolved_agent_id =
                        match core.storage.agents.resolve_existing_agent_id(&agent_id) {
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
            IpcRequest::RenameSession { id, name } => {
                let mut session = match core.storage.chat_sessions.get(&id) {
                    Ok(Some(session)) => session,
                    Ok(None) => return IpcResponse::not_found("Session"),
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                let workspace_managed = match is_workspace_managed_session(&core.storage, &session)
                {
                    Ok(value) => value,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                if !workspace_managed {
                    let owner = session_management_owner(&core.storage, &session)
                        .ok()
                        .flatten()
                        .or(match session.source_channel {
                            Some(ChatSessionSource::Workspace) | None => None,
                            Some(source) => Some(source),
                        })
                        .unwrap_or(ChatSessionSource::ExternalLegacy);
                    return IpcResponse::error(
                        403,
                        format!(
                            "Session {} is managed by {:?} and cannot be renamed from workspace",
                            session.id, owner,
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
            IpcRequest::ArchiveSession { id } => {
                let session_service = SessionService::from_storage(&core.storage);
                let session = match core.storage.chat_sessions.get(&id) {
                    Ok(Some(session)) => session,
                    Ok(None) => {
                        return IpcResponse::success(serde_json::json!({ "archived": false }));
                    }
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                let workspace_managed = match is_workspace_managed_session(&core.storage, &session)
                {
                    Ok(value) => value,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                if !workspace_managed {
                    let owner = session_management_owner(&core.storage, &session)
                        .ok()
                        .flatten()
                        .or(match session.source_channel {
                            Some(ChatSessionSource::Workspace) | None => None,
                            Some(source) => Some(source),
                        })
                        .unwrap_or(ChatSessionSource::ExternalLegacy);
                    return IpcResponse::error(
                        403,
                        format!(
                            "Session {} is managed by {:?} and cannot be archived from workspace",
                            session.id, owner,
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
            IpcRequest::DeleteSession { id } => {
                let session_service = SessionService::from_storage(&core.storage);
                let session = match core.storage.chat_sessions.get(&id) {
                    Ok(Some(session)) => session,
                    Ok(None) => {
                        return IpcResponse::success(serde_json::json!({ "deleted": false }));
                    }
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                let workspace_managed = match is_workspace_managed_session(&core.storage, &session)
                {
                    Ok(value) => value,
                    Err(err) => return IpcResponse::error(500, err.to_string()),
                };
                if !workspace_managed {
                    let owner = session_management_owner(&core.storage, &session)
                        .ok()
                        .flatten()
                        .or(match session.source_channel {
                            Some(ChatSessionSource::Workspace) | None => None,
                            Some(source) => Some(source),
                        })
                        .unwrap_or(ChatSessionSource::ExternalLegacy);
                    return IpcResponse::error(
                        403,
                        format!(
                            "Session {} is managed by {:?} and cannot be deleted from workspace",
                            session.id, owner,
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
            IpcRequest::RebuildExternalSession { id } => {
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
                let rebuilt = match build_rebuilt_external_session(
                    &session,
                    source_channel,
                    &conversation_id,
                ) {
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
            IpcRequest::SearchSessions { query } => match core.storage.chat_sessions.list() {
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
                hydrate_voice_message_metadata(&mut message);
                session.add_message(message);
                if session.name == "New Chat" && session.messages.len() == 1 {
                    session.auto_name_from_first_message();
                }
                match core.storage.chat_sessions.update(&session) {
                    Ok(()) => {
                        publish_session_event(ChatSessionEvent::MessageAdded {
                            session_id: session.id.clone(),
                            source: "ipc".to_string(),
                        });
                        IpcResponse::success(session)
                    }
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
                hydrate_voice_message_metadata(&mut message);
                session.add_message(message);
                if session.name == "New Chat" && session.messages.len() == 1 {
                    session.auto_name_from_first_message();
                }
                match core.storage.chat_sessions.update(&session) {
                    Ok(()) => {
                        publish_session_event(ChatSessionEvent::MessageAdded {
                            session_id: session.id.clone(),
                            source: "ipc".to_string(),
                        });
                        IpcResponse::success(session)
                    }
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ExecuteChatSession {
                session_id,
                user_input,
            } => match execute_chat_session(
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
            },
            IpcRequest::ExecuteChatSessionStream { .. } => {
                IpcResponse::error(-3, "Chat session streaming requires direct stream handler")
            }
            IpcRequest::SteerChatSessionStream {
                session_id,
                instruction,
            } => {
                let steered = steer_chat_stream(&session_id, &instruction).await;
                IpcResponse::success(serde_json::json!({ "steered": steered }))
            }
            IpcRequest::CancelChatSessionStream { stream_id } => {
                let canceled = cancel_chat_stream(&stream_id).await;
                IpcResponse::success(serde_json::json!({ "canceled": canceled }))
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
            IpcRequest::ListToolTraces {
                session_id,
                turn_id,
                limit,
            } => {
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
            IpcRequest::QueryExecutionTraces { query } => {
                match core.storage.execution_traces.query(&query) {
                    Ok(events) => IpcResponse::success(events),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetExecutionTraceStats { task_id } => {
                match core.storage.execution_traces.stats(task_id.as_deref()) {
                    Ok(stats) => IpcResponse::success(stats),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetExecutionTraceById { id } => {
                match core.storage.execution_traces.get_by_id(&id) {
                    Ok(Some(event)) => IpcResponse::success(event),
                    Ok(None) => IpcResponse::not_found("Execution trace"),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
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
                match core.storage.background_agents.list_events_for_task(&id) {
                    Ok(events) => IpcResponse::success(events),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::CreateBackgroundAgent { spec } => {
                match core.storage.background_agents.create_background_agent(spec) {
                    Ok(task) => IpcResponse::success(task),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::UpdateBackgroundAgent { id, patch } => {
                match core
                    .storage
                    .background_agents
                    .update_background_agent(&id, patch)
                {
                    Ok(task) => IpcResponse::success(task),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::DeleteBackgroundAgent { id } => {
                match core.storage.background_agents.delete_task(&id) {
                    Ok(deleted) => {
                        IpcResponse::success(serde_json::json!({ "deleted": deleted, "id": id }))
                    }
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ControlBackgroundAgent { id, action } => {
                let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id)
                {
                    Ok(id) => id,
                    Err(_) => return IpcResponse::not_found("Background agent"),
                };
                match core
                    .storage
                    .background_agents
                    .control_background_agent(&resolved_id, action)
                {
                    Ok(task) => IpcResponse::success(task),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::GetBackgroundAgentProgress { id, event_limit } => {
                let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id)
                {
                    Ok(id) => id,
                    Err(_) => return IpcResponse::not_found("Background agent"),
                };
                match core
                    .storage
                    .background_agents
                    .get_background_agent_progress(&resolved_id, event_limit.unwrap_or(10))
                {
                    Ok(progress) => IpcResponse::success(progress),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::SendBackgroundAgentMessage {
                id,
                message,
                source,
            } => {
                let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id)
                {
                    Ok(id) => id,
                    Err(_) => return IpcResponse::not_found("Background agent"),
                };
                match core
                    .storage
                    .background_agents
                    .send_background_agent_message(
                        &resolved_id,
                        message,
                        source.unwrap_or(crate::models::BackgroundMessageSource::User),
                    ) {
                    Ok(msg) => IpcResponse::success(msg),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::HandleBackgroundAgentApproval { id, approved } => {
                let resolved_id = match core.storage.background_agents.resolve_existing_task_id(&id)
                {
                    Ok(id) => id,
                    Err(_) => return IpcResponse::not_found("Background agent"),
                };
                let message = if approved {
                    "User approved the pending action."
                } else {
                    "User rejected the pending action."
                };
                match core
                    .storage
                    .background_agents
                    .send_background_agent_message(
                        &resolved_id,
                        message.to_string(),
                        crate::models::BackgroundMessageSource::System,
                    ) {
                    Ok(_) => {
                        // Simplified placeholder:
                        // approval is currently injected as a system message so running
                        // background agents can continue without a dedicated approval queue.
                        // Keep handled=false to make this fallback explicit to callers.
                        IpcResponse::success(serde_json::json!({ "handled": false }))
                    }
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            IpcRequest::ListBackgroundAgentMessages { id, limit } => {
                match core
                    .storage
                    .background_agents
                    .list_background_agent_messages(&id, limit.unwrap_or(50).max(1))
                {
                    Ok(messages) => IpcResponse::success(messages),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
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

fn create_runtime_tool_registry(
    core: &Arc<AppCore>,
) -> anyhow::Result<restflow_ai::tools::ToolRegistry> {
    create_tool_registry(
        core.storage.skills.clone(),
        core.storage.memory.clone(),
        core.storage.chat_sessions.clone(),
        core.storage.channel_session_bindings.clone(),
        core.storage.tool_traces.clone(),
        core.storage.kv_store.clone(),
        core.storage.work_items.clone(),
        core.storage.secrets.clone(),
        core.storage.config.clone(),
        core.storage.agents.clone(),
        core.storage.background_agents.clone(),
        core.storage.triggers.clone(),
        core.storage.terminal_sessions.clone(),
        core.storage.deliverables.clone(),
        None,
        None,
        None,
    )
}

fn get_runtime_tool_registry<'a>(
    core: &Arc<AppCore>,
    runtime_tool_registry: &'a OnceLock<restflow_ai::tools::ToolRegistry>,
) -> Result<&'a restflow_ai::tools::ToolRegistry, String> {
    if let Some(registry) = runtime_tool_registry.get() {
        return Ok(registry);
    }

    let registry = create_runtime_tool_registry(core).map_err(|error| error.to_string())?;
    let _ = runtime_tool_registry.set(registry);
    runtime_tool_registry
        .get()
        .ok_or_else(|| "runtime tool registry initialization failed".to_string())
}

fn subagent_config_from_defaults(defaults: &AgentDefaults) -> SubagentConfig {
    SubagentConfig {
        max_parallel_agents: defaults.max_parallel_subagents,
        subagent_timeout_secs: defaults.subagent_timeout_secs,
        max_iterations: defaults.max_iterations,
        max_depth: defaults.max_depth,
    }
}

fn load_agent_defaults_from_core(core: &Arc<AppCore>) -> AgentDefaults {
    match core.storage.config.get_effective_config() {
        Ok(config) => config.agent,
        Err(error) => {
            warn!(
                error = %error,
                "Failed to load system config for chat runtime; falling back to default agent config"
            );
            AgentDefaults::default()
        }
    }
}

fn load_chat_max_session_history_from_core(core: &Arc<AppCore>) -> usize {
    match core.storage.config.get_effective_config() {
        Ok(config) => config.runtime_defaults.chat_max_session_history,
        Err(error) => {
            warn!(
                error = %error,
                "Failed to load runtime config for chat history; falling back to default history size"
            );
            DEFAULT_CHAT_MAX_SESSION_HISTORY
        }
    }
}

fn create_chat_executor(
    core: &Arc<AppCore>,
    auth_manager: Arc<AuthProfileManager>,
) -> AgentRuntimeExecutor {
    let agent_defaults = load_agent_defaults_from_core(core);
    let (completion_tx, completion_rx) = mpsc::channel(128);
    let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
    let subagent_definitions = Arc::new(StorageBackedSubagentLookup::new(
        core.storage.agents.clone(),
    ));
    let subagent_config = subagent_config_from_defaults(&agent_defaults);
    let process_registry =
        Arc::new(ProcessRegistry::new().with_ttl_seconds(agent_defaults.process_session_ttl_secs));

    AgentRuntimeExecutor::new(
        core.storage.clone(),
        process_registry,
        auth_manager,
        subagent_tracker,
        subagent_definitions,
        subagent_config,
    )
}

async fn cancel_chat_stream(stream_id: &str) -> bool {
    if let Some(handle) = active_chat_streams().lock().await.remove(stream_id) {
        handle.abort();
        active_chat_stream_steers().lock().await.remove(stream_id);
        let mut session_streams = active_chat_stream_sessions().lock().await;
        if let Some((session_id, _)) = session_streams
            .iter()
            .find(|(_, active_stream_id)| active_stream_id.as_str() == stream_id)
            .map(|(session_id, active_stream_id)| (session_id.clone(), active_stream_id.clone()))
        {
            session_streams.remove(&session_id);
        }
        true
    } else {
        false
    }
}

async fn steer_chat_stream(session_id: &str, instruction: &str) -> bool {
    let stream_id = {
        let session_streams = active_chat_stream_sessions().lock().await;
        session_streams.get(session_id).cloned()
    };

    let Some(stream_id) = stream_id else {
        return false;
    };

    let sender = {
        let steers = active_chat_stream_steers().lock().await;
        steers.get(&stream_id).cloned()
    };
    let Some(sender) = sender else {
        return false;
    };

    let steer = SteerMessage::message(instruction.to_string(), SteerSource::User);
    match sender.send(steer).await {
        Ok(()) => true,
        Err(_) => {
            active_chat_stream_steers().lock().await.remove(&stream_id);
            let mut session_streams = active_chat_stream_sessions().lock().await;
            if session_streams.get(session_id) == Some(&stream_id) {
                session_streams.remove(session_id);
            }
            false
        }
    }
}

fn latest_assistant_payload(session: &ChatSession) -> Option<(String, Option<u32>)> {
    session
        .messages
        .iter()
        .rev()
        .find(|message| message.role == ChatRole::Assistant)
        .map(|message| {
            (
                message.content.clone(),
                message.execution.as_ref().map(|exec| exec.tokens_used),
            )
        })
}

async fn execute_chat_session(
    core: &Arc<AppCore>,
    session_id: String,
    user_input: Option<String>,
    turn_id: String,
    ack_frame_tx: Option<mpsc::UnboundedSender<StreamFrame>>,
    emitter: Option<Box<dyn StreamEmitter>>,
    steer_rx: Option<mpsc::Receiver<SteerMessage>>,
) -> Result<ChatSession> {
    let mut session = core
        .storage
        .chat_sessions
        .get(&session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

    let explicit_user_input = user_input.as_deref();
    let input = match explicit_user_input {
        Some(input) if !input.trim().is_empty() => input.to_string(),
        _ => session
            .messages
            .iter()
            .rev()
            .find(|msg| msg.role == ChatRole::User)
            .map(|msg| msg.content.clone())
            .ok_or_else(|| anyhow::anyhow!("No user message found in session"))?,
    };

    persist_ipc_user_message_if_needed(core, &mut session, explicit_user_input, &input)?;

    let reply_buffer = Arc::new(Mutex::new(VecDeque::<String>::new()));
    let auth_manager = Arc::new(build_auth_manager(core).await?);
    let reply_sender = Arc::new(SessionReplySender::new(
        reply_buffer.clone(),
        ack_frame_tx.clone(),
    ));
    let executor = create_chat_executor(core, auth_manager).with_reply_sender(reply_sender);
    let chat_max_session_history = load_chat_max_session_history_from_core(core);

    match executor
        .generate_session_acknowledgement(
            &mut session,
            &input,
            SessionInputMode::PersistedInSession,
        )
        .await
    {
        Ok(Some(ack_content)) => {
            session.add_message(ChatMessage::assistant(&ack_content));
            match core.storage.chat_sessions.update(&session) {
                Ok(()) => {
                    publish_session_event(ChatSessionEvent::MessageAdded {
                        session_id: session.id.clone(),
                        source: "ipc".to_string(),
                    });
                    if let Some(tx) = ack_frame_tx.as_ref() {
                        let _ = tx.send(StreamFrame::Ack {
                            content: ack_content,
                        });
                    }
                }
                Err(err) => {
                    warn!(
                        session_id = %session.id,
                        error = %err,
                        "Failed to persist acknowledgement message"
                    );
                }
            }
        }
        Ok(None) => {}
        Err(err) => {
            warn!(
                session_id = %session.id,
                error = %err,
                "Failed to generate acknowledgement message"
            );
        }
    }

    let orchestrator = AgentOrchestratorImpl::from_runtime_executor(executor);
    let traced_execution = orchestrator
        .run_traced_interactive_session_turn(InteractiveSessionRequest {
            session: &mut session,
            user_input: &input,
            max_history: chat_max_session_history,
            input_mode: SessionInputMode::PersistedInSession,
            run_id: turn_id,
            tool_trace_storage: core.storage.tool_traces.clone(),
            execution_trace_storage: core.storage.execution_traces.clone(),
            timeout_secs: None,
            emitter,
            steer_rx,
        })
        .await
        .map_err(anyhow::Error::new)?;
    let trace = traced_execution.trace;
    let duration_ms = traced_execution.duration_ms;
    let exec_result = traced_execution.execution;

    let (execution, persisted_input) = build_turn_persistence_payload(
        &core.storage.tool_traces,
        &session.id,
        &trace.turn_id,
        &input,
        duration_ms,
        exec_result.iterations,
    );

    if persisted_input != input {
        replace_latest_user_message_content(&mut session, &input, &persisted_input);
    }
    let buffered_replies = {
        let mut guard = reply_buffer.lock().await;
        std::mem::take(&mut *guard)
    };
    for reply in buffered_replies {
        session.add_message(ChatMessage::assistant(&reply));
    }
    let assistant_message = ChatMessage::assistant(&exec_result.output).with_execution(execution);
    session.add_message(assistant_message);
    if let Some(normalized_model) = AIModel::normalize_model_id(&exec_result.active_model) {
        // Only update last_model metadata; preserve the user's chosen session model
        // so that switch_model calls during execution don't permanently override it.
        session.metadata.last_model = Some(normalized_model);
    }
    SessionService::from_storage(&core.storage).save_existing_session(&session, "ipc")?;
    Ok(session)
}

fn persist_ipc_user_message_if_needed(
    core: &Arc<AppCore>,
    session: &mut ChatSession,
    explicit_user_input: Option<&str>,
    effective_input: &str,
) -> Result<()> {
    let Some(raw_input) = explicit_user_input.map(str::trim) else {
        return Ok(());
    };
    if raw_input.is_empty() {
        return Ok(());
    }

    let already_persisted = session
        .messages
        .last()
        .map(|message| message.role == ChatRole::User && message.content == effective_input)
        .unwrap_or(false);
    if already_persisted {
        return Ok(());
    }

    session.add_message(ChatMessage::user(effective_input));
    if session.name == "New Chat" && session.messages.len() == 1 {
        session.auto_name_from_first_message();
    }
    core.storage.chat_sessions.update(session)?;
    publish_session_event(ChatSessionEvent::MessageAdded {
        session_id: session.id.clone(),
        source: "ipc".to_string(),
    });
    Ok(())
}

fn resolve_agent_id(core: &Arc<AppCore>, agent_id: Option<String>) -> Result<String> {
    if let Some(agent_id) = agent_id {
        return core.storage.agents.resolve_existing_agent_id(&agent_id);
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
        "interrupted" => Ok(BackgroundAgentStatus::Interrupted),
        _ => Err(anyhow::anyhow!(
            "Unknown background agent status: {}",
            status
        )),
    }
}

fn sample_hook_context(event: &HookEvent) -> HookContext {
    let now = chrono::Utc::now().timestamp_millis();
    match event {
        HookEvent::TaskFailed | HookEvent::TaskInterrupted => HookContext {
            event: event.clone(),
            task_id: "hook-test-task".to_string(),
            task_name: "hook test task".to_string(),
            agent_id: "hook-test-agent".to_string(),
            success: Some(false),
            output: None,
            error: Some("Hook test error".to_string()),
            duration_ms: Some(321),
            timestamp: now,
        },
        _ => HookContext {
            event: event.clone(),
            task_id: "hook-test-task".to_string(),
            task_name: "hook test task".to_string(),
            agent_id: "hook-test-agent".to_string(),
            success: Some(true),
            output: Some("Hook test output".to_string()),
            error: None,
            duration_ms: Some(321),
            timestamp: now,
        },
    }
}

fn build_agent_system_prompt(core: &Arc<AppCore>, agent_node: AgentNode) -> Result<String> {
    crate::runtime::agent::build_agent_system_prompt(core.storage.clone(), &agent_node, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentNode, ChannelSessionBinding, Skill};
    use restflow_traits::SteerCommand;
    use restflow_traits::store::ReplySender;
    use restflow_traits::tool::ToolErrorCategory;
    use tempfile::tempdir;
    use uuid::Uuid;

    async fn create_test_core() -> (Arc<AppCore>, tempfile::TempDir) {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("ipc-server-test.db");
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (core, temp)
    }

    #[tokio::test]
    async fn resolve_chat_stream_trace_uses_session_agent_and_run_turn_id() {
        let (core, _temp) = create_test_core().await;
        let session = ChatSession::new("agent-trace".to_string(), "gpt-5".to_string());
        core.storage.chat_sessions.create(&session).unwrap();

        let trace = resolve_chat_stream_trace(&core, &session.id, "stream-123");

        assert_eq!(trace.run_id, "stream-123");
        assert_eq!(trace.parent_run_id, None);
        assert_eq!(trace.turn_id, "run-stream-123");
        assert_eq!(trace.session_id, session.id);
        assert_eq!(trace.scope_id, session.id);
        assert_eq!(trace.actor_id, "agent-trace");
    }

    #[tokio::test]
    async fn resolve_chat_stream_trace_falls_back_when_session_is_missing() {
        let (core, _temp) = create_test_core().await;

        let trace = resolve_chat_stream_trace(&core, "missing-session", "stream-123");

        assert_eq!(trace.run_id, "stream-123");
        assert_eq!(trace.parent_run_id, None);
        assert_eq!(trace.turn_id, "run-stream-123");
        assert_eq!(trace.session_id, "missing-session");
        assert_eq!(trace.scope_id, "missing-session");
        assert_eq!(trace.actor_id, UNKNOWN_TRACE_ACTOR_ID);
    }

    #[test]
    fn subagent_config_from_defaults_maps_max_iterations() {
        let defaults = AgentDefaults {
            max_parallel_subagents: 21,
            subagent_timeout_secs: 1200,
            max_iterations: 111,
            max_depth: 4,
            ..AgentDefaults::default()
        };

        let config = subagent_config_from_defaults(&defaults);

        assert_eq!(config.max_parallel_agents, 21);
        assert_eq!(config.subagent_timeout_secs, 1200);
        assert_eq!(config.max_iterations, 111);
        assert_eq!(config.max_depth, 4);
    }

    #[tokio::test]
    async fn load_chat_max_session_history_from_core_uses_runtime_config() {
        let (core, _temp) = create_test_core().await;
        let mut config = core.storage.config.get_effective_config().unwrap();
        config.runtime_defaults.chat_max_session_history = 42;
        core.storage.config.update_config(config).unwrap();

        assert_eq!(load_chat_max_session_history_from_core(&core), 42);
    }

    #[tokio::test]
    async fn persist_ipc_user_message_if_needed_adds_missing_user_turn() {
        let (core, _temp) = create_test_core().await;
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        core.storage.chat_sessions.create(&session).unwrap();

        persist_ipc_user_message_if_needed(&core, &mut session, Some("hello"), "hello").unwrap();

        let stored = core
            .storage
            .chat_sessions
            .get(&session.id)
            .unwrap()
            .expect("session");
        assert_eq!(stored.messages.len(), 1);
        assert_eq!(stored.messages[0].role, ChatRole::User);
        assert_eq!(stored.messages[0].content, "hello");
    }

    #[tokio::test]
    async fn persist_ipc_user_message_if_needed_deduplicates_latest_user_turn() {
        let (core, _temp) = create_test_core().await;
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::user("hello"));
        core.storage.chat_sessions.create(&session).unwrap();

        persist_ipc_user_message_if_needed(&core, &mut session, Some("hello"), "hello").unwrap();

        let stored = core
            .storage
            .chat_sessions
            .get(&session.id)
            .unwrap()
            .expect("session");
        assert_eq!(stored.messages.len(), 1);
    }

    #[tokio::test]
    async fn persist_ipc_user_message_if_needed_auto_names_new_chat() {
        let (core, _temp) = create_test_core().await;
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        core.storage.chat_sessions.create(&session).unwrap();

        persist_ipc_user_message_if_needed(
            &core,
            &mut session,
            Some("hello from ipc"),
            "hello from ipc",
        )
        .unwrap();

        let stored = core
            .storage
            .chat_sessions
            .get(&session.id)
            .unwrap()
            .expect("session");
        assert_eq!(stored.name, "hello from ipc");
    }

    #[test]
    fn normalize_model_input_converts_to_serialized_form() {
        assert_eq!(
            normalize_model_input("MiniMax-M2.5").unwrap(),
            "minimax-m2-5"
        );
        assert_eq!(normalize_model_input("gpt-5.1").unwrap(), "gpt-5-1");
    }

    #[test]
    fn normalize_model_input_rejects_unknown_value() {
        assert!(normalize_model_input("not-a-real-model").is_err());
    }

    #[tokio::test]
    async fn is_workspace_managed_session_accepts_sessions_without_channel_bindings() {
        let (core, _temp) = create_test_core().await;

        let mut workspace = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        workspace.source_channel = Some(ChatSessionSource::Workspace);
        assert!(is_workspace_managed_session(&core.storage, &workspace).unwrap());

        let legacy = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        assert!(is_workspace_managed_session(&core.storage, &legacy).unwrap());
    }

    #[tokio::test]
    async fn is_workspace_managed_session_rejects_sessions_with_channel_bindings() {
        let (core, _temp) = create_test_core().await;

        let mut telegram = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        telegram.source_channel = Some(ChatSessionSource::Telegram);
        core.storage.chat_sessions.create(&telegram).unwrap();
        core.storage
            .channel_session_bindings
            .upsert(&ChannelSessionBinding::new(
                "telegram",
                None,
                "chat-123",
                &telegram.id,
            ))
            .unwrap();

        assert!(!is_workspace_managed_session(&core.storage, &telegram).unwrap());
    }

    #[tokio::test]
    async fn is_workspace_managed_session_rejects_legacy_external_and_backfills_binding() {
        let (core, _temp) = create_test_core().await;

        let telegram = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Telegram, "chat-legacy");
        core.storage.chat_sessions.create(&telegram).unwrap();

        assert!(!is_workspace_managed_session(&core.storage, &telegram).unwrap());

        let binding = core
            .storage
            .channel_session_bindings
            .get_by_route("telegram", None, "chat-legacy")
            .unwrap()
            .expect("legacy external route should be backfilled");
        assert_eq!(binding.session_id, telegram.id);
    }

    #[tokio::test]
    async fn delete_session_rejects_background_bound_workspace_session() {
        let (core, _temp) = create_test_core().await;
        let runtime_tool_registry = OnceLock::new();
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.source_channel = Some(ChatSessionSource::Workspace);
        core.storage.chat_sessions.create(&session).unwrap();

        core.storage
            .background_agents
            .create_background_agent(crate::models::BackgroundAgentSpec {
                name: "bound-task".to_string(),
                agent_id: "agent-1".to_string(),
                chat_session_id: Some(session.id.clone()),
                description: None,
                input: Some("run".to_string()),
                input_template: None,
                schedule: crate::models::BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let response = IpcServer::process(
            &core,
            &runtime_tool_registry,
            IpcRequest::DeleteSession {
                id: session.id.clone(),
            },
        )
        .await;
        match response {
            IpcResponse::Error { code, message, .. } => {
                assert_eq!(code, 409);
                assert!(message.contains("bound to background task"));
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn archive_session_rejects_background_bound_workspace_session() {
        let (core, _temp) = create_test_core().await;
        let runtime_tool_registry = OnceLock::new();
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.source_channel = Some(ChatSessionSource::Workspace);
        core.storage.chat_sessions.create(&session).unwrap();

        core.storage
            .background_agents
            .create_background_agent(crate::models::BackgroundAgentSpec {
                name: "bound-task".to_string(),
                agent_id: "agent-1".to_string(),
                chat_session_id: Some(session.id.clone()),
                description: None,
                input: Some("run".to_string()),
                input_template: None,
                schedule: crate::models::BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let response = IpcServer::process(
            &core,
            &runtime_tool_registry,
            IpcRequest::ArchiveSession {
                id: session.id.clone(),
            },
        )
        .await;
        match response {
            IpcResponse::Error { code, message, .. } => {
                assert_eq!(code, 409);
                assert!(message.contains("bound to background task"));
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn execute_tool_browser_session_persists_between_process_calls() {
        let (core, _temp) = create_test_core().await;
        let runtime_tool_registry = OnceLock::new();

        let create_response = IpcServer::process(
            &core,
            &runtime_tool_registry,
            IpcRequest::ExecuteTool {
                name: "browser".to_string(),
                input: serde_json::json!({
                    "action": "new_session",
                    "headless": true
                }),
            },
        )
        .await;

        let session_id = match create_response {
            IpcResponse::Success(value) => {
                assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(true));
                value
                    .get("result")
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
                    .expect("browser new_session should return an id")
            }
            other => panic!("expected success response, got {other:?}"),
        };

        let list_response = IpcServer::process(
            &core,
            &runtime_tool_registry,
            IpcRequest::ExecuteTool {
                name: "browser".to_string(),
                input: serde_json::json!({
                    "action": "list_sessions"
                }),
            },
        )
        .await;

        match list_response {
            IpcResponse::Success(value) => {
                assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(true));
                let sessions = value
                    .get("result")
                    .and_then(|v| v.as_array())
                    .expect("browser list_sessions should return an array");
                assert!(
                    sessions.iter().any(|session| {
                        session.get("id").and_then(|v| v.as_str()) == Some(session_id.as_str())
                    }),
                    "created browser session should be visible in list_sessions"
                );
            }
            other => panic!("expected success response, got {other:?}"),
        }

        let close_response = IpcServer::process(
            &core,
            &runtime_tool_registry,
            IpcRequest::ExecuteTool {
                name: "browser".to_string(),
                input: serde_json::json!({
                    "action": "close_session",
                    "session_id": session_id
                }),
            },
        )
        .await;

        match close_response {
            IpcResponse::Success(value) => {
                assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(true));
            }
            other => panic!("expected success response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn execute_tool_failure_includes_structured_error_metadata() {
        let (core, _temp) = create_test_core().await;
        let runtime_tool_registry = OnceLock::new();

        let response = IpcServer::process(
            &core,
            &runtime_tool_registry,
            IpcRequest::ExecuteTool {
                name: "bash".to_string(),
                input: serde_json::json!({
                    "command": "definitely_not_a_real_command_restflow_12345",
                    "yolo_mode": true
                }),
            },
        )
        .await;

        match response {
            IpcResponse::Success(value) => {
                let result: ToolExecutionResult =
                    serde_json::from_value(value.clone()).expect("tool result should deserialize");
                assert!(!result.success);
                assert!(result.error.is_some());
                assert_eq!(result.error_category, Some(ToolErrorCategory::Config));
                assert_eq!(result.retryable, Some(false));
                assert_eq!(result.retry_after_ms, None);

                assert_eq!(value["error_category"], "Config");
                assert_eq!(value["retryable"], false);
                assert!(value.get("retry_after_ms").is_some());
            }
            other => panic!("expected success response with failed tool payload, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn apply_effective_session_source_uses_binding_data() {
        let (core, _temp) = create_test_core().await;

        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Workspace, "stale-conv");
        core.storage.chat_sessions.create(&session).unwrap();
        core.storage
            .channel_session_bindings
            .upsert(&ChannelSessionBinding::new(
                "telegram",
                None,
                "chat-888",
                &session.id,
            ))
            .unwrap();

        apply_effective_session_source(&core.storage, &mut session).unwrap();
        assert_eq!(session.source_channel, Some(ChatSessionSource::Telegram));
        assert_eq!(session.source_conversation_id.as_deref(), Some("chat-888"));
    }

    #[tokio::test]
    async fn apply_effective_session_source_backfills_legacy_external_binding() {
        let (core, _temp) = create_test_core().await;

        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Telegram, "legacy-conv");
        apply_effective_session_source(&core.storage, &mut session).unwrap();
        assert_eq!(session.source_channel, Some(ChatSessionSource::Telegram));
        assert_eq!(
            session.source_conversation_id.as_deref(),
            Some("legacy-conv")
        );

        let binding = core
            .storage
            .channel_session_bindings
            .get_by_route("telegram", None, "legacy-conv")
            .unwrap()
            .expect("legacy route should be backfilled");
        assert_eq!(binding.session_id, session.id);
    }

    #[tokio::test]
    async fn apply_effective_session_source_defaults_to_workspace_when_no_external_route() {
        let (core, _temp) = create_test_core().await;

        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        apply_effective_session_source(&core.storage, &mut session).unwrap();
        assert_eq!(session.source_channel, Some(ChatSessionSource::Workspace));
        assert!(session.source_conversation_id.is_none());
    }

    #[test]
    fn build_rebuilt_external_session_preserves_binding_and_runtime_config() {
        let mut source = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Telegram, "chat-123")
            .with_name("channel:chat-123")
            .with_skill("skill-1")
            .with_retention("7d");
        source.source_conversation_id = Some("chat-123".to_string());

        let rebuilt =
            build_rebuilt_external_session(&source, ChatSessionSource::Telegram, "chat-123")
                .expect("rebuilt session");
        assert_ne!(rebuilt.id, source.id);
        assert_eq!(rebuilt.agent_id, source.agent_id);
        assert_eq!(rebuilt.model, source.model);
        assert_eq!(rebuilt.skill_id, source.skill_id);
        assert_eq!(rebuilt.retention, source.retention);
        assert_eq!(rebuilt.source_channel, Some(ChatSessionSource::Telegram));
        assert_eq!(rebuilt.source_conversation_id.as_deref(), Some("chat-123"));
        assert_eq!(rebuilt.name, "channel:chat-123");
    }

    #[test]
    fn build_rebuilt_external_session_rejects_workspace_session() {
        let mut source = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        source.source_channel = Some(ChatSessionSource::Workspace);
        source.source_conversation_id = Some("chat-123".to_string());
        let err = build_rebuilt_external_session(&source, ChatSessionSource::Workspace, "chat-123")
            .expect_err("should fail");
        assert!(err.to_string().contains("not externally managed"));
    }

    #[tokio::test]
    async fn resolve_external_session_route_prefers_binding_over_legacy_fields() {
        let (core, _temp) = create_test_core().await;

        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Telegram, "legacy-chat");
        session.source_conversation_id = Some("legacy-chat".to_string());
        core.storage.chat_sessions.create(&session).unwrap();
        core.storage
            .channel_session_bindings
            .upsert(&ChannelSessionBinding::new(
                "discord",
                None,
                "binding-chat",
                &session.id,
            ))
            .unwrap();

        let (channel, conversation_id) =
            resolve_external_session_route(&core.storage, &session).unwrap();
        assert_eq!(channel, ChatSessionSource::Discord);
        assert_eq!(conversation_id, "binding-chat");
    }

    #[tokio::test]
    /// Skills are now registered as callable tools, not injected into the system prompt.
    async fn build_agent_system_prompt_does_not_inject_skills() {
        let (core, _temp) = create_test_core().await;

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
        // Skills are now tools, not injected into prompt
        assert!(!prompt.contains("## Skill: Test Skill"));
    }

    #[tokio::test]
    async fn steer_chat_stream_delivers_message_to_registered_stream() {
        let session_id = format!("session-{}", Uuid::new_v4());
        let stream_id = format!("stream-{}", Uuid::new_v4());
        let (tx, mut rx) = mpsc::channel::<SteerMessage>(1);

        active_chat_stream_sessions()
            .lock()
            .await
            .insert(session_id.clone(), stream_id.clone());
        active_chat_stream_steers()
            .lock()
            .await
            .insert(stream_id.clone(), tx);

        let steered = steer_chat_stream(&session_id, "continue with option B").await;
        assert!(steered);

        let message = rx.recv().await.expect("steer message");
        match message.command {
            SteerCommand::Message { instruction } => {
                assert_eq!(instruction, "continue with option B")
            }
            _ => panic!("expected message steer command"),
        }

        active_chat_stream_sessions()
            .lock()
            .await
            .remove(&session_id);
        active_chat_stream_steers().lock().await.remove(&stream_id);
    }

    #[tokio::test]
    async fn steer_chat_stream_returns_false_when_no_active_session_stream() {
        let session_id = format!("session-{}", Uuid::new_v4());
        let steered = steer_chat_stream(&session_id, "test").await;
        assert!(!steered);
    }

    #[tokio::test]
    async fn session_reply_sender_buffers_message_and_emits_ack_frame() {
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let (tx, mut rx) = mpsc::unbounded_channel::<StreamFrame>();
        let sender = SessionReplySender::new(buffer.clone(), Some(tx));
        ReplySender::send(&sender, "Working on it".to_string())
            .await
            .unwrap();

        let mut guard = buffer.lock().await;
        assert_eq!(guard.pop_front(), Some("Working on it".to_string()));
        drop(guard);

        let frame = rx.recv().await.expect("ack stream frame");
        match frame {
            StreamFrame::Ack { content } => assert_eq!(content, "Working on it"),
            _ => panic!("expected ack frame"),
        }
    }
}
