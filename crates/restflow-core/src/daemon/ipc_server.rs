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

#[path = "ipc_server/dispatch.rs"]
mod dispatch;
#[path = "ipc_server/runtime.rs"]
mod runtime;

use self::runtime::{execute_chat_session, latest_assistant_payload};

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
}

#[cfg(test)]
#[path = "ipc_server/tests.rs"]
mod tests;
