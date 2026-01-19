//! Agent executor with ReAct loop

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::agent::state::{AgentState, AgentStatus};
use crate::error::{AiError, Result};
use crate::llm::{CompletionRequest, FinishReason, LlmClient, Message};
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
        }
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

        // Initialize messages
        let system_prompt = self.build_system_prompt(&config);
        state.add_message(Message::system(&system_prompt));
        state.add_message(Message::user(&config.goal));

        // Core loop (Swarm-inspired simplicity)
        while state.iteration < state.max_iterations && !state.is_terminal() {
            // 1. LLM call
            let mut request = CompletionRequest::new(state.messages.clone())
                .with_tools(self.tools.schemas());

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
                state.add_message(Message::assistant(&answer));

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
            state.add_message(Message::assistant_with_tool_calls(
                response.content.clone(),
                response.tool_calls.clone(),
            ));

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

                state.add_tool_result(tool_call_id, result_str);
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
        let tools_desc: Vec<String> = self
            .tools
            .list()
            .iter()
            .filter_map(|name| self.tools.get(name))
            .map(|t| format!("- {}: {}", t.name(), t.description()))
            .collect();

        let base = config.system_prompt.as_deref().unwrap_or(
            "You are a helpful AI assistant that can use tools to accomplish tasks.",
        );

        format!("{}\n\nAvailable tools:\n{}", base, tools_desc.join("\n"))
    }
}
