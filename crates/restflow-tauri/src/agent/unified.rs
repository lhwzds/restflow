//! UnifiedAgent - The single agent implementation for all triggers.

use super::react::{AgentAction, AgentState, ConversationHistory, ReActConfig, ResponseParser};
use super::skills::SkillLoader;
use super::tools::{ToolDefinition, ToolRegistry, ToolResult};
use anyhow::Result;
use restflow_ai::llm::{CompletionRequest, Message, ToolCall};
use restflow_ai::tools::ToolSchema;
use restflow_ai::LlmClient;
use restflow_core::models::AgentNode;
use restflow_core::storage::Storage;
use std::sync::Arc;
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
            config: config.clone(),
            agent_node,
            history: ConversationHistory::new(config.max_history),
            state: AgentState::Ready,
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

    /// Execute the agent with given input
    pub async fn execute(&mut self, input: &str) -> Result<ExecutionResult> {
        info!("UnifiedAgent executing: {}...", &input[..input.len().min(50)]);

        // Prepend system prompt at the beginning to ensure correct order:
        // [system, history..., user] instead of [history..., system, user]
        let system_prompt = self.build_system_prompt()?;
        self.history.prepend(Message::system(system_prompt));
        self.history.add(Message::user(input.to_string()));

        let mut iterations = 0;
        self.state = AgentState::Thinking;

        loop {
            iterations += 1;
            if iterations > self.config.react.max_iterations {
                warn!("Agent reached max iterations ({})", self.config.react.max_iterations);
                return Ok(ExecutionResult {
                    output: "Reached maximum iterations".to_string(),
                    messages: self.history.clone().into_messages(),
                    success: false,
                    iterations,
                });
            }

            debug!("ReAct iteration {}", iterations);

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

                    self.history
                        .add(Message::assistant_with_tool_calls(response.content, response.tool_calls.clone()));

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
                    self.history.add(Message::assistant(
                        response.content.unwrap_or_default(),
                    ));
                    self.state = AgentState::Thinking;
                }
            }
        }
    }

    fn build_system_prompt(&self) -> Result<String> {
        let base = self
            .agent_node
            .prompt
            .clone()
            .unwrap_or_else(|| "You are a helpful AI assistant.".to_string());
        let tool_section = self.build_tool_section();
        let skill_ids = self.agent_node.skills.clone().unwrap_or_default();
        let skill_vars = self.agent_node.skill_variables.clone();
        let prompt = self.skill_loader.build_system_prompt(
            &base,
            &skill_ids,
            skill_vars.as_ref(),
        )?;

        let workspace_context = load_workspace_context();

        Ok(format!(
            "{}\n\n{}{}\n\n## Instructions\nYou are in a ReAct loop. For each step:\n1. Think about what to do\n2. Use a tool if needed\n3. Observe the result\n4. Provide final answer when done",
            prompt, tool_section, workspace_context
        ))
    }

    fn build_tool_section(&self) -> String {
        let defs = self.tool_registry.definitions();
        if defs.is_empty() {
            return String::new();
        }
        let mut section = "## Available Tools\n\n".to_string();
        for def in defs {
            section.push_str(&format!("### {}\n{}\n\n", def.name, def.description));
        }
        section
    }

    async fn execute_tool_calls(&mut self, tool_calls: &[ToolCall]) -> Result<()> {
        for call in tool_calls {
            let result = self
                .tool_registry
                .execute(&call.name, call.arguments.clone())
                .await?;
            let output = format_tool_result(&result);
            self.history.add(Message::tool_result(call.id.clone(), output));
        }
        Ok(())
    }

    async fn get_completion(&self) -> Result<restflow_ai::llm::CompletionResponse> {
        let request = CompletionRequest::new(self.history.messages().to_vec())
            .with_tools(self.tool_schemas())
            .with_max_tokens(self.config.max_tokens)
            .with_temperature(self.config.temperature);
        let response = self.llm_client.complete(request).await?;
        Ok(response)
    }

    fn tool_schemas(&self) -> Vec<ToolSchema> {
        self.tool_registry
            .definitions()
            .into_iter()
            .map(tool_definition_to_schema)
            .collect()
    }
}

fn format_tool_result(result: &ToolResult) -> String {
    if result.success {
        result.output.clone()
    } else {
        result
            .error
            .clone()
            .unwrap_or_else(|| "Unknown tool error".to_string())
    }
}

fn tool_definition_to_schema(def: ToolDefinition) -> ToolSchema {
    ToolSchema {
        name: def.name,
        description: def.description,
        parameters: def.parameters,
    }
}

/// Load workspace context files (CLAUDE.md, AGENTS.md) from current directory.
fn load_workspace_context() -> String {
    let Ok(workdir) = std::env::current_dir() else {
        return String::new();
    };

    let context_files = [
        "CLAUDE.md",
        "AGENTS.md",
        ".claude/CLAUDE.md",
        ".claude/instructions.md",
    ];
    let mut context = String::new();

    for filename in context_files {
        let path = workdir.join(filename);
        if let Ok(content) = std::fs::read_to_string(&path)
            && !content.trim().is_empty()
        {
            context.push_str(&format!(
                "\n\n## Workspace Context ({})\n\n{}",
                filename, content
            ));
        }
    }

    context
}
