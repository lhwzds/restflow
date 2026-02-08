//! UnifiedAgent - The single agent implementation for all triggers.

use super::react::{AgentAction, AgentState, ConversationHistory, ReActConfig, ResponseParser};
use crate::LlmClient;
use crate::agent::context::{ContextDiscoveryConfig, WorkspaceContextCache};
use crate::agent::stream::{StreamEmitter, ToolCallAccumulator};
use crate::error::{AiError, Result};
use crate::llm::{CompletionRequest, FinishReason, Message, ToolCall};
use crate::steer::SteerMessage;
use crate::tools::{ToolOutput, ToolRegistry, ToolSchema};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Configuration for UnifiedAgent
#[derive(Debug, Clone)]
pub struct UnifiedAgentConfig {
    pub react: ReActConfig,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_history: usize,
}

impl Default for UnifiedAgentConfig {
    fn default() -> Self {
        Self {
            react: ReActConfig::default(),
            max_tokens: 4096,
            temperature: 0.7,
            max_history: 20,
        }
    }
}

/// Result of agent execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub output: String,
    pub messages: Vec<Message>,
    pub success: bool,
    pub iterations: usize,
}

/// The unified agent that all triggers use
pub struct UnifiedAgent {
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    system_prompt: String,
    config: UnifiedAgentConfig,
    history: ConversationHistory,
    state: AgentState,
    context_cache: Option<WorkspaceContextCache>,
    steer_rx: Option<mpsc::Receiver<SteerMessage>>,
}

impl UnifiedAgent {
    pub fn new(
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<ToolRegistry>,
        system_prompt: String,
        config: UnifiedAgentConfig,
    ) -> Self {
        let context_cache = std::env::current_dir()
            .ok()
            .map(|workdir| WorkspaceContextCache::new(ContextDiscoveryConfig::default(), workdir));

        Self {
            llm_client,
            tool_registry,
            system_prompt,
            config: config.clone(),
            history: ConversationHistory::new(config.max_history),
            state: AgentState::Ready,
            context_cache,
            steer_rx: None,
        }
    }

    /// Add a message to the conversation history.
    pub fn add_history_message(&mut self, message: Message) {
        self.history.add(message);
    }

    /// Seed the agent with an initial history of messages.
    pub fn with_history(mut self, messages: Vec<Message>) -> Self {
        for message in messages {
            self.history.add(message);
        }
        self
    }

    /// Attach a steer channel for live instruction updates.
    pub fn with_steer_channel(mut self, rx: mpsc::Receiver<SteerMessage>) -> Self {
        self.steer_rx = Some(rx);
        self
    }

