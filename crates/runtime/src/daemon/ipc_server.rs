use super::ipc_protocol::{
    IPC_PROTOCOL_VERSION, IpcDaemonStatus, IpcRequest, IpcResponse, IpcStreamEvent,
    MAX_MESSAGE_SIZE, StreamFrame, ToolDefinition,
};
use super::session_events::subscribe_session_events;
use super::subscribe_task_events;
use crate::AgentDefaults;
use crate::AppCore;
use crate::auth::{AuthManagerConfig, AuthProfileManager};
use crate::models::{
    AgentNode, ChatExecutionStatus, ChatMessage, ChatRole, ChatSession, ChatSessionSummary,
    ChatTurnEventKind, MessageExecution, ModelId, SteerMessage, SteerSource, TaskStatus,
    TerminalSession,
};
use crate::process::ProcessRegistry;
use crate::runtime::orchestrator::{AgentOrchestratorImpl, InteractiveSessionRequest};
use crate::runtime::session_turn::{
    build_turn_persistence_payload, detect_voice_message, hydrate_voice_message_metadata,
    preprocess_voice_message, replace_latest_user_message_content,
};
use crate::runtime::subagent::StorageBackedSubagentLookup;
use crate::runtime::task_runtime::{AgentRuntimeExecutor, SessionInputMode};
use crate::services::{
    agent as agent_service, config as config_service, secrets as secrets_service,
    session::{PersistInteractiveTurnRequest, SessionService},
    session_policy::SessionPolicyError,
    skills as skills_service,
};
use crate::telemetry::{build_execution_trace_sink, emit_run_interrupted};
use ai::agent::StreamEmitter;
use ai::agent::{SubagentConfig, SubagentTracker};
use ai::telemetry::RestflowTrace;
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
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
use types::DEFAULT_CHAT_MAX_SESSION_HISTORY;
use types::ExecutionScope;
use types::store::ReplySender;
use uuid::Uuid;

#[path = "ipc_server/dispatch.rs"]
mod dispatch;
#[path = "ipc_server/runtime.rs"]
mod runtime;

use self::runtime::{
    ExecuteChatSessionRequest, execute_chat_session, latest_assistant_payload,
    record_turn_event_in_session_store,
};

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

pub struct IpcServer {
    core: Arc<AppCore>,
    socket_path: PathBuf,
    runtime_tool_registry: Arc<OnceLock<ai::tools::ToolRegistry>>,
}

fn active_chat_streams() -> &'static Mutex<HashMap<String, JoinHandle<()>>> {
    static STREAMS: OnceLock<Mutex<HashMap<String, JoinHandle<()>>>> = OnceLock::new();
    STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveChatStreamBinding {
    stream_id: String,
    turn_id: String,
    scope: Option<ExecutionScope>,
}

impl ActiveChatStreamBinding {
    fn new(
        stream_id: impl Into<String>,
        turn_id: impl Into<String>,
        scope: Option<ExecutionScope>,
    ) -> Self {
        Self {
            stream_id: stream_id.into(),
            turn_id: turn_id.into(),
            scope,
        }
    }

    fn same_owner(&self, scope: &Option<ExecutionScope>) -> bool {
        self.scope == *scope
    }
}

fn active_chat_stream_sessions() -> &'static Mutex<HashMap<String, ActiveChatStreamBinding>> {
    static SESSION_STREAMS: OnceLock<Mutex<HashMap<String, ActiveChatStreamBinding>>> =
        OnceLock::new();
    SESSION_STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn active_chat_stream_steers() -> &'static Mutex<HashMap<String, mpsc::Sender<SteerMessage>>> {
    static STEERS: OnceLock<Mutex<HashMap<String, mpsc::Sender<SteerMessage>>>> = OnceLock::new();
    STEERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub async fn open_foreground_chat_session_stream(
    core: Arc<AppCore>,
    session_id: String,
    user_input: Option<String>,
    stream_id: String,
    workspace_root: Option<String>,
) -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
    IpcServer::open_execute_chat_session_stream(
        core,
        session_id,
        user_input,
        stream_id,
        workspace_root,
        None,
    )
    .await
}

