//! Chat Dispatcher - Handles natural language messages via AI agent.
//!
//! When a user sends a natural language message (not a command), the
//! ChatDispatcher processes it through an AI agent and returns the response.

use anyhow::{Result, anyhow};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use restflow_core::channel::{ChannelRouter, InboundMessage, OutboundMessage};
use restflow_core::models::{ChatMessage, ChatSession};
use restflow_core::storage::Storage;

use super::debounce::MessageDebouncer;
use crate::agent_task::runner::AgentExecutor;

/// Configuration for the ChatDispatcher.
#[derive(Debug, Clone)]
pub struct ChatDispatcherConfig {
    /// Maximum number of messages to keep in session history.
    pub max_session_history: usize,
    /// AI response timeout in seconds.
    pub response_timeout_secs: u64,
    /// Whether to send typing indicator while processing.
    pub send_typing_indicator: bool,
    /// Default agent name to use when none is specified.
    pub default_agent_name: String,
}

impl Default for ChatDispatcherConfig {
    fn default() -> Self {
        Self {
            max_session_history: 20,
            response_timeout_secs: 60,
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

impl ChatError {
    /// Get a user-friendly error message.
    pub fn user_message(&self) -> &str {
        match self {
            Self::NoDefaultAgent => "No AI agent configured. Please set up a default agent in settings.",
            Self::NoApiKey { .. } => "API key not configured. Please add your API key in settings.",
            Self::RateLimited => "Too many requests. Please wait a moment and try again.",
            Self::Timeout => "AI response timed out. Please try again or simplify your question.",
            Self::ExecutionFailed(_) | Self::SessionError(_) => {
                "An error occurred while processing your message. Please try again."
            }
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
    default_agent_id: Option<String>,
    max_history: usize,
}

impl ChatSessionManager {
    /// Create a new ChatSessionManager.
    pub fn new(storage: Arc<Storage>, max_history: usize) -> Self {
        Self {
            storage,
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
    pub fn get_or_create_session(
        &self,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<ChatSession> {
        // Try to find existing session by conversation ID
        // We use a naming convention: "channel:{conversation_id}"
        let session_name = format!("channel:{}", conversation_id);

        // List sessions and find one with matching name
        let sessions = self.storage.chat_sessions.list()?;
        if let Some(session) = sessions.into_iter().find(|s| s.name == session_name) {
            return Ok(session);
        }

        // Create new session
        let agent_id = self.get_default_agent_id()?;
        let model = self.get_agent_model(&agent_id)?;

        let session = ChatSession::new(agent_id, model)
            .with_name(session_name);

        self.storage.chat_sessions.create(&session)?;
        
        info!(
            "Created new chat session {} for conversation {} (user: {})",
            session.id, conversation_id, user_id
        );

        Ok(session)
    }

    /// Append a user-assistant exchange to a session.
    pub fn append_exchange(
        &self,
        session_id: &str,
        user_message: &str,
        assistant_message: &str,
    ) -> Result<()> {
        let mut session = self
            .storage
            .chat_sessions
            .get(session_id)?
            .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

        // Add user message
        session.add_message(ChatMessage::user(user_message));

        // Add assistant message
        session.add_message(ChatMessage::assistant(assistant_message));

        // Trim history if needed
        if session.messages.len() > self.max_history * 2 {
            let keep_from = session.messages.len() - self.max_history * 2;
            session.messages = session.messages[keep_from..].to_vec();
            debug!(
                "Trimmed session {} history to {} messages",
                session_id,
                session.messages.len()
            );
        }

        self.storage.chat_sessions.save(&session)?;
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

    /// Get the default agent ID.
    fn get_default_agent_id(&self) -> Result<String> {
        if let Some(ref id) = self.default_agent_id {
            return Ok(id.clone());
        }

        // Try to find an agent named "default" or use the first available agent
        let agents = self.storage.agents.list_agents()?;
        
        if let Some(agent) = agents.iter().find(|a| a.name.to_lowercase() == "default") {
            return Ok(agent.id.clone());
        }

        if let Some(agent) = agents.first() {
            return Ok(agent.id.clone());
        }

        Err(anyhow!("No agents configured"))
    }

    /// Get the model for an agent.
    fn get_agent_model(&self, agent_id: &str) -> Result<String> {
        let agent = self
            .storage
            .agents
            .get_agent(agent_id.to_string())?
            .ok_or_else(|| anyhow!("Agent not found: {}", agent_id))?;

        Ok(agent.agent.model.map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string()))
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
    executor: Arc<dyn AgentExecutor>,
    debouncer: Arc<MessageDebouncer>,
    channel_router: Arc<ChannelRouter>,
    config: ChatDispatcherConfig,
}

impl ChatDispatcher {
    /// Create a new ChatDispatcher.
    pub fn new(
        sessions: Arc<ChatSessionManager>,
        executor: Arc<dyn AgentExecutor>,
        debouncer: Arc<MessageDebouncer>,
        channel_router: Arc<ChannelRouter>,
        config: ChatDispatcherConfig,
    ) -> Self {
        Self {
            sessions,
            executor,
            debouncer,
            channel_router,
            config,
        }
    }

    /// Dispatch a message to the AI agent.
    pub async fn dispatch(&self, message: &InboundMessage) -> Result<()> {
        // 1. Debounce messages
        let input = match self.debouncer.debounce(&message.conversation_id, &message.content).await {
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

        // 2. Get or create session
        let session = match self.sessions.get_or_create_session(
            &message.conversation_id,
            &message.sender_id,
        ) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get/create session: {}", e);
                self.send_error_response(message, ChatError::NoDefaultAgent).await?;
                return Ok(());
            }
        };

        info!(
            "Processing chat message for session {} (conversation: {})",
            session.id, message.conversation_id
        );

        // 3. Send typing indicator if enabled
        if self.config.send_typing_indicator
            && let Err(e) = self.send_typing_indicator(message).await
        {
            warn!("Failed to send typing indicator: {}", e);
        }

        // 4. Execute agent
        // Note: In a full implementation, we'd pass conversation history to the executor
        let result = match tokio::time::timeout(
            tokio::time::Duration::from_secs(self.config.response_timeout_secs),
            self.executor.execute(&session.agent_id, Some(&input)),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                error!("Agent execution failed: {}", e);
                let chat_error = if e.to_string().contains("API key") {
                    ChatError::NoApiKey {
                        provider: "unknown".to_string(),
                    }
                } else {
                    ChatError::ExecutionFailed(e.to_string())
                };
                self.send_error_response(message, chat_error).await?;
                return Ok(());
            }
            Err(_) => {
                error!("Agent execution timed out");
                self.send_error_response(message, ChatError::Timeout).await?;
                return Ok(());
            }
        };

        // 5. Save exchange to session
        if let Err(e) = self.sessions.append_exchange(&session.id, &input, &result.output) {
            warn!("Failed to save exchange to session: {}", e);
        }

        // 6. Send response (plain message without emoji prefix for AI chat)
        let response = OutboundMessage::plain(&message.conversation_id, &result.output);
        self.channel_router.send_to(message.channel_type, response).await?;

        info!(
            "Chat response sent for session {} (output length: {} chars)",
            session.id,
            result.output.len()
        );

        Ok(())
    }

    /// Send typing indicator to the conversation.
    async fn send_typing_indicator(&self, message: &InboundMessage) -> Result<()> {
        // Note: This would need channel-specific implementation
        // For now, we log it
        debug!(
            "Would send typing indicator to {} on {:?}",
            message.conversation_id, message.channel_type
        );
        Ok(())
    }

    /// Send an error response to the user.
    async fn send_error_response(
        &self,
        message: &InboundMessage,
        error: ChatError,
    ) -> Result<()> {
        let error_text = format!("⚠️ {}", error.user_message());
        let response = OutboundMessage::new(&message.conversation_id, &error_text);
        self.channel_router.send_to(message.channel_type, response).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_task::runner::ExecutionResult;
    use async_trait::async_trait;
    use restflow_core::channel::ChannelType;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tempfile::tempdir;
    use tokio::time::Duration;

    /// Mock executor for testing.
    #[allow(dead_code)]
    struct MockExecutor {
        call_count: AtomicU32,
        response: String,
    }

    #[allow(dead_code)]
    impl MockExecutor {
        fn new(response: impl Into<String>) -> Self {
            Self {
                call_count: AtomicU32::new(0),
                response: response.into(),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl AgentExecutor for MockExecutor {
        async fn execute(&self, _agent_id: &str, _input: Option<&str>) -> Result<ExecutionResult> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(ExecutionResult::success(self.response.clone(), Vec::new()))
        }
    }

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
    fn test_config_defaults() {
        let config = ChatDispatcherConfig::default();
        assert_eq!(config.max_session_history, 20);
        assert_eq!(config.response_timeout_secs, 60);
        assert!(config.send_typing_indicator);
    }

    #[test]
    fn test_chat_error_user_messages() {
        assert!(!ChatError::NoDefaultAgent.user_message().is_empty());
        assert!(!ChatError::NoApiKey { provider: "test".to_string() }
            .user_message()
            .is_empty());
        assert!(!ChatError::RateLimited.user_message().is_empty());
        assert!(!ChatError::Timeout.user_message().is_empty());
        assert!(!ChatError::ExecutionFailed("test".to_string())
            .user_message()
            .is_empty());
    }

    #[tokio::test]
    async fn test_session_manager_creates_session() {
        let (storage, _temp_dir) = create_test_storage();

        // Create a test agent first
        use restflow_core::models::AgentNode;
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let manager = ChatSessionManager::new(storage, 20).with_default_agent(agent_id);

        let session = manager.get_or_create_session("conv-1", "user-1").unwrap();
        assert_eq!(session.name, "channel:conv-1");

        // Getting again should return same session
        let session2 = manager.get_or_create_session("conv-1", "user-1").unwrap();
        assert_eq!(session.id, session2.id);
    }

    #[tokio::test]
    async fn test_session_manager_appends_exchange() {
        let (storage, _temp_dir) = create_test_storage();

        // Create a test agent first
        use restflow_core::models::AgentNode;
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let manager = ChatSessionManager::new(storage.clone(), 20).with_default_agent(agent_id);

        let session = manager.get_or_create_session("conv-1", "user-1").unwrap();

        manager
            .append_exchange(&session.id, "Hello!", "Hi there!")
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
}
