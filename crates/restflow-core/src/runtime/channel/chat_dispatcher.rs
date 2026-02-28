//! Chat Dispatcher - Handles natural language messages via AI agent.
//!
//! When a user sends a natural language message (not a command), the
//! ChatDispatcher processes it through an AI agent and returns the response.

use anyhow::{Result, anyhow};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, error, info, warn};

use crate::auth::AuthProfileManager;
use crate::channel::{
    ChannelReplySender, ChannelRouter, ChannelType, InboundMessage, OutboundMessage,
};
use crate::daemon::session_events::{ChatSessionEvent, publish_session_event};
use crate::models::{AIModel, ChatMessage, ChatSession, ChatSessionSource};
use crate::process::ProcessRegistry;
use crate::runtime::background_agent::{AgentRuntimeExecutor, SessionInputMode};
use crate::runtime::channel::{
    ToolTraceEmitter, append_turn_completed, append_turn_failed, append_turn_started,
};
use crate::runtime::output::{ensure_success_output, format_error_output};
use crate::storage::Storage;

use super::debounce::MessageDebouncer;
use crate::runtime::subagent::AgentDefinitionRegistry;
use restflow_ai::agent::{NullEmitter, StreamEmitter, SubagentConfig, SubagentTracker};

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
            max_session_history: 20,
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
            Self::ExecutionFailed(_) | Self::SessionError(_) => {
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
            Self::NoApiKey { provider } => write!(f, "No API key for provider: {}", provider),
            Self::RateLimited => write!(f, "Rate limited"),
            Self::Timeout => write!(f, "Response timeout"),
        }
    }
}

impl std::error::Error for ChatError {}

/// Chat session manager for conversation persistence.
pub struct ChatSessionManager {
    storage: Arc<Storage>,
    /// Mutex to serialize session creation and prevent duplicate sessions
    /// from being created under concurrent requests for the same conversation.
    session_creation_mutex: TokioMutex<()>,
    default_agent_id: Option<String>,
    max_history: usize,
    /// Mutex to serialize append_exchange operations per session.
    /// This prevents lost update races when concurrent messages are appended
    /// to the same session (e.g., from multiple Telegram updates or parallel handlers).
    append_locks: Arc<Mutex<std::collections::HashMap<String, Arc<Mutex<()>>>>>,
}