pub async fn steer_foreground_chat_stream(
    core: &Arc<AppCore>,
    session_id: &str,
    instruction: &str,
) -> bool {
    runtime::steer_chat_stream(core, session_id, instruction, None).await
}

pub async fn cancel_foreground_chat_stream(core: &Arc<AppCore>, stream_id: &str) -> bool {
    runtime::cancel_chat_stream(core, stream_id).await
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
    let session_service = SessionService::from_storage(&core.storage);
    let actor_id = match session_service.get_session_view(session_id) {
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

pub(crate) fn build_daemon_status() -> IpcDaemonStatus {
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
    core: Arc<AppCore>,
    session_id: String,
    turn_id: String,
    tx: mpsc::UnboundedSender<StreamFrame>,
    has_text_streamed: Arc<AtomicBool>,
    assistant_segment: String,
}

impl IpcStreamEmitter {
    fn new(
        core: Arc<AppCore>,
        session_id: String,
        turn_id: String,
        tx: mpsc::UnboundedSender<StreamFrame>,
        has_text_streamed: Arc<AtomicBool>,
    ) -> Self {
        Self {
            core,
            session_id,
            turn_id,
            tx,
            has_text_streamed,
            assistant_segment: String::new(),
        }
    }

    fn persist_assistant_segment(&mut self) {
        let content = self.assistant_segment.trim_end().to_string();
        self.assistant_segment.clear();
        if content.trim().is_empty() {
            return;
        }
        if let Err(error) = record_turn_event_in_session_store(
            &self.core,
            &self.session_id,
            &self.turn_id,
            ChatTurnEventKind::AssistantMessage { content },
        ) {
            warn!(
                session_id = %self.session_id,
                turn_id = %self.turn_id,
                error = %error,
                "Failed to persist streamed assistant segment"
            );
        }
    }
}

impl Drop for IpcStreamEmitter {
    fn drop(&mut self) {
        self.persist_assistant_segment();
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
            if message.trim().is_empty() {
                return Ok(());
            }

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
    ModelId::normalize_model_id(model)
        .ok_or_else(|| anyhow::anyhow!("Unsupported model identifier: {}", model))
}

fn ipc_session_lifecycle_error(error: anyhow::Error) -> IpcResponse {
    if let Some(lifecycle_error) = error.downcast_ref::<SessionPolicyError>() {
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
        self.assistant_segment.push_str(text);
        let _ = self.tx.send(StreamFrame::Data {
            content: text.to_string(),
        });
    }

    async fn emit_thinking_delta(&mut self, _text: &str) {}

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        self.persist_assistant_segment();
        if let Err(error) = record_turn_event_in_session_store(
            &self.core,
            &self.session_id,
            &self.turn_id,
            ChatTurnEventKind::ToolCall {
                call_id: id.to_string(),
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
        ) {
            warn!(
                session_id = %self.session_id,
                turn_id = %self.turn_id,
                call_id = %id,
                error = %error,
                "Failed to persist turn tool call event"
            );
        }
        let _ = self.tx.send(StreamFrame::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: parse_tool_arguments(arguments),
        });
    }

    async fn emit_tool_call_result(&mut self, id: &str, _name: &str, result: &str, success: bool) {
        if let Err(error) = record_turn_event_in_session_store(
            &self.core,
            &self.session_id,
            &self.turn_id,
            ChatTurnEventKind::ToolResult {
                call_id: id.to_string(),
                success,
                result: result.to_string(),
            },
        ) {
            warn!(
                session_id = %self.session_id,
                turn_id = %self.turn_id,
                call_id = %id,
                error = %error,
                "Failed to persist turn tool result event"
            );
        }
        let _ = self.tx.send(StreamFrame::ToolResult {
            id: id.to_string(),
            result: result.to_string(),
            success,
        });
    }

    async fn emit_complete(&mut self) {
        self.persist_assistant_segment();
    }
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
        runtime_tool_registry: Arc<OnceLock<ai::tools::ToolRegistry>>,
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
                Ok(
                    request @ (IpcRequest::ExecuteChatSessionStream { .. }
                    | IpcRequest::SubscribeTaskEvents { .. }
                    | IpcRequest::SubscribeSessionEvents),
                ) => match Self::open_stream(core.clone(), request).await {
                    Ok(mut rx) => {
                        while let Some(frame) = rx.recv().await {
                            if let Err(err) = Self::send_stream_frame(&mut stream, &frame).await {
                                debug!(error = %err, "Stream client disconnected");
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        let frame = StreamFrame::error(500, err.to_string());
                        let _ = Self::send_stream_frame(&mut stream, &frame).await;
                    }
                },
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

    pub(crate) async fn open_stream(
        _core: Arc<AppCore>,
        request: IpcRequest,
    ) -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
        match request {
            IpcRequest::ExecuteChatSessionStream { .. } => {
                anyhow::bail!("Foreground chat streaming runs in the TUI process")
            }
            IpcRequest::SubscribeTaskEvents {
                task_id,
                run_id,
                scope,
            } => Self::open_task_event_stream(task_id, run_id, scope).await,
            IpcRequest::SubscribeSessionEvents => Self::open_session_event_stream().await,
            other => anyhow::bail!("Unsupported streaming request: {:?}", other),
        }
    }

    async fn open_execute_chat_session_stream(
        core: Arc<AppCore>,
        session_id: String,
        user_input: Option<String>,
        stream_id: String,
        workspace_root: Option<String>,
        scope: Option<ExecutionScope>,
    ) -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
        let stream_id = if stream_id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            stream_id
        };

        // Abort an existing stream with the same ID to avoid duplicate workers.
        let telemetry_sink = build_execution_trace_sink(&core.storage.execution_traces);
        if let Some(existing) = active_chat_streams().lock().await.remove(&stream_id) {
            existing.abort();
            let trace = resolve_chat_stream_trace(&core, &session_id, &stream_id);
            emit_run_interrupted(
                &telemetry_sink,
                trace,
                "replaced by a newer stream with the same stream_id",
                None,
            )
            .await;
        }
        active_chat_stream_steers().lock().await.remove(&stream_id);

        // Keep foreground streams scoped to their terminal owner. A second TUI on
        // the same session should not silently abort the first TUI's active turn.
        let previous_binding = {
            let mut session_streams = active_chat_stream_sessions().lock().await;
            match session_streams.get(&session_id) {
                Some(existing)
                    if existing.stream_id != stream_id && !existing.same_owner(&scope) =>
                {
                    anyhow::bail!(
                        "Session {session_id} already has an active stream owned by another client"
                    );
                }
                _ => session_streams.insert(
                    session_id.clone(),
                    ActiveChatStreamBinding::new(
                        stream_id.clone(),
                        stream_id.clone(),
                        scope.clone(),
                    ),
                ),
            }
        };
        if let Some(previous_binding) = previous_binding
            && previous_binding.stream_id != stream_id
        {
            if let Some(previous) = active_chat_streams()
                .lock()
                .await
                .remove(&previous_binding.stream_id)
            {
                previous.abort();
                let trace =
                    resolve_chat_stream_trace(&core, &session_id, &previous_binding.stream_id);
                emit_run_interrupted(
                    &telemetry_sink,
                    trace,
                    "replaced by a newer stream for the same session owner",
                    None,
                )
                .await;
            }
            active_chat_stream_steers()
                .lock()
                .await
                .remove(&previous_binding.stream_id);
        }

        let (tx, rx) = mpsc::unbounded_channel::<StreamFrame>();
        tx.send(StreamFrame::Start {
            stream_id: stream_id.clone(),
        })?;
        let (steer_tx, steer_rx) = mpsc::channel::<SteerMessage>(64);
        let worker_stream_id = stream_id.clone();
        let worker_turn_id = stream_id.clone();
        let worker_session_id = session_id.clone();
        let worker_session_registry_id = session_id.clone();
        let worker_user_input = user_input.clone();
        let worker_workspace_root = workspace_root.clone();
        let worker_core = core.clone();
        let handle = tokio::spawn(async move {
            let has_text_streamed = Arc::new(AtomicBool::new(false));
            let emitter = IpcStreamEmitter::new(
                worker_core.clone(),
                worker_session_id.clone(),
                worker_turn_id.clone(),
                tx.clone(),
                has_text_streamed.clone(),
            );
            let result = execute_chat_session(
                &worker_core,
                ExecuteChatSessionRequest {
                    session_id: worker_session_id,
                    user_input: worker_user_input,
                    turn_id: worker_turn_id,
                    workspace_root: worker_workspace_root,
                    ack_frame_tx: Some(tx.clone()),
                    emitter: Some(Box::new(emitter)),
                    steer_rx: Some(steer_rx),
                },
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
                        let _ = tx.send(StreamFrame::error(
                            500,
                            "Assistant response missing after execution",
                        ));
                    }
                }
                Err(err) => {
                    let _ = tx.send(StreamFrame::error(err.status_code(), err.to_string()));
                }
            }

            let mut streams = active_chat_streams().lock().await;
            streams.remove(&worker_stream_id);
            active_chat_stream_steers()
                .lock()
                .await
                .remove(&worker_stream_id);
            let mut session_streams = active_chat_stream_sessions().lock().await;
            if session_streams
                .get(&worker_session_registry_id)
                .is_some_and(|binding| binding.stream_id == worker_stream_id)
            {
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

        Ok(rx)
    }

    async fn open_task_event_stream(
        task_id: String,
        run_id: Option<String>,
        scope: Option<ExecutionScope>,
    ) -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
        let stream_id = format!("task-{}", Uuid::new_v4());
        let (tx, rx) = mpsc::unbounded_channel::<StreamFrame>();
        let mut receiver = subscribe_task_events();
        tx.send(StreamFrame::Start {
            stream_id: stream_id.clone(),
        })?;
        let include_all = task_id.trim().is_empty() || task_id == "*";
        tokio::spawn(async move {
            loop {
                let event = match receiver.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(
                            skipped,
                            task_id = %task_id,
                            "Task event stream lagged; dropping oldest events"
                        );
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        let _ = tx.send(StreamFrame::error(500, "Task event stream closed"));
                        break;
                    }
                };

                if !include_all && event.task_id != task_id {
                    continue;
                }
                if let Some(run_id) = run_id.as_deref()
                    && event.run_id.as_deref() != Some(run_id)
                {
                    continue;
                }
                if let Some(scope) = scope.as_ref()
                    && event.scope.as_ref() != Some(scope)
                {
                    continue;
                }

                if tx
                    .send(StreamFrame::Event {
                        event: IpcStreamEvent::Task(event),
                    })
                    .is_err()
                {
                    break;
                }
            }

            debug!(stream_id = %stream_id, "Background event subscription ended");
        });

        Ok(rx)
    }

    async fn open_session_event_stream() -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
        let stream_id = format!("session-events-{}", Uuid::new_v4());
        let (tx, rx) = mpsc::unbounded_channel::<StreamFrame>();
        let mut receiver = subscribe_session_events();
        tx.send(StreamFrame::Start {
            stream_id: stream_id.clone(),
        })?;

        tokio::spawn(async move {
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
                        let _ = tx.send(StreamFrame::error(500, "Session event stream closed"));
                        break;
                    }
                };

                if tx
                    .send(StreamFrame::Event {
                        event: IpcStreamEvent::Session(event),
                    })
                    .is_err()
                {
                    break;
                }
            }

            debug!(stream_id = %stream_id, "Session event subscription ended");
        });

        Ok(rx)
    }
}

#[cfg(test)]
#[path = "ipc_server/tests/mod.rs"]
mod tests;
