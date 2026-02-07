//! Chat Dispatcher - Handles natural language messages via AI agent.
//!
//! When a user sends a natural language message (not a command), the
//! ChatDispatcher processes it through an AI agent and returns the response.

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::auth::AuthProfileManager;
use crate::channel::{ChannelRouter, InboundMessage, OutboundMessage};
use crate::models::{ApiKeyConfig, ChatMessage, ChatRole, ChatSession};
use crate::storage::Storage;
use crate::{AIModel, Provider};
use restflow_ai::llm::Message;
use restflow_ai::{
    DefaultLlmClientFactory, LlmClient, LlmClientFactory, LlmProvider, SwappableLlm,
    SwitchModelTool,
};

use super::debounce::MessageDebouncer;
use crate::runtime::agent::{
    SubagentDeps, ToolRegistry, UnifiedAgent, UnifiedAgentConfig, build_agent_system_prompt,
    effective_main_agent_tool_names, registry_from_allowlist, secret_resolver_from_storage,
};
use crate::runtime::subagent::{AgentDefinitionRegistry, SubagentConfig, SubagentTracker};

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
            response_timeout_secs: 300,
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

        let session = ChatSession::new(agent_id, model).with_name(session_name);

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
        active_model: Option<&str>,
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

        if let Some(model) = active_model {
            session.model = model.to_string();
            session.metadata.last_model = Some(model.to_string());
        }

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

        Ok(agent
            .agent
            .model
            .map(|m| m.as_str().to_string())
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

    /// Resolve API key for a model provider.
    async fn resolve_api_key(
        &self,
        provider: Provider,
        agent_api_key_config: Option<&ApiKeyConfig>,
    ) -> Result<String> {
        // First, check agent-level API key config
        if let Some(config) = agent_api_key_config {
            match config {
                ApiKeyConfig::Direct(key) => {
                    if !key.is_empty() {
                        return Ok(key.clone());
                    }
                }
                ApiKeyConfig::Secret(secret_name) => {
                    if let Some(secret_value) = self.storage.secrets.get_secret(secret_name)? {
                        return Ok(secret_value);
                    }
                    return Err(anyhow!("Secret '{}' not found", secret_name));
                }
            }
        }

        // Check auth profiles
        if let Some(profile) = self.auth_manager.get_credential_for_model(provider).await {
            return profile
                .get_api_key(self.auth_manager.resolver())
                .map_err(|e| anyhow!("{}", e));
        }

        // Fall back to well-known secret names
        let secret_name = provider.api_key_env();

        if let Some(secret_value) = self.storage.secrets.get_secret(secret_name)? {
            return Ok(secret_value);
        }

        Err(anyhow!("No API key configured for provider {:?}", provider))
    }

    /// Resolve API key, avoiding mismatched agent-level keys for fallback providers.
    async fn resolve_api_key_for_model(
        &self,
        provider: Provider,
        agent_api_key_config: Option<&ApiKeyConfig>,
        primary_provider: Provider,
    ) -> Result<String> {
        let config = if provider == primary_provider {
            agent_api_key_config
        } else {
            None
        };
        self.resolve_api_key(provider, config).await
    }

    async fn build_api_keys(
        &self,
        agent_api_key_config: Option<&ApiKeyConfig>,
        primary_provider: Provider,
    ) -> HashMap<LlmProvider, String> {
        let mut keys = HashMap::new();

        for provider in Provider::all() {
            if let Ok(key) = self
                .resolve_api_key_for_model(*provider, agent_api_key_config, primary_provider)
                .await
            {
                keys.insert(provider.as_llm_provider(), key);
            }
        }

        keys
    }

    fn switch_model_enabled(tool_names: Option<&[String]>) -> bool {
        tool_names
            .map(|names| names.iter().any(|name| name == "switch_model"))
            .unwrap_or(false)
    }

    fn build_subagent_deps(&self, llm_client: Arc<dyn LlmClient>) -> SubagentDeps {
        SubagentDeps {
            tracker: self.subagent_tracker.clone(),
            definitions: self.subagent_definitions.clone(),
            llm_client,
            tool_registry: Arc::new(ToolRegistry::new()),
            config: self.subagent_config.clone(),
        }
    }

    /// Convert a stored chat message into an LLM message.
    fn chat_message_to_llm_message(message: &ChatMessage) -> Message {
        match message.role {
            ChatRole::User => Message::user(message.content.clone()),
            ChatRole::Assistant => Message::assistant(message.content.clone()),
            ChatRole::System => Message::system(message.content.clone()),
        }
    }

    /// Build the effective agent input from an inbound message.
    ///
    /// For voice messages, we attach a media context block so the main agent
    /// can call the `transcribe` tool with a concrete local file path.
    fn build_agent_input(message: &InboundMessage) -> String {
        let Some(metadata) = message.metadata.as_ref() else {
            return message.content.clone();
        };

        let media_type = metadata.get("media_type").and_then(|value| value.as_str());
        let file_path = metadata.get("file_path").and_then(|value| value.as_str());

        match (media_type, file_path) {
            (Some("voice"), Some(path)) => format!(
                "{content}\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: {path}\ninstruction: Use the transcribe tool with this file_path before answering.",
                content = message.content,
                path = path
            ),
            _ => message.content.clone(),
        }
    }

    /// Dispatch a message to the AI agent.
    pub async fn dispatch(&self, message: &InboundMessage) -> Result<()> {
        let agent_input = Self::build_agent_input(message);

        // 1. Debounce messages
        let input = match self
            .debouncer
            .debounce(&message.conversation_id, &agent_input)
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

        // 2. Get or create session
        let session = match self
            .sessions
            .get_or_create_session(&message.conversation_id, &message.sender_id)
        {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get/create session: {}", e);
                self.send_error_response(message, ChatError::NoDefaultAgent)
                    .await?;
                return Ok(());
            }
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

        // 4. Load agent and create UnifiedAgent
        debug!("Loading agent: {}", session.agent_id);
        let stored_agent = match self.storage.agents.get_agent(session.agent_id.clone()) {
            Ok(Some(a)) => a,
            Ok(None) => {
                error!("Agent '{}' not found", session.agent_id);
                self.send_error_response(message, ChatError::NoDefaultAgent)
                    .await?;
                return Ok(());
            }
            Err(e) => {
                error!("Failed to load agent: {}", e);
                self.send_error_response(message, ChatError::ExecutionFailed(e.to_string()))
                    .await?;
                return Ok(());
            }
        };

        let agent_node = &stored_agent.agent;
        debug!("Getting model for agent");
        let model = match agent_node.require_model() {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to get model: {}", e);
                self.send_error_response(message, ChatError::ExecutionFailed(e.to_string()))
                    .await?;
                return Ok(());
            }
        };
        debug!(
            "Model: {} (provider: {:?})",
            model.as_str(),
            model.provider()
        );

        let primary_provider = model.provider();
        let model_specs = AIModel::build_model_specs();
        let api_keys = self
            .build_api_keys(agent_node.api_key_config.as_ref(), primary_provider)
            .await;
        let factory: Arc<dyn LlmClientFactory> =
            Arc::new(DefaultLlmClientFactory::new(api_keys, model_specs));

        debug!("Resolving API key for initial model");
        let api_key = if model.is_codex_cli() {
            None
        } else {
            match self
                .resolve_api_key_for_model(
                    model.provider(),
                    agent_node.api_key_config.as_ref(),
                    primary_provider,
                )
                .await
            {
                Ok(key) => Some(key),
                Err(e) => {
                    error!("Failed to resolve API key: {}", e);
                    self.send_error_response(
                        message,
                        ChatError::NoApiKey {
                            provider: format!("{:?}", model.provider()),
                        },
                    )
                    .await?;
                    return Ok(());
                }
            }
        };
        if let Some(ref key) = api_key {
            debug!(
                "API key resolved (starts with: {}...)",
                &key[..key.len().min(10)]
            );
        } else {
            debug!("No API key required for initial model");
        }

        debug!("Creating swappable LLM client");
        let llm_client = match factory.create_client(model.as_serialized_str(), api_key.as_deref())
        {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to create LLM client: {}", e);
                self.send_error_response(message, ChatError::ExecutionFailed(e.to_string()))
                    .await?;
                return Ok(());
            }
        };
        let swappable = Arc::new(SwappableLlm::new(llm_client));
        let subagent_deps = self.build_subagent_deps(swappable.clone());
        let secret_resolver = Some(secret_resolver_from_storage(&self.storage));
        let effective_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
        let mut tools = registry_from_allowlist(
            Some(&effective_tools),
            Some(&subagent_deps),
            secret_resolver,
            Some(self.storage.as_ref()),
        );
        if Self::switch_model_enabled(Some(&effective_tools)) {
            tools.register(SwitchModelTool::new(swappable.clone(), factory));
        }
        let tools = Arc::new(tools);
        let system_prompt = build_agent_system_prompt(self.storage.clone(), agent_node)
            .map_err(|e| ChatError::ExecutionFailed(e.to_string()))?;

        let mut config = UnifiedAgentConfig::default();
        if model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config.temperature = temp as f32;
        }

        let mut agent = UnifiedAgent::new(swappable.clone(), tools, system_prompt, config);

        // Add conversation history
        let history = self.sessions.get_history(&session.id).unwrap_or_default();
        let start = history
            .len()
            .saturating_sub(self.config.max_session_history);
        for msg in &history[start..] {
            agent.add_history_message(Self::chat_message_to_llm_message(msg));
        }

        // 5. Execute with timeout
        let result = match tokio::time::timeout(
            tokio::time::Duration::from_secs(self.config.response_timeout_secs),
            agent.execute(&input),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                error!("Agent execution failed: {}", e);
                self.send_error_response(message, ChatError::ExecutionFailed(e.to_string()))
                    .await?;
                return Ok(());
            }
            Err(_) => {
                error!("Agent execution timed out");
                self.send_error_response(message, ChatError::Timeout)
                    .await?;
                return Ok(());
            }
        };

        // 6. Save exchange to session
        let active_model = swappable.current_model();
        if let Err(e) = self.sessions.append_exchange(
            &session.id,
            &message.content,
            &result.output,
            Some(&active_model),
        ) {
            warn!("Failed to save exchange to session: {}", e);
        }

        // 7. Send response (plain message without emoji prefix for AI chat)
        let response = OutboundMessage::plain(&message.conversation_id, &result.output);
        self.channel_router
            .send_to(message.channel_type, response)
            .await?;

        info!(
            "Chat response sent for session {} (output length: {} chars)",
            session.id,
            result.output.len()
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
        let error_text = format!("⚠️ {}", error.user_message());
        let response = OutboundMessage::new(&message.conversation_id, &error_text);
        self.channel_router
            .send_to(message.channel_type, response)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::ChannelType;
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
        let input = ChatDispatcher::build_agent_input(&message);
        assert_eq!(input, "hello");
    }

    #[test]
    fn test_build_agent_input_voice_includes_transcribe_hint() {
        let message = create_message("[Voice message, 6s]").with_metadata(json!({
            "media_type": "voice",
            "file_path": "/tmp/restflow-media/tg-voice.ogg"
        }));

        let input = ChatDispatcher::build_agent_input(&message);
        assert!(input.contains("media_type: voice"));
        assert!(input.contains("local_file_path: /tmp/restflow-media/tg-voice.ogg"));
        assert!(input.contains("Use the transcribe tool with this file_path"));
    }

    #[test]
    fn test_build_agent_input_voice_without_file_path_keeps_original_content() {
        let message = create_message("[Voice message, 6s]").with_metadata(json!({
            "media_type": "voice"
        }));

        let input = ChatDispatcher::build_agent_input(&message);
        assert_eq!(input, "[Voice message, 6s]");
    }

    #[test]
    fn test_config_defaults() {
        let config = ChatDispatcherConfig::default();
        assert_eq!(config.max_session_history, 20);
        assert_eq!(config.response_timeout_secs, 300);
        assert!(config.send_typing_indicator);
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
        use crate::models::AgentNode;
        storage
            .agents
            .create_agent("Test Agent".to_string(), AgentNode::new())
            .unwrap();
        let agents = storage.agents.list_agents().unwrap();
        let agent_id = agents[0].id.clone();

        let manager = ChatSessionManager::new(storage.clone(), 20).with_default_agent(agent_id);

        let session = manager.get_or_create_session("conv-1", "user-1").unwrap();

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
        let session = manager.get_or_create_session("conv-1", "user-1").unwrap();

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
        assert_eq!(updated.model, "gpt-5.3-codex");
        assert_eq!(
            updated.metadata.last_model.as_deref(),
            Some("gpt-5.3-codex")
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
    fn test_switch_model_enabled_detection() {
        let enabled = vec!["bash".to_string(), "switch_model".to_string()];
        let disabled = vec!["bash".to_string(), "http".to_string()];

        assert!(ChatDispatcher::switch_model_enabled(Some(&enabled)));
        assert!(!ChatDispatcher::switch_model_enabled(Some(&disabled)));
        assert!(!ChatDispatcher::switch_model_enabled(None));
    }

    #[test]
    fn test_main_agent_default_tools_include_switch_model() {
        let tools = crate::runtime::agent::main_agent_default_tool_names();

        assert!(tools.iter().any(|name| name == "switch_model"));
        assert!(tools.iter().any(|name| name == "manage_tasks"));
        assert!(tools.iter().any(|name| name == "bash"));
    }

    #[test]
    fn test_effective_main_agent_tool_names_merges_extra_tools() {
        let extra = vec!["custom_tool".to_string(), "bash".to_string()];
        let merged = effective_main_agent_tool_names(Some(&extra));

        assert!(merged.iter().any(|name| name == "switch_model"));
        assert!(merged.iter().any(|name| name == "manage_tasks"));
        assert!(merged.iter().any(|name| name == "custom_tool"));
        assert_eq!(
            merged.iter().filter(|name| name.as_str() == "bash").count(),
            1
        );
    }
}
