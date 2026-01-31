//! TUI application state

use super::MIN_INPUT_HEIGHT;
use crate::tui::stream::{
    StreamCancelHandle, StreamEvent, StreamingExecutor, build_system_prompt,
    create_working_memory, format_tool_output,
};
use restflow_ai::llm::Message as LlmMessage;
use restflow_ai::{AgentConfig, AgentState, AnthropicClient, OpenAIClient};
use restflow_core::models::{
    ApiKeyConfig, ChatMessage, ChatRole, ChatSession, ExecutionStepInfo, MessageExecution,
};
use restflow_core::{AppCore, models::AIModel, models::Provider};
use std::sync::Arc;
use tokio::sync::mpsc;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Default, Clone)]
pub(super) struct TokenCounter {
    pub(super) input: u32,
    pub(super) output: u32,
}

pub struct TuiApp {
    pub input: String,
    pub cursor_position: usize,
    /// Messages pending flush to history
    pub new_messages: Vec<String>,
    pub show_commands: bool,
    pub commands: Vec<Command>,
    pub selected_command: usize,
    pub core: Arc<AppCore>,
    pub should_clear: bool,
    pub command_history: Vec<String>,
    pub last_total_height: u16,
    pub last_terminal_height: u16,
    pub current_agent_id: Option<String>,
    pub current_agent_name: String,
    pub current_model: String,
    pub current_session: Option<ChatSession>,
    pub activity_status: String,
    pub is_streaming: bool,
    pub streaming_text: String,
    pub streaming_started: bool,
    pub token_counter: TokenCounter,
    pub stream_rx: Option<mpsc::UnboundedReceiver<StreamEvent>>,
    pub cancel_handle: Option<StreamCancelHandle>,
    pub current_execution: Option<MessageExecution>,
}

impl TuiApp {
    pub fn new(core: Arc<AppCore>) -> Self {
        let commands = vec![
            Command {
                name: "/help".to_string(),
                description: "Show help information".to_string(),
            },
            Command {
                name: "/new".to_string(),
                description: "Start a new chat session".to_string(),
            },
            Command {
                name: "/sessions".to_string(),
                description: "List recent chat sessions".to_string(),
            },
            Command {
                name: "/load".to_string(),
                description: "Load a session by id".to_string(),
            },
            Command {
                name: "/export".to_string(),
                description: "Export current session to markdown".to_string(),
            },
            Command {
                name: "/clear".to_string(),
                description: "Clear the screen".to_string(),
            },
            Command {
                name: "/exit".to_string(),
                description: "Exit the program".to_string(),
            },
        ];

        let mut app = Self {
            input: String::new(),
            cursor_position: 0,
            new_messages: Vec::new(),
            show_commands: false,
            commands,
            selected_command: 0,
            core,
            should_clear: false,
            command_history: Vec::new(),
            last_total_height: MIN_INPUT_HEIGHT,
            last_terminal_height: MIN_INPUT_HEIGHT,
            current_agent_id: None,
            current_agent_name: "".to_string(),
            current_model: "".to_string(),
            current_session: None,
            activity_status: "Idle".to_string(),
            is_streaming: false,
            streaming_text: String::new(),
            streaming_started: false,
            token_counter: TokenCounter::default(),
            stream_rx: None,
            cancel_handle: None,
            current_execution: None,
        };

        app.bootstrap_session();
        app
    }

    fn bootstrap_session(&mut self) {
        if let Err(err) = self.select_default_agent() {
            self.new_messages
                .push(format!("❌ Failed to load agents: {}", err));
            return;
        }

        if let Some(session) = self.load_latest_session() {
            self.new_messages.push(format!(
                "✅ Loaded session '{}' ({} messages)",
                session.name,
                session.messages.len()
            ));
            self.current_session = Some(session);
        } else {
            self.start_new_session();
        }
    }

    fn select_default_agent(&mut self) -> anyhow::Result<()> {
        let agents = self.core.storage.agents.list_agents()?;
        let Some(agent) = agents.first() else {
            self.current_agent_id = None;
            self.current_agent_name = "No agents".to_string();
            self.current_model = "".to_string();
            return Ok(());
        };

        self.current_agent_id = Some(agent.id.clone());
        self.current_agent_name = agent.name.clone();
        self.current_model = agent.agent.model.as_str().to_string();
        Ok(())
    }

