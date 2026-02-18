//! Compatibility wrapper around the unified AgentExecutor core.
//!
//! This module preserves the legacy `AgentExecutionEngine` API so upstream
//! callers do not need to migrate immediately. Internally, all execution now
//! delegates to `AgentExecutor` to guarantee a single ReAct implementation.

use super::executor::{AgentConfig, AgentExecutor, AgentResult};
use super::react::{ConversationHistory, ReActConfig};
use super::resource::{ResourceLimits, ResourceUsage};
use super::state::AgentState;
use super::stream::StreamEmitter;
use super::stuck::StuckDetectorConfig;
use crate::LlmClient;
use crate::error::Result;
use crate::llm::Message;
use crate::steer::SteerMessage;
use crate::tools::ToolRegistry;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Configuration for the compatibility execution engine.
#[derive(Debug, Clone)]
pub struct AgentExecutionEngineConfig {
    pub react: ReActConfig,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_history: usize,
    pub resource_limits: ResourceLimits,
    /// Optional stuck detection configuration.
    pub stuck_detection: Option<StuckDetectorConfig>,
}

impl Default for AgentExecutionEngineConfig {
    fn default() -> Self {
        Self {
            react: ReActConfig::default(),
            max_tokens: 4096,
            temperature: 0.7,
            max_history: 20,
            resource_limits: ResourceLimits::default(),
            stuck_detection: Some(StuckDetectorConfig::default()),
        }
    }
}

/// Result of agent execution.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub output: String,
    pub messages: Vec<Message>,
    pub success: bool,
    pub iterations: usize,
    pub resource_usage: ResourceUsage,
}

/// Legacy API wrapper that delegates to `AgentExecutor`.
pub struct AgentExecutionEngine {
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    system_prompt: String,
    config: AgentExecutionEngineConfig,
    history: ConversationHistory,
    steer_rx: Option<mpsc::Receiver<SteerMessage>>,
}

impl AgentExecutionEngine {
    pub fn new(
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<ToolRegistry>,
        system_prompt: String,
        config: AgentExecutionEngineConfig,
    ) -> Self {
        Self {
            llm_client,
            tool_registry,
            system_prompt,
            history: ConversationHistory::new(config.max_history.max(1)),
            config,
            steer_rx: None,
        }
    }

    /// Add a message to legacy history.
    pub fn add_history_message(&mut self, message: Message) {
        self.history.add(message);
    }

    /// Seed the engine with initial history.
    pub fn with_history(mut self, messages: Vec<Message>) -> Self {
        for message in messages {
            self.history.add(message);
        }
        self
    }

    /// Attach a steer channel.
    pub fn with_steer_channel(mut self, rx: mpsc::Receiver<SteerMessage>) -> Self {
        self.steer_rx = Some(rx);
        self
    }

    /// Execute without streaming.
    pub async fn execute(&mut self, input: &str) -> Result<ExecutionResult> {
        let config = self.build_agent_config(input);
        let executor = self.build_executor();
        let result = if let Some(state) = self.build_seed_state(input) {
            executor.run_from_state(config, state).await?
        } else {
            executor.run(config).await?
        };
        self.update_history(&result.state.messages);
        Ok(Self::convert_result(result))
    }

    /// Execute with streaming events.
    pub async fn execute_streaming(
        &mut self,
        input: &str,
        emitter: &mut dyn StreamEmitter,
    ) -> Result<ExecutionResult> {
        let config = self.build_agent_config(input);
        let executor = self.build_executor();
        let result = if let Some(state) = self.build_seed_state(input) {
            executor.execute_from_state(config, state, emitter).await?
        } else {
            #[allow(deprecated)]
            {
                executor.execute_streaming(config, emitter).await?
            }
        };
        self.update_history(&result.state.messages);
        Ok(Self::convert_result(result))
    }

    fn build_executor(&mut self) -> AgentExecutor {
        let mut executor = AgentExecutor::new(self.llm_client.clone(), self.tool_registry.clone());
        if let Some(rx) = self.steer_rx.take() {
            executor = executor.with_steer_channel(rx);
        }
        executor
    }

