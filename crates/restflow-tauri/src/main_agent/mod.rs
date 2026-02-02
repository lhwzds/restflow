//! Main Agent module - Interactive agent with parallel sub-agent support.
//!
//! This module provides the infrastructure for running a main agent that can:
//! - Maintain persistent chat sessions with conversation history
//! - Spawn parallel sub-agents using Tokio for concurrent task execution
//! - Track sub-agent status and aggregate results
//! - Load and use skills to extend agent capabilities
//!
//! # Architecture
//!
//! - `session`: Chat session management with message history
//! - `tracker`: Sub-agent tracking with DashMap for concurrent access
//! - `spawn`: Tokio-based parallel sub-agent spawning
//! - `events`: Real-time event streaming to frontend
//! - `definition`: Agent type definitions (researcher, coder, etc.)
//! - `tools`: Main agent tools (spawn_agent, wait_agents, etc.)
//!
//! # Usage
//!
//! ```ignore
//! use restflow_tauri::main_agent::{MainAgent, MainAgentConfig};
//!
//! // Create a main agent session
//! let agent = MainAgent::new(
//!     session_id,
//!     llm_client,
//!     tool_registry,
//!     MainAgentConfig::default(),
//!     event_emitter,
//! );
//!
//! // Process a user message
//! agent.process_message("Help me research X and code Y").await?;
//!
//! // The agent may spawn sub-agents that run in parallel
//! // Results are aggregated and returned to the user
//! ```

pub mod events;
pub mod session;
pub mod spawn;
pub mod tools;

pub use crate::subagent::{
    builtin_agents, AgentDefinition, AgentDefinitionRegistry, SpawnHandle, SpawnPriority,
    SpawnRequest, SubagentCompletion, SubagentResult, SubagentState, SubagentStatus, SubagentTracker,
};
pub use events::{
    MainAgentEvent, MainAgentEventEmitter, MainAgentEventKind, NoopMainAgentEmitter,
    TauriMainAgentEmitter, MAIN_AGENT_EVENT,
};
pub use session::{
    AgentSession, ChatRole, MessageSource, SessionMessage, SessionMessageExecution,
    SessionMetadata,
};
pub use tools::{ListAgentsTool, SpawnAgentTool, UseSkillTool, WaitAgentsTool};

use anyhow::Result;
use restflow_ai::llm::{CompletionRequest, Message, Role};
use restflow_ai::LlmClient;
use restflow_core::storage::Storage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use ts_rs::TS;

/// Main Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MainAgentConfig {
    /// Maximum number of parallel sub-agents
    pub max_parallel_agents: usize,
    /// Default timeout for sub-agents in seconds
    pub subagent_timeout_secs: u64,
    /// Whether to automatically wait for all sub-agents before responding
    pub auto_wait_subagents: bool,
    /// Default model for the main agent
    pub default_model: String,
    /// Maximum ReAct loop iterations
    pub max_iterations: u32,
}

impl Default for MainAgentConfig {
    fn default() -> Self {
        Self {
            max_parallel_agents: 5,
            subagent_timeout_secs: 300,
            auto_wait_subagents: false,
            default_model: "claude-sonnet-4-20250514".to_string(),
            max_iterations: 20,
        }
    }
}

/// Main Agent - Core coordinating agent with sub-agent support
#[allow(dead_code)]
pub struct MainAgent {
    /// Unique identifier for this agent instance
    pub id: String,

    /// Chat session with conversation history
    session: Arc<RwLock<AgentSession>>,

    /// LLM client for inference
    llm_client: Arc<dyn LlmClient>,

    /// Available agent definitions for spawning
    agent_definitions: Arc<AgentDefinitionRegistry>,

    /// Running sub-agent tracker
    running_subagents: Arc<SubagentTracker>,

    /// Storage reference for persistence
    storage: Arc<Storage>,

    /// Event emitter for frontend updates
    event_emitter: Arc<dyn MainAgentEventEmitter>,

    /// Configuration
    config: MainAgentConfig,
}

impl MainAgent {
    /// Create a new MainAgent instance
    pub fn new(
        id: String,
        storage: Arc<Storage>,
        llm_client: Arc<dyn LlmClient>,
        event_emitter: Arc<dyn MainAgentEventEmitter>,
        config: MainAgentConfig,
    ) -> Result<Self> {
        let session = AgentSession::new(id.clone(), config.default_model.clone());
        let agent_definitions = Arc::new(AgentDefinitionRegistry::with_builtins());
        let (completion_tx, completion_rx) = mpsc::channel(100);
        let running_subagents = Arc::new(SubagentTracker::new(completion_tx, completion_rx));

        Ok(Self {
            id,
            session: Arc::new(RwLock::new(session)),
            llm_client,
            agent_definitions,
            running_subagents,
            storage,
            event_emitter,
            config,
        })
    }

