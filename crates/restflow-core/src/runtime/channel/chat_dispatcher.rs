//! Chat Dispatcher - Handles natural language messages via AI agent.
//!
//! When a user sends a natural language message (not a command), the
//! ChatDispatcher processes it through an AI agent and returns the response.

use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, error, info, warn};

use crate::auth::AuthProfileManager;
use crate::channel::{
    ChannelReplySender, ChannelRouter, ChannelType, InboundMessage, OutboundMessage,
};
use crate::models::{
    ChannelSessionBinding, ChatMessage, ChatSession, ChatSessionSource, MessageExecution, ModelId,
};
use crate::process::ProcessRegistry;
use crate::runtime::background_agent::{AgentRuntimeExecutor, SessionInputMode};
use crate::runtime::channel::{
    build_turn_persistence_payload, detect_voice_message, hydrate_voice_message_metadata,
    preprocess_voice_message,
};
use crate::runtime::orchestrator::{
    AgentOrchestratorImpl, InteractiveExecutionError, InteractiveSessionRequest,
};
use crate::runtime::output::{ensure_success_output, format_error_output};
use crate::runtime::trace::append_message_trace;
use crate::services::session::{PersistInteractiveTurnRequest, SessionService};
use crate::storage::Storage;
use restflow_storage::AgentDefaults;
use restflow_traits::DEFAULT_CHAT_MAX_SESSION_HISTORY;

use super::debounce::MessageDebouncer;
use restflow_ai::agent::{SubagentConfig, SubagentDefLookup, SubagentTracker};

/// Configuration for the ChatDispatcher.
#[derive(Debug, Clone)]
pub struct ChatDispatcherConfig {
    /// Maximum number of messages to keep in session history.
    pub max_session_history: usize,
    /// AI response timeout in seconds.
    ///
    /// `None` disables timeout and allows long-running foreground tasks.
    pub response_timeout_secs: Option<u64>,
    /// Whether to send typing indicator while processing.
    pub send_typing_indicator: bool,
    /// Default agent name to use when none is specified.
    pub default_agent_name: String,
}

impl Default for ChatDispatcherConfig {
    fn default() -> Self {
        Self {
            max_session_history: DEFAULT_CHAT_MAX_SESSION_HISTORY,
            response_timeout_secs: None,
            send_typing_indicator: true,
            default_agent_name: "default".to_string(),
        }
    }
}

/// Error types for chat operations.
#[derive(Debug)]
pub enum ChatError {
    /// No default agent configured.
    NoDefaultAgent,
    /// Agent execution failed.
    ExecutionFailed(String),
    /// Session storage error.
    SessionError(String),
    /// Voice preprocessing failed before agent execution.
    VoicePreprocessFailed(String),
    /// API key not configured.
    NoApiKey { provider: String },
    /// Rate limited.
    RateLimited,
    /// Timeout.
    Timeout,
}

const MAX_USER_ERROR_DETAIL_CHARS: usize = 280;

fn summarize_error_detail(detail: &str) -> String {
    let first_line = detail
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(detail.trim());
    let collapsed = first_line.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.is_empty() {
        return "unknown error".to_string();
    }

    let char_count = collapsed.chars().count();
    if char_count <= MAX_USER_ERROR_DETAIL_CHARS {
        return collapsed;
    }

    let truncated = collapsed
        .chars()
        .take(MAX_USER_ERROR_DETAIL_CHARS)
        .collect::<String>();
    format!("{}...", truncated)
}

impl ChatError {
    /// Get a user-friendly error message.
    pub fn user_message(&self) -> &str {
        match self {
            Self::NoDefaultAgent => {
                "No AI agent configured. Please set up a default agent in settings."
            }
            Self::NoApiKey { .. } => "API key not configured. Please add your API key in settings.",
            Self::RateLimited => "Too many requests. Please wait a moment and try again.",
            Self::Timeout => "AI response timed out. Please try again or simplify your question.",
            Self::ExecutionFailed(_) | Self::SessionError(_) | Self::VoicePreprocessFailed(_) => {
                "An error occurred while processing your message. Please try again."
            }
        }
    }

    /// Get a user-facing message that includes concise execution details when available.
    pub fn user_message_with_details(&self) -> String {
        match self {
            Self::ExecutionFailed(detail) => {
                format!("Agent execution failed: {}", summarize_error_detail(detail))
            }
            Self::SessionError(detail) => {
                format!("Session error: {}", summarize_error_detail(detail))
            }
            Self::VoicePreprocessFailed(detail) => {
                format!(
                    "Voice transcription failed: {}",
                    summarize_error_detail(detail)
                )
            }
            _ => self.user_message().to_string(),
        }
    }
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDefaultAgent => write!(f, "No default agent configured"),
            Self::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            Self::SessionError(msg) => write!(f, "Session error: {}", msg),
            Self::VoicePreprocessFailed(msg) => {
                write!(f, "Voice preprocessing failed: {}", msg)
            }
            Self::NoApiKey { provider } => write!(f, "No API key for provider: {}", provider),
            Self::RateLimited => write!(f, "Rate limited"),
            Self::Timeout => write!(f, "Response timeout"),
        }
    }
}

impl std::error::Error for ChatError {}

#[derive(Debug)]
struct PreparedVoiceInput {
    agent_input: String,
    persisted_input: String,
}

#[derive(Debug)]
struct VoicePreprocessFailure {
    normalized_input: String,
    error: ChatError,
}

/// Chat session manager for conversation persistence.
pub struct ChatSessionManager {
    storage: Arc<Storage>,
    session_service: SessionService,
    /// Mutex to serialize session creation and prevent duplicate sessions
    /// from being created under concurrent requests for the same conversation.
    session_creation_mutex: TokioMutex<()>,
    default_agent_id: Option<String>,
    max_history: usize,
}