    fn build_agent_config(&self, input: &str) -> AgentConfig {
        let mut config = AgentConfig::new(input.to_string())
            .with_system_prompt(self.system_prompt.clone())
            .with_max_iterations(self.config.react.max_iterations)
            .with_max_memory_messages(self.config.max_history.max(1))
            .with_resource_limits(self.config.resource_limits.clone())
            .with_temperature(self.config.temperature);

        if self.config.max_tokens > 0 {
            config = config.with_max_output_tokens(self.config.max_tokens);
        }

        if let Some(stuck) = self.config.stuck_detection.clone() {
            config = config.with_stuck_detection(stuck);
        } else {
            config = config.without_stuck_detection();
        }

        config
    }

    fn build_seed_state(&self, input: &str) -> Option<AgentState> {
        let history = self.history.messages();
        if history.is_empty() {
            return None;
        }

        let mut state = AgentState::new(
            uuid::Uuid::new_v4().to_string(),
            self.config.react.max_iterations,
        );
        state.add_message(Message::system(self.system_prompt.clone()));
        for message in history {
            state.add_message(message.clone());
        }
        state.add_message(Message::user(input.to_string()));
        Some(state)
    }

    fn update_history(&mut self, messages: &[Message]) {
        let mut history = ConversationHistory::new(self.config.max_history.max(1));
        for message in messages {
            history.add(message.clone());
        }
        self.history = history;
    }

    fn convert_result(result: AgentResult) -> ExecutionResult {
        let AgentResult {
            success,
            answer,
            error,
            iterations,
            state,
            resource_usage,
            ..
        } = result;

        let output = if success {
            answer.unwrap_or_default()
        } else {
            error.unwrap_or_else(|| "Agent execution failed".to_string())
        };

        ExecutionResult {
            output,
            messages: state.messages,
            success,
            iterations,
            resource_usage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CompletionRequest, CompletionResponse, FinishReason, LlmClient, Message, StreamChunk,
        StreamResult,
    };
    use crate::tools::ToolRegistry;
    use async_trait::async_trait;
    use futures::stream;
    use std::sync::Mutex;

    struct MockLlmClient {
        responses: Mutex<Vec<CompletionResponse>>,
    }

    impl MockLlmClient {
        fn new(responses: Vec<CompletionResponse>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl LlmClient for MockLlmClient {
        fn provider(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-model"
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
            let mut responses = self.responses.lock().expect("responses lock poisoned");
            Ok(responses.remove(0))
        }

        fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
            Box::pin(stream::empty::<Result<StreamChunk>>())
        }

        fn supports_streaming(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn compatibility_engine_executes_via_unified_executor() {
        let llm = Arc::new(MockLlmClient::new(vec![CompletionResponse {
            content: Some("done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        }]));
        let tools = Arc::new(ToolRegistry::new());
        let mut engine = AgentExecutionEngine::new(
            llm,
            tools,
            "You are a test assistant".to_string(),
            AgentExecutionEngineConfig::default(),
        );

        let result = engine.execute("hello").await.expect("execution failed");
        assert!(result.success);
        assert_eq!(result.output, "done");
        assert!(
            result
                .messages
                .iter()
                .any(|m| m.role == crate::llm::Role::Assistant && m.content == "done")
        );
    }

    #[tokio::test]
    async fn compatibility_engine_preserves_seeded_history() {
        let llm = Arc::new(MockLlmClient::new(vec![CompletionResponse {
            content: Some("ok".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        }]));
        let tools = Arc::new(ToolRegistry::new());
        let mut engine = AgentExecutionEngine::new(
            llm,
            tools,
            "You are a test assistant".to_string(),
            AgentExecutionEngineConfig::default(),
        )
        .with_history(vec![Message::assistant("previous answer")]);

        let result = engine
            .execute("next question")
            .await
            .expect("execution failed");
        assert!(
            result
                .messages
                .iter()
                .any(|m| m.role == crate::llm::Role::Assistant && m.content == "previous answer")
        );
    }
}