impl ChatSessionManager {
    /// Create a new ChatSessionManager.
    pub fn new(storage: Arc<Storage>, max_history: usize) -> Self {
        Self {
            storage,
            session_creation_mutex: TokioMutex::new(()),
            default_agent_id: None,
            max_history,
            append_locks: Arc::new(Mutex::new(std::collections::HashMap::new())),
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

        // Use mutex to serialize session creation and prevent race conditions.
        // This ensures that under concurrent requests for the same conversation,
        // only one session will be created.
        let _guard = self.session_creation_mutex.lock().await;

        // Re-check after acquiring lock (another task may have created it)
        let sessions = self.storage.chat_sessions.list()?;
        if let Some(session) = sessions
            .iter()
            .find(|s| {
                s.source_channel == source_channel
                    && s.source_conversation_id.as_deref() == Some(conversation_id)
            })
            .cloned()
        {
            let mut session = session;
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
            // Check if another request created the session while we were creating ours
            let sessions = self.storage.chat_sessions.list()?;
            if let Some(existing) = sessions.into_iter().find(|s| {
                s.source_channel == source_channel
                    && s.source_conversation_id.as_deref() == Some(conversation_id)
            }) {
                debug!(
                    "Session {} was created by another request, using existing",
                    existing.id
                );
                return Ok(existing);
            }
            // It's a real error, propagate it
            return Err(e);
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
    ) -> Result<()> {
        // Get or create a per-session lock to serialize append operations.
        // This ensures atomic read-modify-write for each session.
        let session_lock = {
            let mut locks = self.append_locks.lock();
            locks
                .entry(session_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        // Lock this specific session for the duration of read-modify-write
        let _guard = session_lock.lock();

        // Re-fetch the session inside the lock to get the latest state
        let mut session = self
            .storage
            .chat_sessions
            .get(session_id)?
            .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

        // Add user message
        session.add_message(ChatMessage::user(user_message));

        // Add assistant message
        session.add_message(ChatMessage::assistant(assistant_message));

        if let Some(model) = active_model
            && let Some(normalized) = AIModel::normalize_model_id(model)
        {
            // Only update last_model metadata; preserve the user's chosen session model
            // so that switch_model calls during execution don't permanently override it.
            session.metadata.last_model = Some(normalized);
        }

        debug!(
            "Persisted full history for session {} (stored messages: {}, runtime window: {})",
            session_id,
            session.messages.len(),
            self.max_history
        );

        self.storage.chat_sessions.save(&session)?;

        publish_session_event(ChatSessionEvent::MessageAdded {
            session_id: session_id.to_string(),
            source: "channel".to_string(),
        });

        Ok(())
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
            .unwrap_or_else(|| AIModel::Gpt5.as_serialized_str().to_string()))
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
    subagent_definitions: Arc<AgentDefinitionRegistry>,
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
        subagent_definitions: Arc<AgentDefinitionRegistry>,
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

    fn create_executor(&self) -> AgentRuntimeExecutor {
        AgentRuntimeExecutor::new(
            self.storage.clone(),
            Arc::new(ProcessRegistry::new()),
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

    /// Build the effective agent input from an inbound message.
    ///
    /// For voice messages, we attach a media context block so the main agent
    /// can call the `transcribe` tool with a concrete local file path.
    fn build_agent_input(message: &InboundMessage, file_path_override: Option<&str>) -> String {
        let Some(metadata) = message.metadata.as_ref() else {
            return message.content.clone();
        };

        let media_type = metadata.get("media_type").and_then(|value| value.as_str());
        let file_path = file_path_override
            .or_else(|| metadata.get("file_path").and_then(|value| value.as_str()));

        match (media_type, file_path) {
            (Some("voice"), Some(path)) => format!(
                "{content}\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: {path}\ninstruction: Use the transcribe tool with this file_path before answering.",
                content = message.content,
                path = path
            ),
            _ => message.content.clone(),
        }
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
        // 1. Build initial agent input (before session is known)
        let initial_input = Self::build_agent_input(message, None);

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
            // Rebuild the agent input with the relocated file path
            Self::build_agent_input(message, relocated_path.as_deref())
        } else {
            debounced
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
        let turn_id = uuid::Uuid::new_v4().to_string();
        append_turn_started(&self.storage.tool_traces, &session.id, &turn_id);
        let mut tool_event_emitter = Some(Box::new(ToolTraceEmitter::new(
            Box::new(NullEmitter),
            self.storage.tool_traces.clone(),
            session.id.clone(),
            turn_id.clone(),
        )) as Box<dyn StreamEmitter>);
        let execution_result = if let Some(timeout_secs) = self.config.response_timeout_secs {
            match tokio::time::timeout(
                tokio::time::Duration::from_secs(timeout_secs),
                executor.execute_session_turn_with_emitter(
                    &mut session,
                    &input,
                    self.config.max_session_history,
                    SessionInputMode::EphemeralInput,
                    tool_event_emitter.take(),
                ),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    append_turn_failed(
                        &self.storage.tool_traces,
                        &session.id,
                        &turn_id,
                        &format!("execution timed out after {} seconds", timeout_secs),
                    );
                    error!("Agent execution timed out after {} seconds", timeout_secs);
                    self.send_error_response(message, ChatError::Timeout)
                        .await?;
                    return Ok(());
                }
            }
        } else {
            executor
                .execute_session_turn_with_emitter(
                    &mut session,
                    &input,
                    self.config.max_session_history,
                    SessionInputMode::EphemeralInput,
                    tool_event_emitter.take(),
                )
                .await
        };

        let exec_result = match execution_result {
            Ok(result) => result,
            Err(error) => {
                append_turn_failed(
                    &self.storage.tool_traces,
                    &session.id,
                    &turn_id,
                    &error.to_string(),
                );
                error!("Agent execution failed: {}", error);
                self.send_error_response(message, Self::map_execution_error(&error))
                    .await?;
                return Ok(());
            }
        };
        append_turn_completed(&self.storage.tool_traces, &session.id, &turn_id);

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

        // 5. Save exchange to session
        if let Err(e) = self.sessions.append_exchange(
            &session.id,
            &message.content,
            &structured_output,
            Some(&exec_result.active_model),
        ) {
            warn!("Failed to save exchange to session: {}", e);
        } else {
            match self.storage.chat_sessions.get(&session.id) {
                Ok(Some(persisted_session)) => {
                    match crate::runtime::background_agent::persist::persist_chat_session_memory(
                        &self.storage.memory,
                        &persisted_session,
                    ) {
                        Ok(Some(result)) if result.chunk_count > 0 => {
                            debug!(
                                "Persisted {} memory chunks for channel session {}",
                                result.chunk_count, session.id
                            );
                        }
                        Ok(Some(_)) | Ok(None) => {}
                        Err(error) => {
                            warn!(
                                "Failed to persist memory for channel session {}: {}",
                                session.id, error
                            );
                        }
                    }
                }
                Ok(None) => {
                    warn!(
                        "Session {} disappeared before memory persistence",
                        session.id
                    );
                }
                Err(error) => {
                    warn!(
                        "Failed to reload session {} for memory persistence: {}",
                        session.id, error
                    );
                }
            }
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
    use crate::AIModel;
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
    fn test_build_agent_input_plain_text_unchanged() {
        let message = create_message("hello");
        let input = ChatDispatcher::build_agent_input(&message, None);
        assert_eq!(input, "hello");
    }

    #[test]
    fn test_build_agent_input_voice_includes_transcribe_hint() {
        let media_dir = crate::paths::media_dir().unwrap();
        let voice_path = media_dir.join("tg-voice.ogg");
        let voice_path_str = voice_path.to_string_lossy().to_string();

        let message = create_message("[Voice message, 6s]").with_metadata(json!({
            "media_type": "voice",
            "file_path": voice_path_str
        }));

        let input = ChatDispatcher::build_agent_input(&message, None);
        assert!(input.contains("media_type: voice"));
        assert!(input.contains(&format!("local_file_path: {}", voice_path_str)));
        assert!(input.contains("Use the transcribe tool with this file_path"));
    }

    #[test]
    fn test_build_agent_input_voice_without_file_path_keeps_original_content() {
        let message = create_message("[Voice message, 6s]").with_metadata(json!({
            "media_type": "voice"
        }));

        let input = ChatDispatcher::build_agent_input(&message, None);
        assert_eq!(input, "[Voice message, 6s]");
    }

    #[test]
    fn test_config_defaults() {
        let config = ChatDispatcherConfig::default();
        assert_eq!(config.max_session_history, 20);
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

        let manager = ChatSessionManager::new(storage, 20).with_default_agent(agent_id);

        let session = manager
            .get_or_create_session(ChannelType::Telegram, "conv-1", "user-1")
            .await
            .unwrap();
        assert_eq!(session.name, "conv-1");
        assert_eq!(session.source_channel, Some(ChatSessionSource::Telegram));
        assert_eq!(session.source_conversation_id.as_deref(), Some("conv-1"));

        // Getting again should return same session
        let session2 = manager
            .get_or_create_session(ChannelType::Telegram, "conv-1", "user-1")
            .await
            .unwrap();
        assert_eq!(session.id, session2.id);
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
                AgentNode::with_model(AIModel::CodexCli),
            )
            .unwrap();
        let default_agent = storage
            .agents
            .create_agent(
                crate::storage::agent::DEFAULT_ASSISTANT_NAME.to_string(),
                AgentNode::with_model(AIModel::ClaudeSonnet4_5),
            )
            .unwrap();

        let stale = ChatSession::new(
            non_default.id.clone(),
            AIModel::CodexCli.as_str().to_string(),
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
        assert_eq!(session.model, AIModel::ClaudeSonnet4_5.as_str());
        assert_eq!(
            session.metadata.last_model.as_deref(),
            Some(AIModel::ClaudeSonnet4_5.as_str())
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
            AIModel::Gpt5.as_serialized_str().to_string(),
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
            .append_exchange(&session.id, "Hello!", "Hi there!", None)
            .unwrap();

        let history = manager.get_history(&session.id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "Hello!");
        assert_eq!(history[1].content, "Hi there!");
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
            .append_exchange(&session.id, "u1", "a1", None)
            .unwrap();
        manager
            .append_exchange(&session.id, "u2", "a2", None)
            .unwrap();
        manager
            .append_exchange(&session.id, "u3", "a3", None)
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
                AgentNode::with_model(AIModel::ClaudeSonnet4_5),
            )
            .unwrap();
        let fallback = storage
            .agents
            .create_agent(
                "Fallback Agent".to_string(),
                AgentNode::with_model(AIModel::CodexCli),
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
        assert_eq!(rebound.model, AIModel::CodexCli.as_str());
        assert_eq!(
            rebound.metadata.last_model.as_deref(),
            Some(AIModel::CodexCli.as_str())
        );
    }

    #[test]
    fn test_model_specs_include_codex_entries() {
        let specs = AIModel::build_model_specs();

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