impl ChatSessionManager {
    /// Create a new ChatSessionManager.
    pub fn new(storage: Arc<Storage>, max_history: usize) -> Self {
        Self {
            session_service: SessionService::from_storage(&storage),
            storage,
            session_creation_mutex: TokioMutex::new(()),
            default_agent_id: None,
            max_history,
        }
    }

    /// Set the default agent ID for new sessions.
    pub fn with_default_agent(mut self, agent_id: String) -> Self {
        self.default_agent_id = Some(agent_id);
        self
    }

    /// Get or create a session for a conversation.
    ///
    /// Sessions are keyed by conversation_id (e.g., Telegram chat ID).
    pub async fn get_or_create_session(
        &self,
        channel_type: ChannelType,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<ChatSession> {
        let source_channel = Self::source_from_channel(channel_type);
        let binding_channel = Self::binding_channel_key(channel_type);

        // Use mutex to serialize session creation and prevent race conditions.
        // This ensures that under concurrent requests for the same conversation,
        // only one session will be created.
        let _guard = self.session_creation_mutex.lock().await;

        if let Some(channel_key) = binding_channel
            && let Some(mut session) =
                self.lookup_session_from_binding(channel_key, conversation_id)?
        {
            self.maybe_rebind_to_forced_default(&mut session)?;
            debug!(
                "Found session {} via channel binding for {:?} conversation {}",
                session.id, channel_type, conversation_id
            );
            return Ok(session);
        }

        // Re-check after acquiring lock (another task may have created it)
        let sessions = self.storage.chat_sessions.list_all()?;
        if let Some(session) = sessions
            .iter()
            .find(|s| {
                s.source_channel == source_channel
                    && s.source_conversation_id.as_deref() == Some(conversation_id)
            })
            .cloned()
        {
            let mut session = session;
            if let Some(channel_key) = binding_channel
                && let Err(err) =
                    self.upsert_channel_binding(channel_key, conversation_id, &session.id)
            {
                warn!(
                    session_id = %session.id,
                    channel = channel_key,
                    conversation_id = %conversation_id,
                    error = %err,
                    "Failed to backfill channel-session binding for existing source session"
                );
            }
            self.maybe_rebind_to_forced_default(&mut session)?;
            debug!(
                "Found existing session {} for {:?} conversation {}",
                session.id, channel_type, conversation_id
            );
            return Ok(session);
        }

        // Rebind migrated legacy sessions (`external_legacy` / missing source) when possible.
        if let Some(source_channel) = source_channel
            && let Some(mut session) = sessions
                .iter()
                .find(|s| {
                    s.source_conversation_id.as_deref() == Some(conversation_id)
                        && matches!(
                            s.source_channel,
                            None | Some(ChatSessionSource::ExternalLegacy)
                        )
                })
                .cloned()
        {
            if session.source_channel != Some(source_channel) {
                session.source_channel = Some(source_channel);
                if let Err(err) = self.storage.chat_sessions.save(&session) {
                    warn!(
                        "Failed to persist source channel rebind for session {}: {}",
                        session.id, err
                    );
                }
            }
            if let Some(channel_key) = binding_channel
                && let Err(err) =
                    self.upsert_channel_binding(channel_key, conversation_id, &session.id)
            {
                warn!(
                    session_id = %session.id,
                    channel = channel_key,
                    conversation_id = %conversation_id,
                    error = %err,
                    "Failed to backfill channel-session binding for migrated legacy session"
                );
            }
            self.maybe_rebind_to_forced_default(&mut session)?;

            debug!(
                "Reused migrated legacy session {} for {:?} conversation {}",
                session.id, channel_type, conversation_id
            );
            return Ok(session);
        }

        // Create new session (we hold the mutex, so no race)
        let agent_id = self.get_default_agent_id()?;
        let model = self.get_agent_model(&agent_id)?;

        let mut session = ChatSession::new(agent_id, model).with_name(conversation_id);
        if let Some(source_channel) = source_channel {
            session = session.with_source(source_channel, conversation_id);
        }

        // Handle potential duplicate from race condition (defensive)
        if let Err(e) = self.storage.chat_sessions.create(&session) {
            if let Some(channel_key) = binding_channel
                && let Some(existing) =
                    self.lookup_session_from_binding(channel_key, conversation_id)?
            {
                debug!(
                    "Session {} was created by another request (binding), using existing",
                    existing.id
                );
                return Ok(existing);
            }
            // It's a real error, propagate it
            return Err(e);
        }

        if let Some(channel_key) = binding_channel
            && let Err(err) = self.upsert_channel_binding(channel_key, conversation_id, &session.id)
        {
            warn!(
                session_id = %session.id,
                channel = channel_key,
                conversation_id = %conversation_id,
                error = %err,
                "Failed to persist channel-session binding for new session"
            );
        }

        info!(
            "Created new chat session {} for conversation {} (user: {})",
            session.id, conversation_id, user_id
        );

        Ok(session)
    }

    fn source_from_channel(channel_type: ChannelType) -> Option<ChatSessionSource> {
        match channel_type {
            ChannelType::Telegram => Some(ChatSessionSource::Telegram),
            ChannelType::Discord => Some(ChatSessionSource::Discord),
            ChannelType::Slack => Some(ChatSessionSource::Slack),
            ChannelType::Email | ChannelType::Webhook => None,
        }
    }

    fn binding_channel_key(channel_type: ChannelType) -> Option<&'static str> {
        match channel_type {
            ChannelType::Telegram => Some("telegram"),
            ChannelType::Discord => Some("discord"),
            ChannelType::Slack => Some("slack"),
            ChannelType::Email | ChannelType::Webhook => None,
        }
    }