    fn drain_steer_messages(&mut self) {
        let Some(rx) = &mut self.steer_rx else {
            return;
        };

        loop {
            match rx.try_recv() {
                Ok(steer) => {
                    info!(
                        instruction = %steer.instruction,
                        source = ?steer.source,
                        "Received steer message, injecting into conversation"
                    );
                    self.history.add(Message::user(format!(
                        "[User Update]: {}",
                        steer.instruction
                    )));
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
    }

    /// Execute the agent with given input
    pub async fn execute(&mut self, input: &str) -> Result<ExecutionResult> {
        info!(
            "UnifiedAgent executing: {}...",
            &input[..input.len().min(50)]
        );

        // Prepend system prompt at the beginning to ensure correct order:
        // [system, history..., user] instead of [history..., system, user]
        let system_prompt = self.build_system_prompt().await;
        self.history.prepend(Message::system(system_prompt));
        self.history.add(Message::user(input.to_string()));

        let mut iterations = 0;
        self.state = AgentState::Thinking;

        loop {
            iterations += 1;
            if iterations > self.config.react.max_iterations {
                warn!(
                    "Agent reached max iterations ({})",
                    self.config.react.max_iterations
                );
                return Ok(ExecutionResult {
                    output: "Reached maximum iterations".to_string(),
                    messages: self.history.clone().into_messages(),
                    success: false,
                    iterations,
                });
            }

            debug!("ReAct iteration {}", iterations);
            self.drain_steer_messages();

            let response = self.get_completion().await?;
            let action = ResponseParser::parse(
                response.content.as_deref().unwrap_or_default(),
                Some(&response.tool_calls),
            )?;

            match action {
                AgentAction::ToolCall { .. } => {
                    self.state = AgentState::Acting {
                        tool: response
                            .tool_calls
                            .first()
                            .map(|call| call.name.clone())
                            .unwrap_or_else(|| "unknown".to_string()),
                    };

                    self.history.add(Message::assistant_with_tool_calls(
                        response.content,
                        response.tool_calls.clone(),
                    ));

                    self.execute_tool_calls(&response.tool_calls).await?;
                    self.state = AgentState::Observing;
                }
                AgentAction::FinalAnswer { content } => {
                    self.state = AgentState::Completed {
                        output: content.clone(),
                    };
                    info!("Agent completed in {} iterations", iterations);
                    return Ok(ExecutionResult {
                        output: content,
                        messages: self.history.clone().into_messages(),
                        success: true,
                        iterations,
                    });
                }
                AgentAction::Continue => {
                    self.history
                        .add(Message::assistant(response.content.unwrap_or_default()));
                    self.state = AgentState::Thinking;
                }
            }
        }
    }

    pub async fn execute_streaming(
        &mut self,
        input: &str,
        emitter: &mut dyn StreamEmitter,
    ) -> Result<ExecutionResult> {
        info!(
            "UnifiedAgent streaming execute: {}...",
            &input[..input.len().min(50)]
        );

        let system_prompt = self.build_system_prompt().await;
        self.history.prepend(Message::system(system_prompt));
        self.history.add(Message::user(input.to_string()));

        let mut iterations = 0;
        self.state = AgentState::Thinking;

        loop {
            iterations += 1;
            if iterations > self.config.react.max_iterations {
                warn!(
                    "Agent reached max iterations ({})",
                    self.config.react.max_iterations
                );
                return Ok(ExecutionResult {
                    output: "Reached maximum iterations".to_string(),
                    messages: self.history.clone().into_messages(),
                    success: false,
                    iterations,
                });
            }

            debug!("ReAct iteration {}", iterations);
            self.drain_steer_messages();

            let response = self.get_streaming_completion(emitter).await?;
            let action = ResponseParser::parse(
                response.content.as_deref().unwrap_or_default(),
                Some(&response.tool_calls),
            )?;

            match action {
                AgentAction::ToolCall { .. } => {
                    self.state = AgentState::Acting {
                        tool: response
                            .tool_calls
                            .first()
                            .map(|call| call.name.clone())
                            .unwrap_or_else(|| "unknown".to_string()),
                    };

                    self.history.add(Message::assistant_with_tool_calls(
                        response.content,
                        response.tool_calls.clone(),
                    ));

                    self.execute_tool_calls_with_events(&response.tool_calls, emitter)
                        .await?;
                    self.state = AgentState::Observing;
                }
                AgentAction::FinalAnswer { content } => {
                    self.state = AgentState::Completed {
                        output: content.clone(),
                    };
                    emitter.emit_complete().await;
                    info!("Agent completed in {} iterations", iterations);
                    return Ok(ExecutionResult {
                        output: content,
                        messages: self.history.clone().into_messages(),
                        success: true,
                        iterations,
                    });
                }
                AgentAction::Continue => {
                    self.history
                        .add(Message::assistant(response.content.unwrap_or_default()));
                    self.state = AgentState::Thinking;
                }
            }
        }
    }

    async fn get_streaming_completion(
        &self,
        emitter: &mut dyn StreamEmitter,
    ) -> Result<crate::llm::CompletionResponse> {
        let request = CompletionRequest::new(self.history.messages().to_vec())
            .with_tools(self.tool_schemas())
            .with_max_tokens(self.config.max_tokens)
            .with_temperature(self.config.temperature);

        if !self.llm_client.supports_streaming() {
            let response = self.llm_client.complete(request).await?;
            if let Some(content) = &response.content {
                emitter.emit_text_delta(content).await;
            }
            return Ok(response);
        }

        let mut stream = self.llm_client.complete_stream(request);
        let mut text = String::new();
        let mut accumulator = ToolCallAccumulator::new();
        let mut usage = None;
        let mut finish_reason = None;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;

            if !chunk.text.is_empty() {
                text.push_str(&chunk.text);
                emitter.emit_text_delta(&chunk.text).await;
            }

            if let Some(thinking) = &chunk.thinking {
                emitter.emit_thinking_delta(thinking).await;
            }

            if let Some(delta) = &chunk.tool_call_delta {
                accumulator.accumulate(delta);
            }

            if let Some(chunk_usage) = chunk.usage {
                usage = Some(chunk_usage);
            }

            if let Some(reason) = chunk.finish_reason {
                finish_reason = Some(reason);
            }
        }

        Ok(crate::llm::CompletionResponse {
            content: if text.is_empty() { None } else { Some(text) },
            tool_calls: accumulator.finalize(),
            finish_reason: finish_reason.unwrap_or(FinishReason::Stop),
            usage,
        })
    }

    async fn execute_tool_calls_with_events(
        &mut self,
        tool_calls: &[ToolCall],
        emitter: &mut dyn StreamEmitter,
    ) -> Result<()> {
        for call in tool_calls {
            let arguments = serde_json::to_string(&call.arguments).unwrap_or_default();
            emitter
                .emit_tool_call_start(&call.id, &call.name, &arguments)
                .await;

            let result = self
                .tool_registry
                .execute(&call.name, call.arguments.clone())
                .await?;

            let output = if result.success {
                result.result.to_string()
            } else {
                result
                    .error
                    .clone()
                    .unwrap_or_else(|| "Unknown tool error".to_string())
            };

            emitter
                .emit_tool_call_result(&call.id, &call.name, &output, result.success)
                .await;

            if !result.success {
                return Err(AiError::Tool(output));
            }

            self.history
                .add(Message::tool_result(call.id.clone(), output));
        }
        Ok(())
    }

    async fn build_system_prompt(&self) -> String {
        let mut prompt = String::new();

        // Section 1: Core system instructions (highest priority)
        prompt.push_str("# Core Instructions (PRIMARY)\n\n");
        prompt.push_str(
            "These are your core instructions. They take the HIGHEST priority over any other context.\n\n",
        );
        prompt.push_str(&self.system_prompt);

        // Section 2: Tools & capabilities (actionable)
        let tool_section = self.build_tool_section();
        if !tool_section.is_empty() {
            prompt.push_str("\n\n---\n\n");
            prompt.push_str(&tool_section);
        }

        // Section 3: Workspace reference context (supplementary)
        let workspace_context = self.workspace_context_section().await;
        if !workspace_context.is_empty() {
            prompt.push_str("\n\n---\n\n");
            prompt.push_str(&workspace_context);
        }

        // Section 4: Operating mode
        prompt.push_str("\n\n---\n\n");
        prompt.push_str("# Operating Mode\n\n");
        prompt.push_str("You are in a ReAct loop. For each step:\n");
        prompt.push_str("1. Think about what to do\n");
        prompt.push_str("2. Use a tool if needed\n");
        prompt.push_str("3. Observe the result\n");
        prompt.push_str("4. Provide final answer when done");

        prompt
    }

    fn build_tool_section(&self) -> String {
        let defs = self.tool_registry.schemas();
        if defs.is_empty() {
            return String::new();
        }
        let mut section =
            "# Tools & Capabilities\n\nUse these tools to accomplish your tasks.\n\n".to_string();
        for def in defs {
            section.push_str(&format!("### {}\n{}\n\n", def.name, def.description));
        }
        section
    }

    async fn workspace_context_section(&self) -> String {
        let Some(cache) = &self.context_cache else {
            return String::new();
        };

        let context = cache.get().await;
        if context.content.is_empty() {
            return String::new();
        }

        debug!(
            files = ?context.loaded_files,
            bytes = context.total_bytes,
            "Loaded workspace context"
        );

        let mut section = "# Reference Context (SUPPLEMENTARY)\n\n".to_string();
        section.push_str("The following instructions are discovered from project workspace files (e.g., CLAUDE.md, AGENTS.md).\n");
        section.push_str("Use them as reference. When they conflict with Core Instructions above, Core Instructions take priority.\n\n");
        section.push_str(&context.content);
        section
    }

    async fn execute_tool_calls(&mut self, tool_calls: &[ToolCall]) -> Result<()> {
        for call in tool_calls {
            let result = self
                .tool_registry
                .execute(&call.name, call.arguments.clone())
                .await?;
            let output = format_tool_result(&result)?;
            self.history
                .add(Message::tool_result(call.id.clone(), output));
        }
        Ok(())
    }

    async fn get_completion(&self) -> Result<crate::llm::CompletionResponse> {
        let request = CompletionRequest::new(self.history.messages().to_vec())
            .with_tools(self.tool_schemas())
            .with_max_tokens(self.config.max_tokens)
            .with_temperature(self.config.temperature);
        let response = self.llm_client.complete(request).await?;
        Ok(response)
    }

    fn tool_schemas(&self) -> Vec<ToolSchema> {
        self.tool_registry.schemas()
    }
}

fn format_tool_result(result: &ToolOutput) -> Result<String> {
    if result.success {
        Ok(result.result.to_string())
    } else {
        Err(AiError::Tool(
            result
                .error
                .clone()
                .unwrap_or_else(|| "Unknown tool error".to_string()),
        ))
    }
}
