//! Agent executor with ReAct loop

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::agent::context::AgentContext;
use crate::agent::state::{AgentState, AgentStatus};
use crate::error::{AiError, Result};
use crate::llm::{CompletionRequest, FinishReason, LlmClient, Message};
use crate::memory::{DEFAULT_MAX_MESSAGES, WorkingMemory};
use crate::tools::ToolRegistry;

/// Configuration for agent execution
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub goal: String,
    pub system_prompt: Option<String>,
    pub max_iterations: usize,
    pub temperature: Option<f32>,
    /// Hidden context passed to tools but not shown to LLM (Swarm-inspired)
    pub context: HashMap<String, Value>,
    /// Timeout for each tool execution (default: 30s)
    pub tool_timeout: Duration,
    /// Max length for tool results to prevent context overflow (default: 4000)
    pub max_tool_result_length: usize,
    /// Maximum messages to retain in working memory (default: 100)
    /// When this limit is reached, oldest non-system messages are evicted.
    pub max_memory_messages: usize,
    /// Optional agent context injected into the system prompt.
    pub agent_context: Option<AgentContext>,
}

impl AgentConfig {
    /// Create a new agent config with a goal
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            system_prompt: None,
            max_iterations: 10,
            temperature: None, // None = use model default
            context: HashMap::new(),
            tool_timeout: Duration::from_secs(30),
            max_tool_result_length: 4000,
            max_memory_messages: DEFAULT_MAX_MESSAGES,
            agent_context: None,
        }
    }

    /// Set maximum messages in working memory
    pub fn with_max_memory_messages(mut self, max: usize) -> Self {
        self.max_memory_messages = max;
        self
    }

    /// Set custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set max iterations
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Add context variable
    pub fn with_context(mut self, key: impl Into<String>, value: Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    /// Set tool timeout
    pub fn with_tool_timeout(mut self, timeout: Duration) -> Self {
        self.tool_timeout = timeout;
        self
    }

    /// Set max tool result length
    pub fn with_max_tool_result_length(mut self, max: usize) -> Self {
        self.max_tool_result_length = max;
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set agent context for prompt injection
    pub fn with_agent_context(mut self, context: AgentContext) -> Self {
        self.agent_context = Some(context);
        self
    }
}

/// Result of agent execution
#[derive(Debug)]
pub struct AgentResult {
    pub success: bool,
    pub answer: Option<String>,
    pub error: Option<String>,
    pub iterations: usize,
    pub total_tokens: u32,
    pub state: AgentState,
}

/// Agent executor implementing Swarm-style ReAct loop
pub struct AgentExecutor {
    llm: Arc<dyn LlmClient>,
    tools: Arc<ToolRegistry>,
}

impl AgentExecutor {
    /// Create a new agent executor
    pub fn new(llm: Arc<dyn LlmClient>, tools: Arc<ToolRegistry>) -> Self {
        Self { llm, tools }
    }

    /// Execute agent - simplified Swarm-style loop
    pub async fn run(&self, config: AgentConfig) -> Result<AgentResult> {
        let execution_id = uuid::Uuid::new_v4().to_string();
        let mut state = AgentState::new(execution_id, config.max_iterations);
        state.context = config.context.clone();
        let mut total_tokens: u32 = 0;

        // Initialize working memory for context window management
        let mut memory = WorkingMemory::new(config.max_memory_messages);

        // Initialize messages
        let system_prompt = self.build_system_prompt(&config);
        let system_msg = Message::system(&system_prompt);
        let user_msg = Message::user(&config.goal);

        // Add to both state (full history) and memory (LLM context window)
        state.add_message(system_msg.clone());
        state.add_message(user_msg.clone());
        memory.add(system_msg);
        memory.add(user_msg);

        // Core loop (Swarm-inspired simplicity)
        while state.iteration < state.max_iterations && !state.is_terminal() {
            // 1. LLM call - use working memory for context (handles overflow)
            let mut request =
                CompletionRequest::new(memory.get_messages()).with_tools(self.tools.schemas());

            // Only set temperature if explicitly configured (some models don't support it)
            if let Some(temp) = config.temperature {
                request = request.with_temperature(temp);
            }

            let response = self.llm.complete(request).await?;

            // Track token usage
            if let Some(usage) = &response.usage {
                total_tokens += usage.total_tokens;
            }

            // 2. No tool calls → check finish reason and complete
            if response.tool_calls.is_empty() {
                let answer = response.content.unwrap_or_default();
                let assistant_msg = Message::assistant(&answer);
                state.add_message(assistant_msg.clone());
                memory.add(assistant_msg);

                match response.finish_reason {
                    FinishReason::MaxTokens => {
                        state.fail("Response truncated due to max token limit");
                        break;
                    }
                    FinishReason::Error => {
                        state.fail("LLM returned an error");
                        break;
                    }
                    _ => {
                        state.complete(&answer);
                        break;
                    }
                }
            }

            // Add assistant message WITH tool_calls to maintain proper conversation history
            // This is required by OpenAI/Anthropic APIs to correlate tool results with their calls
            let tool_call_msg = Message::assistant_with_tool_calls(
                response.content.clone(),
                response.tool_calls.clone(),
            );
            state.add_message(tool_call_msg.clone());
            memory.add(tool_call_msg);

            // 3. Execute tools in parallel with timeout (Rig-inspired)
            let tool_futures: Vec<_> = response
                .tool_calls
                .iter()
                .map(|tc| {
                    let name = tc.name.clone();
                    let args = tc.arguments.clone();
                    let tools = Arc::clone(&self.tools);
                    let timeout = config.tool_timeout;
                    async move {
                        // Tool timeout
                        let result =
                            tokio::time::timeout(timeout, tools.execute(&name, args)).await;
                        let result = match result {
                            Ok(r) => r,
                            Err(_) => Err(AiError::Tool(format!("Tool {} timed out", name))),
                        };
                        (tc.id.clone(), name, result)
                    }
                })
                .collect();

            let results = futures::future::join_all(tool_futures).await;

            for (tool_call_id, _tool_name, result) in results {
                let mut result_str = match result {
                    Ok(output) if output.success => {
                        serde_json::to_string(&output.result).unwrap_or_default()
                    }
                    Ok(output) => format!("Error: {}", output.error.unwrap_or_default()),
                    Err(e) => format!("Error: {}", e),
                };

                // Truncate long results to prevent context overflow
                if result_str.len() > config.max_tool_result_length {
                    result_str = format!(
                        "{}...[truncated, {} chars total]",
                        &result_str[..config.max_tool_result_length],
                        result_str.len()
                    );
                }

                // Add tool result to both state and working memory
                let tool_result_msg = Message::tool_result(tool_call_id.clone(), result_str);
                state.add_message(tool_result_msg.clone());
                memory.add(tool_result_msg);
            }

            state.increment_iteration();
        }

        // Build result
        Ok(AgentResult {
            success: matches!(state.status, AgentStatus::Completed),
            answer: state.final_answer.clone(),
            error: match &state.status {
                AgentStatus::Failed { error } => Some(error.clone()),
                AgentStatus::MaxIterations => Some("Max iterations reached".to_string()),
                _ => None,
            },
            iterations: state.iteration,
            total_tokens,
            state,
        })
    }

    fn build_system_prompt(&self, config: &AgentConfig) -> String {
        let mut sections = Vec::new();

        let base = config
            .system_prompt
            .as_deref()
            .unwrap_or("You are a helpful AI assistant that can use tools to accomplish tasks.");
        sections.push(base.to_string());

        let tools_desc: Vec<String> = self
            .tools
            .list()
            .iter()
            .filter_map(|name| self.tools.get(name))
            .map(|t| format!("- {}: {}", t.name(), t.description()))
            .collect();

        if !tools_desc.is_empty() {
            sections.push(format!("## Available Tools\n\n{}", tools_desc.join("\n")));
        }

        if let Some(ref context) = config.agent_context {
            let context_str = context.format_for_prompt();
            if !context_str.is_empty() {
                sections.push(context_str);
            }
        }

        sections.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CompletionResponse, FinishReason, Role, StreamChunk, StreamResult, ToolCall, TokenUsage,
    };
    use async_trait::async_trait;
    use futures::stream;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock LLM client for testing
    struct MockLlmClient {
        responses: Mutex<Vec<CompletionResponse>>,
        call_count: AtomicUsize,
        /// Captured requests for verification
        captured_requests: Mutex<Vec<Vec<Message>>>,
    }

    impl MockLlmClient {
        fn new(responses: Vec<CompletionResponse>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
                captured_requests: Mutex::new(Vec::new()),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }

        fn captured_requests(&self) -> Vec<Vec<Message>> {
            self.captured_requests.lock().unwrap().clone()
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

        async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
            self.call_count.fetch_add(1, Ordering::SeqCst);

            // Capture the messages sent to the LLM
            self.captured_requests
                .lock()
                .unwrap()
                .push(request.messages.clone());

            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                Ok(CompletionResponse {
                    content: Some("Done".to_string()),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                    usage: Some(TokenUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        total_tokens: 15,
                    }),
                })
            } else {
                Ok(responses.remove(0))
            }
        }

        fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
            // For mock: convert the sync response to a single-chunk stream
            let response = futures::executor::block_on(self.complete(request));
            match response {
                Ok(resp) => {
                    let chunk = StreamChunk {
                        text: resp.content.unwrap_or_default(),
                        thinking: None,
                        tool_call_delta: None,
                        finish_reason: Some(resp.finish_reason),
                        usage: resp.usage,
                    };
                    Box::pin(stream::once(async move { Ok(chunk) }))
                }
                Err(e) => Box::pin(stream::once(async move { Err(e) })),
            }
        }
    }

    #[tokio::test]
    async fn test_agent_config_max_memory_messages() {
        let config = AgentConfig::new("Test goal").with_max_memory_messages(50);
        assert_eq!(config.max_memory_messages, 50);
    }

    #[tokio::test]
    async fn test_agent_config_default_memory_messages() {
        let config = AgentConfig::new("Test goal");
        assert_eq!(config.max_memory_messages, DEFAULT_MAX_MESSAGES);
    }

    #[tokio::test]
    async fn test_executor_simple_completion() {
        let response = CompletionResponse {
            content: Some("Hello, I'm done!".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: Some(TokenUsage {
                prompt_tokens: 20,
                completion_tokens: 10,
                total_tokens: 30,
            }),
        };

        let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm.clone(), tools);

        let config = AgentConfig::new("Say hello");
        let result = executor.run(config).await.unwrap();

        assert!(result.success);
        assert_eq!(result.answer, Some("Hello, I'm done!".to_string()));
        assert_eq!(mock_llm.call_count(), 1);
    }

    #[tokio::test]
    async fn test_executor_uses_working_memory() {
        // Create a response that completes immediately
        let response = CompletionResponse {
            content: Some("Done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        };

        let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm.clone(), tools);

        let config = AgentConfig::new("Test task")
            .with_system_prompt("You are a test assistant")
            .with_max_memory_messages(10);

        let result = executor.run(config).await.unwrap();
        assert!(result.success);

        // Verify the messages sent to LLM
        let requests = mock_llm.captured_requests();
        assert_eq!(requests.len(), 1);

        let messages = &requests[0];
        assert_eq!(messages.len(), 2); // system + user
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        assert!(messages[1].content.contains("Test task"));
    }

    #[tokio::test]
    async fn test_executor_memory_window_limits_context() {
        // Create responses for a multi-turn conversation
        let responses = vec![
            // First response with tool call
            CompletionResponse {
                content: Some("Let me help".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "unknown_tool".to_string(),
                    arguments: serde_json::json!({}),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            // Second response (completion)
            CompletionResponse {
                content: Some("All done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];

        let mock_llm = Arc::new(MockLlmClient::new(responses));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm.clone(), tools);

        // Set a very small memory limit
        let config = AgentConfig::new("Multi-turn task").with_max_memory_messages(4); // system + user + assistant + tool_result

        let result = executor.run(config).await.unwrap();
        assert!(result.success);
        assert_eq!(mock_llm.call_count(), 2);

        // Second call should have all messages (within limit)
        let requests = mock_llm.captured_requests();
        let second_request = &requests[1];

        // Should have: system, user, assistant (with tool calls), tool result
        assert_eq!(second_request.len(), 4);
    }

    #[tokio::test]
    async fn test_executor_state_tracks_full_history() {
        let responses = vec![
            CompletionResponse {
                content: Some("Step 1".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "test".to_string(),
                    arguments: serde_json::json!({}),
                }],
                finish_reason: FinishReason::ToolCalls,
                usage: None,
            },
            CompletionResponse {
                content: Some("Done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        ];

        let mock_llm = Arc::new(MockLlmClient::new(responses));
        let tools = Arc::new(ToolRegistry::new());
        let executor = AgentExecutor::new(mock_llm, tools);

        let config = AgentConfig::new("Test").with_max_memory_messages(100); // Large enough to hold all

        let result = executor.run(config).await.unwrap();

        // State should have full history
        // system + user + assistant(tool_call) + tool_result + assistant(final)
        assert_eq!(result.state.messages.len(), 5);
    }
}