    fn upsert_channel_binding(
        &self,
        channel_key: &str,
        conversation_id: &str,
        session_id: &str,
    ) -> Result<()> {
        let binding = ChannelSessionBinding::new(channel_key, None, conversation_id, session_id);
        self.storage.channel_session_bindings.upsert(&binding)
    }

    fn lookup_session_from_binding(
        &self,
        channel_key: &str,
        conversation_id: &str,
    ) -> Result<Option<ChatSession>> {
        let Some(binding) = self.storage.channel_session_bindings.get_by_route(
            channel_key,
            None,
            conversation_id,
        )?
        else {
            return Ok(None);
        };

        if let Some(session) = self.storage.chat_sessions.get(&binding.session_id)? {
            return Ok(Some(session));
        }

        warn!(
            channel = channel_key,
            conversation_id = %conversation_id,
            session_id = %binding.session_id,
            "Found stale channel-session binding without corresponding session; cleaning up"
        );
        if let Err(error) = self.storage.channel_session_bindings.remove_by_route(
            channel_key,
            None,
            conversation_id,
        ) {
            warn!(
                channel = channel_key,
                conversation_id = %conversation_id,
                session_id = %binding.session_id,
                error = %error,
                "Failed to remove stale channel-session binding"
            );
        }
        Ok(None)
    }

    /// Append a user-assistant exchange to a session.
    ///
    /// This method is thread-safe: concurrent calls for the same session are serialized
    /// using a per-session lock to prevent lost updates.
    pub fn append_exchange(
        &self,
        session_id: &str,
        user_message: &str,
        assistant_message: &str,
        active_model: Option<&str>,
        execution: Option<MessageExecution>,
    ) -> Result<()> {
        let mut user_msg = ChatMessage::user(user_message);
        hydrate_voice_message_metadata(&mut user_msg);
        let assistant_msg = if let Some(exec) = execution {
            ChatMessage::assistant(assistant_message).with_execution(exec)
        } else {
            ChatMessage::assistant(assistant_message)
        };

        let session = self.session_service.append_exchange(
            session_id,
            user_msg,
            assistant_msg,
            active_model,
            "channel",
        )?;

        debug!(
            "Persisted full history for session {} (stored messages: {}, runtime window: {})",
            session_id,
            session.messages.len(),
            self.max_history
        );

        Ok(())
    }

    pub fn append_user_message(&self, session_id: &str, user_message: &str) -> Result<ChatSession> {
        self.session_service.append_user_message(
            session_id,
            ChatMessage::user(user_message),
            "channel",
        )
    }

    pub fn persist_interactive_turn(
        &self,
        session: &mut ChatSession,
        request: PersistInteractiveTurnRequest<'_>,
    ) -> Result<()> {
        self.session_service
            .persist_interactive_turn(session, request)
    }

    /// Get message history for a session (for context).
    pub fn get_history(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        let session = self
            .storage
            .chat_sessions
            .get(session_id)?
            .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

        Ok(session.messages)
    }

    /// Rebind an existing session to another agent.
    pub fn rebind_session_agent(&self, session_id: &str, agent_id: &str) -> Result<ChatSession> {
        let mut session = self
            .storage
            .chat_sessions
            .get(session_id)?
            .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

        let model = self.get_agent_model(agent_id)?;
        session.agent_id = agent_id.to_string();
        session.model = model.clone();
        session.metadata.last_model = Some(model);

        self.storage.chat_sessions.save(&session)?;
        Ok(session)
    }

    /// Get the default agent ID.
    fn get_default_agent_id(&self) -> Result<String> {
        if let Some(ref id) = self.default_agent_id {
            return Ok(id.clone());
        }

        self.storage.agents.resolve_default_agent_id()
    }

    /// Get the model for an agent.
    fn get_agent_model(&self, agent_id: &str) -> Result<String> {
        let agent = self
            .storage
            .agents
            .get_agent(agent_id.to_string())?
            .ok_or_else(|| anyhow!("Agent not found: {}", agent_id))?;

        Ok(agent
            .agent
            .model
            .map(|m| m.as_serialized_str().to_string())
            .unwrap_or_else(|| ModelId::Gpt5.as_serialized_str().to_string()))
    }

    fn maybe_rebind_to_forced_default(&self, session: &mut ChatSession) -> Result<()> {
        let Some(default_agent_id) = self.default_agent_id.as_ref() else {
            return Ok(());
        };
        if session.agent_id == *default_agent_id {
            return Ok(());
        }

        let model = self.get_agent_model(default_agent_id)?;
        session.agent_id = default_agent_id.clone();
        session.model = model.clone();
        session.metadata.last_model = Some(model);

        if let Err(err) = self.storage.chat_sessions.save(session) {
            warn!(
                "Failed to persist forced default-agent rebind for session {}: {}",
                session.id, err
            );
        }

        Ok(())
    }
}

/// Dispatches natural language messages to AI agents.
///
/// The ChatDispatcher:
/// 1. Debounces rapid messages
/// 2. Retrieves or creates a chat session
/// 3. Executes the AI agent with conversation history
/// 4. Sends the response back to the user
pub struct ChatDispatcher {
    sessions: Arc<ChatSessionManager>,
    storage: Arc<Storage>,
    auth_manager: Arc<AuthProfileManager>,
    debouncer: Arc<MessageDebouncer>,
    channel_router: Arc<ChannelRouter>,
    config: ChatDispatcherConfig,
    subagent_tracker: Arc<SubagentTracker>,
    subagent_definitions: Arc<dyn SubagentDefLookup>,
    subagent_config: SubagentConfig,
}