    fn load_latest_session(&self) -> Option<ChatSession> {
        let sessions = self.core.storage.chat_sessions.list().ok()?;
        sessions.into_iter().next()
    }

    fn start_new_session(&mut self) {
        let Some(agent_id) = self.current_agent_id.clone() else {
            self.new_messages
                .push("❌ No agents available. Create one in the web UI.".to_string());
            return;
        };

        let model = if self.current_model.is_empty() {
            "unknown".to_string()
        } else {
            self.current_model.clone()
        };

        let session = ChatSession::new(agent_id, model);
        self.current_session = Some(session);
        self.streaming_text.clear();
        self.streaming_started = false;
        self.activity_status = "New session".to_string();
        self.new_messages
            .push("✨ Started a new chat session.".to_string());
    }

    fn ensure_session_mut(&mut self) -> Option<&mut ChatSession> {
        if self.current_session.is_none() {
            self.start_new_session();
        }
        self.current_session.as_mut()
    }

    fn save_session(&self) {
        if let Some(session) = &self.current_session {
            let _ = self.core.storage.chat_sessions.save(session);
        }
    }

    fn list_sessions(&mut self) {
        match self.core.storage.chat_sessions.list_summaries() {
            Ok(summaries) => {
                if summaries.is_empty() {
                    self.new_messages.push("No sessions found.".to_string());
                    return;
                }
                self.new_messages.push("📂 Recent sessions:".to_string());
                for summary in summaries.iter().take(10) {
                    self.new_messages.push(format!(
                        "  {} - {} ({} msgs)",
                        summary.id, summary.name, summary.message_count
                    ));
                }
            }
            Err(err) => self
                .new_messages
                .push(format!("❌ Failed to list sessions: {}", err)),
        }
    }

    fn load_session(&mut self, session_id: &str) {
        match self.core.storage.chat_sessions.get(session_id) {
            Ok(Some(session)) => {
                self.current_session = Some(session);
                self.activity_status = format!("Loaded session {session_id}");
                self.new_messages
                    .push(format!("✅ Loaded session {session_id}"));
            }
            Ok(None) => self
                .new_messages
                .push(format!("❌ Session not found: {session_id}")),
            Err(err) => self
                .new_messages
                .push(format!("❌ Failed to load session: {}", err)),
        }
    }

