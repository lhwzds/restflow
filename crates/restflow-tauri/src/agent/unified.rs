//! UnifiedAgent - single agent implementation for all triggers.

use anyhow::{Result, anyhow};
use restflow_ai::LlmClient;
use restflow_ai::llm::{CompletionRequest, FinishReason, Message, ToolCall};
use restflow_core::models::AgentNode;
use restflow_core::storage::Storage;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::react::{AgentAction, AgentState, ConversationHistory, ReActConfig, ResponseParser};
use super::skills::SkillLoader;
use super::tools::ToolRegistry;

/// Configuration for UnifiedAgent.
#[derive(Debug, Clone)]
pub struct UnifiedAgentConfig {
    pub react: ReActConfig,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_history: usize,
    pub max_tool_result_length: usize,
}

impl Default for UnifiedAgentConfig {
    fn default() -> Self {
        Self {
            react: ReActConfig::default(),
            max_tokens: 4096,
            temperature: 0.7,
            max_history: 40,
            max_tool_result_length: 4000,
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
}

/// The unified agent that all triggers use.
pub struct UnifiedAgent {
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    skill_loader: SkillLoader,
    config: UnifiedAgentConfig,
    agent_node: AgentNode,
    history: ConversationHistory,
    state: AgentState,
}

impl UnifiedAgent {
    pub fn new(
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<ToolRegistry>,
        storage: Arc<Storage>,
        agent_node: AgentNode,
        config: UnifiedAgentConfig,
    ) -> Self {
        Self {
            llm_client,
            tool_registry,
            skill_loader: SkillLoader::new(storage),
            history: ConversationHistory::new(config.max_history),
            config,
            agent_node,
            state: AgentState::Ready,
        }
    }

    /// Execute the agent with given input.
    pub async fn execute(&mut self, input: &str) -> Result<ExecutionResult> {
        info!("UnifiedAgent executing");

        let system_prompt = self.build_system_prompt()?;
        self.history.add(Message::system(system_prompt));
        self.history.add(Message::user(input.to_string()));

        self.state = AgentState::Thinking;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > self.config.react.max_iterations {
                warn!("UnifiedAgent reached max iterations");
                return Ok(ExecutionResult {
                    output: "Reached maximum iterations".to_string(),
                    messages: self.history.clone().into_messages(),
                    success: false,
                    iterations,
                });
            }

            debug!(iteration = iterations, "ReAct iteration");

            let request = CompletionRequest::new(self.history.messages().to_vec())
                .with_tools(self.tool_registry.schemas())
                .with_max_tokens(self.config.max_tokens)
                .with_temperature(self.config.temperature);

            let response = self.llm_client.complete(request).await?;
            let content = response.content.clone().unwrap_or_default();
            let action = ResponseParser::parse(&content, Some(&response.tool_calls))?;

            match action {
                AgentAction::ToolCalls { calls } => {
                    self.state = AgentState::Acting {
                        tool: calls
                            .first()
                            .map(|call| call.name.clone())
                            .unwrap_or_else(|| "unknown".to_string()),
                    };

                    self.history.add(Message::assistant_with_tool_calls(
                        response.content.clone(),
                        response.tool_calls.clone(),
                    ));

                    self.execute_tool_calls(calls).await?;
                    self.state = AgentState::Observing;
                }
                AgentAction::FinalAnswer { content } => {
                    self.state = AgentState::Completed {
                        output: content.clone(),
                    };
                    self.history.add(Message::assistant(content.clone()));
                    return Ok(ExecutionResult {
                        output: content,
                        messages: self.history.clone().into_messages(),
                        success: response.finish_reason != FinishReason::Error,
                        iterations,
                    });
                }
                AgentAction::Continue => {
                    if !content.is_empty() {
                        self.history.add(Message::assistant(content));
                    }
                    self.state = AgentState::Thinking;
                }
            }
        }
    }

    async fn execute_tool_calls(&mut self, calls: Vec<ToolCall>) -> Result<()> {
        for call in calls {
            let result = self
                .tool_registry
                .execute(&call.name, call.arguments.clone())
                .await
                .map_err(|err| anyhow!("Tool '{}' failed: {}", call.name, err))?;

            let mut payload = json!({
                "success": result.success,
                "output": result.output,
                "error": result.error,
            });

            if let Some(output) = payload.get_mut("output") {
                if let Some(text) = output.as_str() {
                    if text.len() > self.config.max_tool_result_length {
                        let truncated =
                            format!("{}...", &text[..self.config.max_tool_result_length]);
                        *output = json!(truncated);
                    }
                }
            }

            let content = serde_json::to_string(&payload)?;
            self.history.add(Message::tool_result(call.id, content));
        }

        Ok(())
    }

    fn build_system_prompt(&self) -> Result<String> {
        let base_prompt = self
            .agent_node
            .prompt
            .clone()
            .unwrap_or_else(|| "You are a helpful AI assistant.".to_string());

        let tool_section = self.build_tool_section();
        let skill_ids = self.agent_node.skills.clone().unwrap_or_default();
        let prompt = self.skill_loader.build_system_prompt(
            &base_prompt,
            &skill_ids,
            self.agent_node.skill_variables.as_ref(),
        )?;

        Ok(format!(
            "{}\n\n{}\n\n## Instructions\nYou are running in a tool-assisted loop. Use tools when helpful, then provide a final answer.",
            prompt, tool_section
        ))
    }

    fn build_tool_section(&self) -> String {
        let defs = self.tool_registry.definitions();
        if defs.is_empty() {
            return String::new();
        }

        let mut section = String::from("## Available Tools\n\n");
        for def in defs {
            section.push_str(&format!("### {}\n{}\n\n", def.name, def.description));
        }
        section
    }
}