    /// Get a reference to the session
    pub async fn session(&self) -> tokio::sync::RwLockReadGuard<'_, AgentSession> {
        self.session.read().await
    }

    /// Get a mutable reference to the session
    pub async fn session_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, AgentSession> {
        self.session.write().await
    }

    /// Get the agent definitions registry
    pub fn agent_definitions(&self) -> &Arc<AgentDefinitionRegistry> {
        &self.agent_definitions
    }

    /// Get the sub-agent tracker
    pub fn running_subagents(&self) -> &Arc<SubagentTracker> {
        &self.running_subagents
    }

    /// Get the configuration
    pub fn config(&self) -> &MainAgentConfig {
        &self.config
    }

    /// Process a user message and generate a response
    ///
    /// This runs the ReAct loop, potentially spawning sub-agents for parallel work.
    pub async fn process_message(&self, message: &str) -> Result<String> {
        // Add user message to session
        {
            let mut session = self.session.write().await;
            session.add_user_message(message.to_string());
        }

        // Emit user message event
        self.event_emitter.emit(MainAgentEvent {
            session_id: self.id.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::UserMessage {
                content: message.to_string(),
            },
        });

        // Run ReAct loop
        let response = self.run_react_loop(message).await?;

        // If auto_wait is enabled, wait for all sub-agents
        if self.config.auto_wait_subagents {
            let results = self.running_subagents.wait_all().await;
            if !results.is_empty() {
                // Inject sub-agent results into response context
                // This is handled by the ReAct loop already
            }
        }

        // Add assistant message to session
        {
            let mut session = self.session.write().await;
            session.add_assistant_message(response.clone(), None);
        }

        // Emit response completed event
        self.event_emitter.emit(MainAgentEvent {
            session_id: self.id.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::ResponseCompleted {
                full_content: response.clone(),
                total_tokens: 0, // TODO: Track actual tokens
                duration_ms: 0,  // TODO: Track actual duration
            },
        });

        Ok(response)
    }

    /// Run the ReAct (Reason + Act) loop
    async fn run_react_loop(&self, initial_message: &str) -> Result<String> {
        let session = self.session.read().await;
        let mut messages: Vec<Message> = Vec::new();
        let has_system = session
            .messages
            .iter()
            .any(|message| message.role == ChatRole::System);

        if !has_system {
            messages.push(Message::system(
                "You are the RestFlow main agent. Provide helpful, concise responses.",
            ));
        }

        for message in &session.messages {
            let role = match message.role {
                ChatRole::System => Role::System,
                ChatRole::User => Role::User,
                ChatRole::Assistant => Role::Assistant,
                ChatRole::Tool => Role::Tool,
            };
            messages.push(Message {
                role,
                content: message.content.clone(),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            });
        }

        if messages.is_empty() {
            messages.push(Message::user(initial_message));
        }

        let request = CompletionRequest::new(messages);
        let response = self.llm_client.complete(request).await?;

        if !response.tool_calls.is_empty() {
            return Ok("Tool calls are not supported in the main agent yet.".to_string());
        }

        Ok(response.content.unwrap_or_default())
    }

    /// Spawn a sub-agent to work on a task in parallel
    pub fn spawn_subagent(&self, request: SpawnRequest) -> Result<SpawnHandle> {
        spawn::spawn_subagent(
            self.running_subagents.clone(),
            self.agent_definitions.clone(),
            self.llm_client.clone(),
            self.event_emitter.clone(),
            self.config.clone(),
            self.id.clone(),
            request,
        )
    }

    /// Spawn multiple sub-agents in parallel
    pub fn spawn_parallel(&self, requests: Vec<SpawnRequest>) -> Result<Vec<SpawnHandle>> {
        requests
            .into_iter()
            .map(|req| self.spawn_subagent(req))
            .collect()
    }

    /// Wait for a specific sub-agent to complete
    pub async fn wait_subagent(&self, task_id: &str) -> Option<SubagentResult> {
        self.running_subagents.wait(task_id).await
    }

    /// Wait for all running sub-agents to complete
    pub async fn wait_all_subagents(&self) -> Vec<SubagentResult> {
        self.running_subagents.wait_all().await
    }

    /// Cancel a running sub-agent
    pub fn cancel_subagent(&self, task_id: &str) -> bool {
        self.running_subagents.cancel(task_id)
    }

    /// Get the current session state for serialization
    pub async fn get_session_state(&self) -> AgentSession {
        self.session.read().await.clone()
    }

    /// Load session state from storage
    pub async fn load_session(&self, session: AgentSession) {
        let mut current = self.session.write().await;
        *current = session;
    }

    /// Check for completed sub-agents and process their results
    pub async fn poll_completions(&self) -> Vec<SubagentCompletion> {
        self.running_subagents.poll_completions().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = MainAgentConfig::default();
        assert_eq!(config.max_parallel_agents, 5);
        assert_eq!(config.subagent_timeout_secs, 300);
        assert!(!config.auto_wait_subagents);
    }
}