impl ChatDispatcher {
    /// Create a new ChatDispatcher.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sessions: Arc<ChatSessionManager>,
        storage: Arc<Storage>,
        auth_manager: Arc<AuthProfileManager>,
        debouncer: Arc<MessageDebouncer>,
        channel_router: Arc<ChannelRouter>,
        config: ChatDispatcherConfig,
        subagent_tracker: Arc<SubagentTracker>,
        subagent_definitions: Arc<dyn SubagentDefLookup>,
        subagent_config: SubagentConfig,
    ) -> Self {
        Self {
            sessions,
            storage,
            auth_manager,
            debouncer,
            channel_router,
            config,
            subagent_tracker,
            subagent_definitions,
            subagent_config,
        }
    }

    fn process_registry_for_executor(&self) -> Arc<ProcessRegistry> {
        let ttl_secs = match self.storage.config.get_effective_config() {
            Ok(config) => config.agent.process_session_ttl_secs,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Failed to load process session TTL from effective config; using default process registry TTL"
                );
                AgentDefaults::default().process_session_ttl_secs
            }
        };
        Arc::new(ProcessRegistry::new().with_ttl_seconds(ttl_secs))
    }

    fn create_executor(&self) -> AgentRuntimeExecutor {
        AgentRuntimeExecutor::new(
            self.storage.clone(),
            self.process_registry_for_executor(),
            self.auth_manager.clone(),
            self.subagent_tracker.clone(),
            self.subagent_definitions.clone(),
            self.subagent_config.clone(),
        )
    }

    fn map_execution_error(error: &anyhow::Error) -> ChatError {
        let msg = error.to_string();
        let lower = msg.to_lowercase();

        if msg.contains("No AI agent configured") || msg.contains("No agents configured") {
            return ChatError::NoDefaultAgent;
        }

        if msg.contains("No API key configured for provider") {
            return ChatError::NoApiKey {
                provider: "unknown".to_string(),
            };
        }

        if lower.contains("rate limit")
            || lower.contains("429")
            || lower.contains("too many requests")
        {
            return ChatError::RateLimited;
        }

        ChatError::ExecutionFailed(msg)
    }

    /// Build the persisted user-facing content for an inbound message.
    ///
    /// Voice messages are normalized into a stable media-context block without
    /// embedding agent-specific instructions.
    fn build_effective_input(
        message: &InboundMessage,
        content: &str,
        file_path_override: Option<&str>,
    ) -> String {
        if let Some(descriptor) =
            detect_voice_message(content, message.metadata.as_ref(), file_path_override)
        {
            return descriptor.persisted_content(None);
        }

        content.to_string()
    }

    async fn preprocess_voice_input(
        &self,
        message: &InboundMessage,
        content: &str,
        file_path_override: Option<&str>,
    ) -> std::result::Result<Option<PreparedVoiceInput>, VoicePreprocessFailure> {
        let Some(descriptor) =
            detect_voice_message(content, message.metadata.as_ref(), file_path_override)
        else {
            return Ok(None);
        };

        let normalized = descriptor.persisted_content(None);
        preprocess_voice_message(&self.storage, &descriptor)
            .await
            .map(|result| {
                Some(PreparedVoiceInput {
                    agent_input: result.agent_input,
                    persisted_input: result.persisted_input,
                })
            })
            .map_err(|error| VoicePreprocessFailure {
                normalized_input: normalized,
                error: ChatError::VoicePreprocessFailed(error.to_string()),
            })
    }

    /// Relocate a media file from `~/.restflow/media/` to `~/.restflow/media/{session_id}/`
    /// if it's not already in a session subdirectory.
    /// Returns the new path if relocated, or None if no relocation was needed.
    fn relocate_media_to_session(
        metadata: Option<&serde_json::Value>,
        session_id: &str,
    ) -> Option<String> {
        let metadata = metadata?;
        let file_path_str = metadata.get("file_path").and_then(|v| v.as_str())?;
        let file_path = std::path::Path::new(file_path_str);

        let media_dir = crate::paths::media_dir().ok()?;

        // Only relocate files that are directly in ~/.restflow/media/ (not already in a session subdir)
        if file_path.parent()? != media_dir {
            return None;
        }

        let session_dir = crate::paths::session_media_dir(session_id).ok()?;
        let file_name = file_path.file_name()?;
        let new_path = session_dir.join(file_name);

        match std::fs::rename(file_path, &new_path) {
            Ok(()) => {
                debug!(
                    old_path = %file_path_str,
                    new_path = %new_path.display(),
                    session_id,
                    "Relocated media file to session directory"
                );
                Some(new_path.to_string_lossy().to_string())
            }
            Err(e) => {
                warn!(
                    error = %e,
                    old_path = %file_path_str,
                    "Failed to relocate media file to session directory"
                );
                None
            }
        }
    }

    /// Dispatch a message to the AI agent.
    pub async fn dispatch(&self, message: &InboundMessage) -> Result<()> {
        // 1. Build initial persisted input (before session is known)
        let initial_input = Self::build_effective_input(message, &message.content, None);

        // 2. Debounce messages
        let debounced = match self
            .debouncer
            .debounce(&message.conversation_id, &initial_input)
            .await
        {
            Some(text) => text,
            None => {
                // Not the primary message in this batch; skip
                debug!(
                    "Skipping message in debounce batch for {}",
                    message.conversation_id
                );
                return Ok(());
            }
        };

        // 3. Get or create session
        let mut session = match self
            .sessions
            .get_or_create_session(
                message.channel_type,
                &message.conversation_id,
                &message.sender_id,
            )
            .await
        {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get/create session: {}", e);
                self.send_error_response(message, ChatError::NoDefaultAgent)
                    .await?;
                return Ok(());
            }
        };

        // 4. Relocate media to session directory and rebuild input if path changed
        let relocated_path =
            Self::relocate_media_to_session(message.metadata.as_ref(), &session.id);
        let input = if relocated_path.is_some() {
            Self::build_effective_input(message, &debounced, relocated_path.as_deref())
        } else {
            debounced
        };

        let voice_input = match self
            .preprocess_voice_input(message, &input, relocated_path.as_deref())
            .await
        {
            Ok(result) => result,
            Err(VoicePreprocessFailure {
                normalized_input,
                error,
            }) => {
                if let Err(session_error) = self
                    .sessions
                    .append_user_message(&session.id, &normalized_input)
                {
                    warn!(
                        session_id = %session.id,
                        error = %session_error,
                        "Failed to persist voice message after preprocessing error"
                    );
                }
                self.send_error_response(message, error).await?;
                return Ok(());
            }
        };
        let persisted_input = voice_input
            .as_ref()
            .map(|voice| voice.persisted_input.clone())
            .unwrap_or_else(|| input.clone());
        let agent_input = voice_input
            .as_ref()
            .map(|voice| voice.agent_input.clone())
            .unwrap_or_else(|| input.clone());
        let input_mode = if voice_input.is_some() {
            if let Err(error) = self
                .sessions
                .append_user_message(&session.id, &persisted_input)
            {
                self.send_error_response(message, ChatError::SessionError(error.to_string()))
                    .await?;
                return Ok(());
            }
            match self.storage.chat_sessions.get(&session.id) {
                Ok(Some(updated)) => session = updated,
                Ok(None) => {
                    self.send_error_response(
                        message,
                        ChatError::SessionError(
                            "Session not found after user message persistence".to_string(),
                        ),
                    )
                    .await?;
                    return Ok(());
                }
                Err(error) => {
                    self.send_error_response(message, ChatError::SessionError(error.to_string()))
                        .await?;
                    return Ok(());
                }
            }
            SessionInputMode::PersistedInSession
        } else {
            SessionInputMode::EphemeralInput
        };

        info!(
            "Processing chat message for session {} (conversation: {})",
            session.id, message.conversation_id
        );

        // 3. Send typing indicator if enabled
        if self.config.send_typing_indicator {
            debug!("Sending typing indicator to {}", message.conversation_id);
            if let Err(e) = self.send_typing_indicator(message).await {
                warn!("Failed to send typing indicator: {}", e);
            }
            debug!("Typing indicator sent");
        }

        // 4. Execute via shared runtime executor.
        let original_agent_id = session.agent_id.clone();
        let reply_sender = Arc::new(ChannelReplySender::new(
            self.channel_router.clone(),
            &message.conversation_id,
            message.channel_type,
        ));
        let executor = self.create_executor().with_reply_sender(reply_sender);
        self.maybe_send_acknowledgement(&executor, &mut session, &agent_input, input_mode, message)
            .await;
        let run_id = uuid::Uuid::new_v4().to_string();
        let orchestrator = AgentOrchestratorImpl::from_runtime_executor(executor);
        let traced_execution = match orchestrator
            .run_traced_interactive_session_turn(InteractiveSessionRequest {
                session: &mut session,
                user_input: &agent_input,
                max_history: self.config.max_session_history,
                input_mode,
                run_id,
                tool_trace_storage: self.storage.tool_traces.clone(),
                execution_trace_storage: self.storage.execution_traces.clone(),
                timeout_secs: self.config.response_timeout_secs,
                emitter: None,
                steer_rx: None,
            })
            .await
        {
            Ok(result) => result,
            Err(InteractiveExecutionError::Timeout { timeout_secs }) => {
                error!("Agent execution timed out after {} seconds", timeout_secs);
                self.send_error_response(message, ChatError::Timeout)
                    .await?;
                return Ok(());
            }
            Err(InteractiveExecutionError::Execution(error)) => {
                error!("Agent execution failed: {}", error);
                self.send_error_response(message, Self::map_execution_error(&error))
                    .await?;
                return Ok(());
            }
        };
        let trace = traced_execution.trace;
        let duration_ms = traced_execution.duration_ms;
        let exec_result = traced_execution.execution;

        if session.agent_id != original_agent_id {
            match self
                .sessions
                .rebind_session_agent(&session.id, &session.agent_id)
            {
                Ok(updated) => session = updated,
                Err(error) => {
                    warn!(
                        "Failed to persist fallback agent binding for session {}: {}",
                        session.id, error
                    );
                }
            }
        }

        let verification = format!(
            "Response is ready for channel {:?} in session {}.",
            message.channel_type, session.id
        );
        let structured_output = ensure_success_output(
            &exec_result.output,
            "Processed the inbound message using the active agent, available tools, and session context.",
            &verification,
        );

        let (execution, final_persisted_input) = build_turn_persistence_payload(
            &self.storage.tool_traces,
            &session.id,
            &trace.turn_id,
            &persisted_input,
            duration_ms,
            exec_result.iterations,
        );
        let persist_result = if voice_input.is_some() {
            self.sessions.persist_interactive_turn(
                &mut session,
                PersistInteractiveTurnRequest {
                    original_input: &persisted_input,
                    persisted_input: &final_persisted_input,
                    assistant_output: &structured_output,
                    active_model: Some(&exec_result.active_model),
                    execution,
                    source: "channel",
                },
            )
        } else {
            self.sessions.append_exchange(
                &session.id,
                &final_persisted_input,
                &structured_output,
                Some(&exec_result.active_model),
                Some(execution),
            )
        };
        if let Err(e) = persist_result {
            warn!("Failed to save exchange to session: {}", e);
        } else {
            append_message_trace(
                &self.storage.tool_traces,
                &self.storage.execution_traces,
                &trace,
                "user",
                &final_persisted_input,
            );
            append_message_trace(
                &self.storage.tool_traces,
                &self.storage.execution_traces,
                &trace,
                "assistant",
                &structured_output,
            );
        }

        // 6. Send response (plain message without emoji prefix for AI chat)
        let response = OutboundMessage::plain(&message.conversation_id, &structured_output);
        self.channel_router
            .send_to(message.channel_type, response)
            .await?;

        info!(
            "Chat response sent for session {} (output length: {} chars)",
            session.id,
            structured_output.len()
        );

        Ok(())
    }

    fn build_ack_outbound_message(conversation_id: &str, content: &str) -> OutboundMessage {
        let mut response = OutboundMessage::new(conversation_id, content);
        // Ack text is generated dynamically and may include markdown-reserved
        // characters. Keep plain text mode to avoid adapter parse failures.
        response.parse_mode = None;
        response
    }

    async fn maybe_send_acknowledgement(
        &self,
        executor: &AgentRuntimeExecutor,
        session: &mut ChatSession,
        user_input: &str,
        input_mode: SessionInputMode,
        message: &InboundMessage,
    ) {
        match executor
            .generate_session_acknowledgement(session, user_input, input_mode)
            .await
        {
            Ok(Some(content)) => {
                let response = Self::build_ack_outbound_message(&message.conversation_id, &content);
                if let Err(error) = self
                    .channel_router
                    .send_to(message.channel_type, response)
                    .await
                {
                    warn!(
                        session_id = %session.id,
                        error = %error,
                        "Failed to send acknowledgement to channel"
                    );
                }
            }
            Ok(None) => {}
            Err(error) => {
                warn!(
                    session_id = %session.id,
                    error = %error,
                    "Failed to generate acknowledgement message"
                );
            }
        }
    }

    /// Send typing indicator to the conversation.
    async fn send_typing_indicator(&self, message: &InboundMessage) -> Result<()> {
        self.channel_router
            .send_typing_to(message.channel_type, &message.conversation_id)
            .await
    }

    /// Send an error response to the user.
    async fn send_error_response(&self, message: &InboundMessage, error: ChatError) -> Result<()> {
        let error_text = error.user_message_with_details();
        let operation = "Attempted to process the incoming message through chat session resolution and agent execution.";
        let verification =
            "Execution failed. Review the evidence above, adjust configuration/input, and retry.";
        let structured_error = format_error_output(&error_text, operation, verification);
        let mut response = OutboundMessage::warning(&message.conversation_id, &structured_error);
        // Error details can include arbitrary characters from provider/tool errors.
        // Use plain text mode to avoid markdown parse failures in channel adapters.
        response.parse_mode = None;
        self.channel_router
            .send_to(message.channel_type, response)
            .await
    }
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use crate::ModelId;
    use crate::channel::ChannelType;
    use crate::models::ChatSessionSource;
    use crate::runtime::effective_main_agent_tool_names;
    use serde_json::json;
    use tempfile::tempdir;
    use tokio::time::Duration;

    fn create_test_storage() -> (Arc<Storage>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        (Arc::new(storage), temp_dir)
    }

    #[allow(dead_code)]
    fn create_message(content: &str) -> InboundMessage {
        InboundMessage::new("msg-1", ChannelType::Telegram, "user-1", "chat-1", content)
    }

    #[test]
    fn test_build_effective_input_plain_text_unchanged() {
        let message = create_message("hello");
        let input = ChatDispatcher::build_effective_input(&message, &message.content, None);
        assert_eq!(input, "hello");
    }

    #[test]
    fn test_build_effective_input_voice_normalizes_media_context_without_instruction() {
        let media_dir = crate::paths::media_dir().unwrap();
        let voice_path = media_dir.join("tg-voice.ogg");
        let voice_path_str = voice_path.to_string_lossy().to_string();

        let message = create_message("[Voice message, 6s]").with_metadata(json!({
            "media_type": "voice",
            "file_path": voice_path_str
        }));

        let input = ChatDispatcher::build_effective_input(&message, &message.content, None);
        assert!(input.contains("media_type: voice"));
        assert!(input.contains(&format!("local_file_path: {}", voice_path_str)));
        assert!(!input.contains("Use the transcribe tool with this file_path"));
    }

    #[test]
    fn test_build_effective_input_voice_without_file_path_keeps_original_content() {
        let message = create_message("[Voice message, 6s]").with_metadata(json!({
            "media_type": "voice"
        }));

        let input = ChatDispatcher::build_effective_input(&message, &message.content, None);
        assert_eq!(input, "[Voice message, 6s]");
    }

    #[test]
    fn test_build_effective_input_rewrites_media_path_after_relocation() {
        let media_dir = crate::paths::media_dir().unwrap();
        let voice_path = media_dir.join("session-voice.ogg");
        let voice_path_str = voice_path.to_string_lossy().to_string();
        let relocated_path = media_dir
            .join("session-id")
            .join("session-voice.ogg")
            .to_string_lossy()
            .to_string();

        let message = create_message("[Voice message, 6s]").with_metadata(json!({
            "media_type": "voice",
            "file_path": voice_path_str
        }));

        let input = ChatDispatcher::build_effective_input(
            &message,
            &format!(
                "[Voice message, 6s]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: {}",
                voice_path_str
            ),
            Some(&relocated_path),
        );

        assert!(input.contains("[Voice message, 6s]"));
        assert!(input.contains(&relocated_path));
        assert!(!input.contains("instruction:"));
    }

    #[test]
    fn test_config_defaults() {
        let config = ChatDispatcherConfig::default();
        assert_eq!(config.max_session_history, DEFAULT_CHAT_MAX_SESSION_HISTORY);
        assert_eq!(config.response_timeout_secs, None);
        assert!(config.send_typing_indicator);
    }

    #[test]
    fn test_config_allows_explicit_timeout() {
        let config = ChatDispatcherConfig {
            response_timeout_secs: Some(300),
            ..ChatDispatcherConfig::default()
        };
        assert_eq!(config.response_timeout_secs, Some(300));
    }

    #[test]
    fn test_chat_error_user_messages() {
        assert!(!ChatError::NoDefaultAgent.user_message().is_empty());
        assert!(
            !ChatError::NoApiKey {
                provider: "test".to_string()
            }
            .user_message()
            .is_empty()
        );
        assert!(!ChatError::RateLimited.user_message().is_empty());
        assert!(!ChatError::Timeout.user_message().is_empty());
        assert!(
            !ChatError::ExecutionFailed("test".to_string())
                .user_message()
                .is_empty()
        );
    }

    #[test]
    fn test_chat_error_user_message_with_details() {
        let message = ChatError::ExecutionFailed(
            "tool call failed: invalid argument\nstacktrace: omitted".to_string(),
        )
        .user_message_with_details();
        assert!(message.contains("Agent execution failed:"));
        assert!(message.contains("tool call failed: invalid argument"));
        assert!(!message.contains("stacktrace"));
    }

    #[test]
    fn test_build_ack_outbound_message_disables_parse_mode() {
        let message = ChatDispatcher::build_ack_outbound_message("chat-1", "收到，开始处理。");
        assert_eq!(message.conversation_id, "chat-1");
        assert_eq!(message.content, "收到，开始处理。");
        assert_eq!(message.parse_mode, None);
    }

    #[tokio::test]
    async fn test_session_manager_creates_session() {
        let (storage, _temp_dir) = create_test_storage();

        // Create a test agent first
        use crate::models::AgentNode;
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let manager = ChatSessionManager::new(storage.clone(), 20).with_default_agent(agent_id);

        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-1", "user-1")
            .await
            .unwrap();
        assert_eq!(session.name, "conv-1");
        assert_eq!(session.source_channel, Some(ChatSessionSource::Telegram));
        assert_eq!(session.source_conversation_id.as_deref(), Some("conv-1"));
        let binding = storage
            .channel_session_bindings
            .get_by_route("telegram", None, "conv-1")
            .unwrap()
            .expect("channel binding should exist");
        assert_eq!(binding.session_id, session.id);

        // Getting again should return same session
        let session2 = manager
            .get_or_create_session(ChannelType::Telegram, "conv-1", "user-1")
            .await
            .unwrap();
        assert_eq!(session.id, session2.id);
    }

    #[tokio::test]
    async fn test_session_manager_recovers_from_stale_binding() {
        let (storage, _temp_dir) = create_test_storage();

        use crate::models::{AgentNode, ChannelSessionBinding};
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let stale = ChannelSessionBinding::new("telegram", None, "conv-stale", "missing-session");
        storage.channel_session_bindings.upsert(&stale).unwrap();

        let manager = ChatSessionManager::new(storage.clone(), 20).with_default_agent(agent_id);
        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-stale", "user-1")
            .await
            .unwrap();
        assert_ne!(session.id, "missing-session");

        let rebound = storage
            .channel_session_bindings
            .get_by_route("telegram", None, "conv-stale")
            .unwrap()
            .expect("binding should be replaced");
        assert_eq!(rebound.session_id, session.id);
    }

    #[tokio::test]
    async fn test_session_manager_forces_existing_channel_session_to_default_agent() {
        let (storage, _temp_dir) = create_test_storage();
        let _env_lock = crate::prompt_files::agents_dir_env_lock();
        let agents_dir = _temp_dir.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        unsafe { std::env::set_var("RESTFLOW_AGENTS_DIR", &agents_dir) };

        use crate::models::AgentNode;
        let non_default = storage
            .agents
            .create_agent(
                "Issue Finder Agent".to_string(),
                AgentNode::with_model(ModelId::CodexCli),
            )
            .unwrap();
        let default_agent = storage
            .agents
            .create_agent(
                crate::storage::agent::DEFAULT_ASSISTANT_NAME.to_string(),
                AgentNode::with_model(ModelId::ClaudeSonnet4_5),
            )
            .unwrap();

        let stale = ChatSession::new(
            non_default.id.clone(),
            ModelId::CodexCli.as_str().to_string(),
        )
        .with_name("conv-rebind")
        .with_source(ChatSessionSource::Telegram, "conv-rebind");
        storage.chat_sessions.create(&stale).unwrap();

        let manager = ChatSessionManager::new(storage.clone(), 20)
            .with_default_agent(default_agent.id.clone());
        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-rebind", "user-1")
            .await
            .unwrap();

        assert_eq!(session.id, stale.id);
        assert_eq!(session.agent_id, default_agent.id);
        assert_eq!(session.model, ModelId::ClaudeSonnet4_5.as_str());
        assert_eq!(
            session.metadata.last_model.as_deref(),
            Some(ModelId::ClaudeSonnet4_5.as_str())
        );
        unsafe { std::env::remove_var("RESTFLOW_AGENTS_DIR") };
    }

    #[tokio::test]
    async fn test_session_manager_rebinds_external_legacy_source_channel() {
        let (storage, _temp_dir) = create_test_storage();

        use crate::models::AgentNode;
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let legacy = ChatSession::new(
            agent_id.clone(),
            ModelId::Gpt5.as_serialized_str().to_string(),
        )
        .with_name("conv-legacy")
        .with_source(ChatSessionSource::ExternalLegacy, "conv-legacy");
        storage.chat_sessions.create(&legacy).unwrap();

        let manager = ChatSessionManager::new(storage.clone(), 20).with_default_agent(agent_id);
        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-legacy", "user-1")
            .await
            .unwrap();

        assert_eq!(session.id, legacy.id);
        assert_eq!(session.source_channel, Some(ChatSessionSource::Telegram));
        assert_eq!(
            session.source_conversation_id.as_deref(),
            Some("conv-legacy")
        );

        let persisted = storage
            .chat_sessions
            .get(&legacy.id)
            .unwrap()
            .expect("session should exist");
        assert_eq!(persisted.source_channel, Some(ChatSessionSource::Telegram));
    }

    #[tokio::test]
    async fn test_session_manager_appends_exchange() {
        let (storage, _temp_dir) = create_test_storage();

        // Create a test agent first
        use crate::models::AgentNode;
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let manager = ChatSessionManager::new(storage.clone(), 20).with_default_agent(agent_id);

        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-1", "user-1")
            .await
            .unwrap();

        manager
            .append_exchange(&session.id, "Hello!", "Hi there!", None, None)
            .unwrap();

        let history = manager.get_history(&session.id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "Hello!");
        assert_eq!(history[1].content, "Hi there!");
    }

    #[tokio::test]
    async fn test_session_manager_appends_multiple_exchanges_sequentially() {
        let (storage, _temp_dir) = create_test_storage();

        use crate::models::AgentNode;
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let manager = ChatSessionManager::new(storage.clone(), 20).with_default_agent(agent_id);

        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-prune", "user-1")
            .await
            .unwrap();

        manager
            .append_exchange(&session.id, "Hello!", "Hi there!", None, None)
            .unwrap();
        manager
            .append_exchange(&session.id, "How are you?", "Doing well.", None, None)
            .unwrap();

        let history = manager.get_history(&session.id).unwrap();
        assert_eq!(history.len(), 4);
        assert_eq!(history[0].content, "Hello!");
        assert_eq!(history[1].content, "Hi there!");
        assert_eq!(history[2].content, "How are you?");
        assert_eq!(history[3].content, "Doing well.");
    }

    #[tokio::test]
    async fn test_debouncer_integration() {
        let debouncer = Arc::new(MessageDebouncer::new(Duration::from_millis(50)));

        // First message should get the combined result
        let result = debouncer.debounce("conv-1", "Hello").await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_session_manager_updates_model_on_append() {
        let (storage, _temp_dir) = create_test_storage();

        use crate::models::AgentNode;
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let manager = ChatSessionManager::new(storage.clone(), 20).with_default_agent(agent_id);
        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-1", "user-1")
            .await
            .unwrap();

        manager
            .append_exchange(
                &session.id,
                "Hello!",
                "Switched to Codex.",
                Some("gpt-5.3-codex"),
                None,
            )
            .unwrap();

        let updated = storage
            .chat_sessions
            .get(&session.id)
            .unwrap()
            .expect("session should exist");
        // session.model should remain unchanged (user's chosen model)
        assert_eq!(updated.model, "gpt-5");
        // only last_model metadata tracks what was actually used
        assert_eq!(
            updated.metadata.last_model.as_deref(),
            Some("gpt-5.3-codex")
        );
    }

    #[tokio::test]
    async fn test_session_manager_does_not_truncate_stored_history() {
        let (storage, _temp_dir) = create_test_storage();

        use crate::models::AgentNode;
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let manager = ChatSessionManager::new(storage.clone(), 2).with_default_agent(agent_id);
        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-1", "user-1")
            .await
            .unwrap();

        manager
            .append_exchange(&session.id, "u1", "a1", None, None)
            .unwrap();
        manager
            .append_exchange(&session.id, "u2", "a2", None, None)
            .unwrap();
        manager
            .append_exchange(&session.id, "u3", "a3", None, None)
            .unwrap();

        let history = manager.get_history(&session.id).unwrap();
        assert_eq!(history.len(), 6);
        assert_eq!(history[0].content, "u1");
        assert_eq!(history[5].content, "a3");
    }

    #[tokio::test]
    async fn test_session_manager_rebinds_agent() {
        let (storage, _temp_dir) = create_test_storage();

        use crate::models::AgentNode;
        let stale = storage
            .agents
            .create_agent(
                "Stale Agent".to_string(),
                AgentNode::with_model(ModelId::ClaudeSonnet4_5),
            )
            .unwrap();
        let fallback = storage
            .agents
            .create_agent(
                "Fallback Agent".to_string(),
                AgentNode::with_model(ModelId::CodexCli),
            )
            .unwrap();

        let manager = ChatSessionManager::new(storage.clone(), 20).with_default_agent(stale.id);
        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-1", "user-1")
            .await
            .unwrap();

        let rebound = manager
            .rebind_session_agent(&session.id, &fallback.id)
            .expect("session should rebind");

        assert_eq!(rebound.agent_id, fallback.id);
        assert_eq!(rebound.model, ModelId::CodexCli.as_str());
        assert_eq!(
            rebound.metadata.last_model.as_deref(),
            Some(ModelId::CodexCli.as_str())
        );
    }

    #[test]
    fn test_model_specs_include_codex_entries() {
        let specs = ModelId::build_model_specs();

        assert!(
            specs
                .iter()
                .any(|spec| spec.name == "gpt-5.3-codex" && spec.is_codex_cli)
        );
    }

    #[test]
    fn test_main_agent_default_tools_include_switch_model() {
        let tools = crate::runtime::agent::main_agent_default_tool_names();

        assert!(tools.iter().any(|name| name == "switch_model"));
        assert!(tools.iter().any(|name| name == "manage_background_agents"));
        assert!(tools.iter().any(|name| name == "bash"));
    }

    #[test]
    fn test_effective_main_agent_tool_names_merges_extra_tools() {
        let extra = vec!["custom_tool".to_string(), "bash".to_string()];
        let merged = effective_main_agent_tool_names(Some(&extra));

        assert!(merged.iter().any(|name| name == "switch_model"));
        assert!(merged.iter().any(|name| name == "manage_background_agents"));
        assert!(merged.iter().any(|name| name == "custom_tool"));
        assert_eq!(
            merged.iter().filter(|name| name.as_str() == "bash").count(),
            1
        );
    }
}