    fn export_session(&mut self) {
        let Some(session) = &self.current_session else {
            self.new_messages
                .push("❌ No session to export.".to_string());
            return;
        };

        let filename = format!(
            "chat-{}-{}.md",
            self.current_agent_name.replace(' ', "_"),
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );

        let mut content = String::new();
        content.push_str(&format!("# Chat Session\n\n"));
        content.push_str(&format!("**Agent**: {}\n", self.current_agent_name));
        content.push_str(&format!("**Model**: {}\n", session.model));
        content.push_str(&format!("**Date**: {}\n\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
        content.push_str("---\n\n");

        for message in &session.messages {
            let role = match message.role {
                ChatRole::User => "**User**",
                ChatRole::Assistant => "**Assistant**",
                ChatRole::System => "**System**",
            };
            content.push_str(&format!("{}\n{}\n\n", role, message.content));
            if let Some(execution) = &message.execution {
                if !execution.steps.is_empty() {
                    content.push_str("**Execution Steps:**\n");
                    for step in &execution.steps {
                        content.push_str(&format!(
                            "- [{}] {} ({})\n",
                            step.step_type, step.name, step.status
                        ));
                    }
                    content.push_str("\n");
                }
            }
        }

        if let Err(err) = std::fs::write(&filename, content) {
            self.new_messages
                .push(format!("❌ Failed to export session: {}", err));
            return;
        }

        self.new_messages
            .push(format!("✅ Session exported to {}", filename));
    }

    fn to_llm_history(&self) -> Vec<LlmMessage> {
        let Some(session) = &self.current_session else {
            return Vec::new();
        };

        session
            .messages
            .iter()
            .filter_map(|msg| match msg.role {
                ChatRole::User => Some(LlmMessage::user(&msg.content)),
                ChatRole::Assistant => Some(LlmMessage::assistant(&msg.content)),
                ChatRole::System => Some(LlmMessage::system(&msg.content)),
            })
            .collect()
    }

    fn build_llm(&self, model: AIModel, api_key: String) -> Arc<dyn restflow_ai::llm::LlmClient> {
        match model.provider() {
            Provider::Anthropic => {
                Arc::new(AnthropicClient::new(api_key).with_model(model.as_str()))
            }
            Provider::OpenAI => Arc::new(OpenAIClient::new(api_key).with_model(model.as_str())),
            Provider::DeepSeek => Arc::new(
                OpenAIClient::new(api_key)
                    .with_model(model.as_str())
                    .with_base_url("https://api.deepseek.com/v1"),
            ),
        }
    }

    fn resolve_api_key(&self, config: &ApiKeyConfig) -> anyhow::Result<String> {
        match config {
            ApiKeyConfig::Direct(value) => Ok(value.clone()),
            ApiKeyConfig::Secret(name) => self
                .core
                .storage
                .secrets
                .get_secret(name)?
                .ok_or_else(|| anyhow::anyhow!("Secret not found: {}", name)),
        }
    }

    async fn start_streaming(&mut self, input: String) {
        if self.is_streaming {
            self.new_messages
                .push("⚠️ Already streaming. Press Esc to cancel.".to_string());
            return;
        }

        if self.current_session.is_none() {
            self.start_new_session();
        }

        let agent_id = match self.current_agent_id.clone() {
            Some(id) => id,
            None => {
                self.new_messages
                    .push("❌ No agent configured. Create one in the web UI.".to_string());
                return;
            }
        };

        let agent = match self.core.storage.agents.get_agent(agent_id.clone()) {
            Ok(Some(agent)) => agent,
            Ok(None) => {
                self.new_messages
                    .push("❌ Agent not found. Create one in the web UI.".to_string());
                return;
            }
            Err(err) => {
                self.new_messages
                    .push(format!("❌ Failed to load agent: {}", err));
                return;
            }
        };

        let api_key_config = match &agent.agent.api_key_config {
            Some(config) => config,
            None => {
                self.new_messages
                    .push("❌ Agent missing API key config.".to_string());
                return;
            }
        };

        let api_key = match self.resolve_api_key(api_key_config) {
            Ok(key) => key,
            Err(err) => {
                self.new_messages
                    .push(format!("❌ Failed to resolve API key: {}", err));
                return;
            }
        };

        let llm = self.build_llm(agent.agent.model, api_key);
        let tool_registry = restflow_core::services::tool_registry::create_tool_registry(
            self.core.storage.skills.clone(),
        );
        let tools = if let Some(selected_tools) = agent.agent.tools.as_ref() {
            let mut filtered = restflow_ai::ToolRegistry::new();
            for name in selected_tools {
                if let Some(tool) = tool_registry.get(name) {
                    filtered.register_arc(tool);
                } else {
                    self.new_messages
                        .push(format!("⚠️ Tool not found: {}", name));
                }
            }
            Arc::new(filtered)
        } else {
            Arc::new(tool_registry)
        };

        let mut config = AgentConfig::new(&input);
        if let Some(prompt) = agent.agent.prompt.as_ref() {
            if !prompt.trim().is_empty() {
                config = config.with_system_prompt(prompt.clone());
            }
        }

        let system_prompt = build_system_prompt(&config, &tools);

        let mut config = config.with_system_prompt(system_prompt.clone());
        if let Some(temp) = agent.agent.temperature {
            config = config.with_temperature(temp as f32);
        }

        let history = self.to_llm_history();
        let memory = create_working_memory(&system_prompt, &history, config.max_memory_messages);
        let state = AgentState::new(uuid::Uuid::new_v4().to_string(), config.max_iterations);

        {
            let Some(session) = self.ensure_session_mut() else {
                return;
            };
            session.add_message(ChatMessage::user(&input));
            session.auto_name_from_first_message();
        }
        self.save_session();

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cancel_handle, cancel_receiver) = StreamCancelHandle::new();
        let executor = StreamingExecutor::new(llm, tools, event_tx.clone());

        self.stream_rx = Some(event_rx);
        self.cancel_handle = Some(cancel_handle);
        self.is_streaming = true;
        self.streaming_text.clear();
        self.streaming_started = false;
        self.activity_status = "Streaming".to_string();
        self.token_counter = TokenCounter::default();
        self.current_execution = Some(MessageExecution::new());

        tokio::spawn(async move {
            if let Err(err) = executor.execute(state, memory, config, cancel_receiver).await {
                let _ = event_tx.send(StreamEvent::Error(err.to_string()));
            }
        });
    }

    pub async fn poll_events(&mut self) {
        let Some(rx) = &mut self.stream_rx else {
            return;
        };

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        for event in events {
            self.handle_stream_event(event);
        }
    }

    fn handle_stream_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::TextDelta(delta) => {
                if !self.streaming_started {
                    self.new_messages.push(format!("🤖 {}", delta));
                    self.streaming_started = true;
                } else {
                    self.new_messages.push(delta.clone());
                }
                self.streaming_text.push_str(&delta);
            }
            StreamEvent::Thinking(thinking) => {
                self.new_messages
                    .push(format!("  💭 {}", thinking.trim()));
                if let Some(exec) = &mut self.current_execution {
                    exec.add_step(ExecutionStepInfo::new("thinking", "Model reasoning"));
                }
            }
            StreamEvent::ToolStart { name, input } => {
                self.new_messages.push(format!("  🔧 Tool start: {}", name));
                for line in input.lines() {
                    self.new_messages.push(format!("    {}", line));
                }
                if let Some(exec) = &mut self.current_execution {
                    exec.add_step(ExecutionStepInfo::new("tool_call", name));
                }
            }
            StreamEvent::ToolEnd {
                name,
                output,
                success,
            } => {
                let status = if success { "✅" } else { "❌" };
                self.new_messages
                    .push(format!("  {} Tool end: {}", status, name));
                let formatted = format_tool_output(&output);
                for line in formatted.lines() {
                    self.new_messages.push(format!("    {}", line));
                }
            }
            StreamEvent::TokenUpdate {
                input_tokens,
                output_tokens,
            } => {
                self.token_counter.input = input_tokens;
                self.token_counter.output = output_tokens;
            }
            StreamEvent::Complete {
                response,
                total_tokens,
                duration_ms,
            } => {
                let final_text = if self.streaming_text.is_empty() {
                    response
                } else {
                    std::mem::take(&mut self.streaming_text)
                };

                let exec = self
                    .current_execution
                    .take()
                    .unwrap_or_else(MessageExecution::new)
                    .complete(duration_ms, total_tokens);

                if let Some(session) = self.ensure_session_mut() {
                    let message = ChatMessage::assistant(final_text).with_execution(exec);
                    session.add_message(message);
                    self.save_session();
                }

                self.new_messages.push(format!(
                    "  ✅ Completed ({} ms, {} tokens)",
                    duration_ms, total_tokens
                ));
                self.is_streaming = false;
                self.activity_status = "Idle".to_string();
                self.stream_rx = None;
                self.cancel_handle = None;
                self.streaming_started = false;
            }
            StreamEvent::Error(err) => {
                self.new_messages.push(format!("❌ Error: {}", err));
                if let Some(exec) = self.current_execution.take() {
                    let failed = exec.fail(0);
                    if let Some(session) = self.ensure_session_mut() {
                        let message = ChatMessage::assistant("Execution failed").with_execution(failed);
                        session.add_message(message);
                        self.save_session();
                    }
                }
                self.is_streaming = false;
                self.activity_status = "Error".to_string();
                self.stream_rx = None;
                self.cancel_handle = None;
                self.streaming_started = false;
            }
            StreamEvent::Cancelled => {
                self.new_messages.push("⏹ Streaming cancelled".to_string());
                self.is_streaming = false;
                self.activity_status = "Cancelled".to_string();
                self.stream_rx = None;
                self.cancel_handle = None;
                self.streaming_started = false;
            }
        }
    }

    pub fn cancel_streaming(&mut self) {
        if let Some(handle) = &self.cancel_handle {
            handle.cancel();
        }
    }

    fn char_to_byte_idx(&self, char_idx: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_idx)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(self.input.len())
    }

    fn cursor_byte_index(&self) -> usize {
        self.char_to_byte_idx(self.cursor_position)
    }

    fn current_line_prefix(&self) -> &str {
        let cursor_byte = self.cursor_byte_index();
        let start = self.input[..cursor_byte]
            .rfind('\n')
            .map(|idx| idx + 1)
            .unwrap_or(0);
        &self.input[start..cursor_byte]
    }

    fn refresh_command_menu(&mut self) {
        let current_line = self.current_line_prefix();
        if current_line == "/" {
            self.show_commands = true;
            self.selected_command = 0;
        } else if !current_line.starts_with('/') {
            self.show_commands = false;
        }
    }

    fn apply_visual_char(line: &mut u16, col: &mut u16, width: u16, ch: char) {
        if width == 0 {
            return;
        }

        if ch == '\n' {
            *line = line.saturating_add(1);
            *col = 0;
            return;
        }

        let w = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
        if w == 0 {
            return;
        }

        if *col + w > width {
            *line = line.saturating_add(1);
            *col = w.min(width);
        } else if *col + w == width {
            *line = line.saturating_add(1);
            *col = 0;
        } else {
            *col += w;
        }

        if *col == width {
            *line = line.saturating_add(1);
            *col = 0;
        }
    }

    pub fn cursor_visual_position_for_width(&self, width: u16) -> (u16, u16) {
        if width == 0 {
            return (0, 0);
        }

        let mut line: u16 = 0;
        let mut col: u16 = 0;
        let cursor_byte = self.cursor_byte_index();
        for ch in self.input[..cursor_byte].chars() {
            Self::apply_visual_char(&mut line, &mut col, width, ch);
        }

        (line, col)
    }

    pub fn visual_line_count(&self, width: u16) -> u16 {
        if width == 0 {
            return 1;
        }

        let mut line: u16 = 0;
        let mut col: u16 = 0;
        for ch in self.input.chars() {
            Self::apply_visual_char(&mut line, &mut col, width, ch);
        }

        line + 1
    }

    pub fn scroll_offset_for_width(&self, visible_lines: u16, width: u16) -> u16 {
        if visible_lines == 0 || width == 0 {
            return 0;
        }

        let total_lines = self.visual_line_count(width);
        let cursor_line = self.cursor_visual_position_for_width(width).0;
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let desired = cursor_line.saturating_sub(visible_lines.saturating_sub(1));
        desired.min(max_scroll)
    }

    pub fn enter_char(&mut self, c: char) {
        let byte_idx = self.char_to_byte_idx(self.cursor_position);
        self.input.insert(byte_idx, c);
        self.cursor_position += 1;
        self.refresh_command_menu();
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            let byte_idx = self.char_to_byte_idx(self.cursor_position);
            self.input.remove(byte_idx);
            self.refresh_command_menu();
        }
    }

    pub fn next_command(&mut self) {
        if self.show_commands && self.selected_command < self.commands.len() - 1 {
            self.selected_command += 1;
        }
    }

    pub fn previous_command(&mut self) {
        if self.show_commands && self.selected_command > 0 {
            self.selected_command -= 1;
        }
    }

    pub fn select_command(&mut self) {
        if self.show_commands && !self.commands.is_empty() {
            self.input = self.commands[self.selected_command].name.clone();
            self.cursor_position = self.input.chars().count();
            self.show_commands = false;
        }
    }

    pub async fn submit(&mut self) {
        if self.input.is_empty() {
            return;
        }

        if self.show_commands {
            self.select_command();
            return;
        }

        let input = self.input.clone();
        self.new_messages.push(format!("> {}", input));

        if self.command_history.is_empty() || self.command_history.last() != Some(&input) {
            self.command_history.push(input.clone());
        }

        match input.as_str() {
            "/clear" => {
                self.should_clear = true;
                self.new_messages.push("🔄 Clearing screen...".to_string());
            }
            "/help" => {
                self.new_messages.push("📖 Available commands:".to_string());
                for cmd in &self.commands {
                    self.new_messages
                        .push(format!("  {} - {}", cmd.name, cmd.description));
                }
            }
            "/new" => {
                self.start_new_session();
            }
            "/sessions" => {
                self.list_sessions();
            }
            "/export" => {
                self.export_session();
            }
            cmd if cmd.starts_with("/load ") => {
                let id = cmd.trim_start_matches("/load ").trim();
                if id.is_empty() {
                    self.new_messages
                        .push("❌ Usage: /load <session_id>".to_string());
                } else {
                    self.load_session(id);
                }
            }
            cmd if cmd.starts_with('/') => {
                self.new_messages
                    .push(format!("❌ Unknown command: {}", cmd));
            }
            _ => {
                self.start_streaming(input).await;
            }
        }

        self.input.clear();
        self.cursor_position = 0;
        self.show_commands = false;
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        let char_count = self.input.chars().count();
        if self.cursor_position < char_count {
            self.cursor_position += 1;
        }
    }
}
