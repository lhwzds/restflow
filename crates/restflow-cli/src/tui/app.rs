use crate::tui::commands::SlashCommand;
use anyhow::Result;
use restflow_core::AppCore;
use std::{sync::Arc, time::Instant};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    Chat,
    AgentSelect,
    ModelSelect,
    SessionSelect,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Error,
}

#[derive(Debug, Clone)]
pub struct ToolCallDisplay {
    pub name: String,
    pub input: String,
    pub output: Option<String>,
    pub success: bool,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Vec<ToolCallDisplay>,
}

#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy)]
pub struct TokenCount {
    pub input: u32,
    pub output: u32,
}

impl TokenCount {
    pub fn total(self) -> u32 {
        self.input.saturating_add(self.output)
    }
}

pub struct ChatTui {
    #[allow(dead_code)]
    pub core: Arc<AppCore>,
    pub current_agent_id: String,
    pub current_agent_name: String,
    pub current_model: String,
    pub current_session_id: Option<String>,
    pub agents: Vec<AgentSummary>,
    pub models: Vec<String>,
    pub sessions: Vec<SessionSummary>,
    pub messages: Vec<ChatMessage>,
    #[allow(dead_code)]
    pub scroll_offset: usize,
    pub streaming_text: String,
    pub is_streaming: bool,
    pub input: String,
    pub cursor_position: usize,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub mode: TuiMode,
    pub show_tools: bool,
    pub show_thinking: bool,
    pub activity_status: String,
    pub token_count: TokenCount,
    #[allow(dead_code)]
    pub last_height: u16,
    pub agent_index: usize,
    pub model_index: usize,
    pub session_index: usize,
    pub should_quit: bool,
    pub last_ctrl_c: Instant,
}

impl ChatTui {
    pub async fn new(core: Arc<AppCore>) -> Result<Self> {
        let agents = vec![AgentSummary {
            id: "default".to_string(),
            name: "Default".to_string(),
            model: Some("default".to_string()),
        }];

        let models = vec![
            "default".to_string(),
            "gpt-4.1".to_string(),
            "claude-3.7".to_string(),
        ];

        let sessions = vec![SessionSummary {
            id: "local".to_string(),
            label: "Local session".to_string(),
        }];

        Ok(Self {
            core,
            current_agent_id: agents[0].id.clone(),
            current_agent_name: agents[0].name.clone(),
            current_model: agents[0]
                .model
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            current_session_id: Some("local".to_string()),
            agents,
            models,
            sessions,
            messages: vec![ChatMessage {
                role: MessageRole::System,
                content: "Welcome to RestFlow CLI chat.".to_string(),
                tool_calls: Vec::new(),
            }],
            scroll_offset: 0,
            streaming_text: String::new(),
            is_streaming: false,
            input: String::new(),
            cursor_position: 0,
            input_history: Vec::new(),
            history_index: None,
            mode: TuiMode::Chat,
            show_tools: false,
            show_thinking: false,
            activity_status: "idle".to_string(),
            token_count: TokenCount {
                input: 0,
                output: 0,
            },
            last_height: 0,
            agent_index: 0,
            model_index: 0,
            session_index: 0,
            should_quit: false,
            last_ctrl_c: Instant::now(),
        })
    }

    pub async fn tick(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn enter_char(&mut self, c: char) {
        let byte_idx = self.char_to_byte_idx(self.cursor_position);
        self.input.insert(byte_idx, c);
        self.cursor_position += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            let byte_idx = self.char_to_byte_idx(self.cursor_position);
            self.input.remove(byte_idx);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.chars().count() {
            self.cursor_position += 1;
        }
    }

    pub fn history_up(&mut self) {
        if self.input_history.is_empty() {
            return;
        }

        let new_index = match self.history_index {
            Some(idx) => idx.saturating_sub(1),
            None => self.input_history.len().saturating_sub(1),
        };

        self.history_index = Some(new_index);
        self.input = self.input_history[new_index].clone();
        self.cursor_position = self.input.chars().count();
    }

    pub fn history_down(&mut self) {
        let Some(idx) = self.history_index else {
            return;
        };

        if idx + 1 >= self.input_history.len() {
            self.history_index = None;
            self.input.clear();
            self.cursor_position = 0;
            return;
        }

        let new_index = idx + 1;
        self.history_index = Some(new_index);
        self.input = self.input_history[new_index].clone();
        self.cursor_position = self.input.chars().count();
    }

    pub async fn submit(&mut self) -> Result<()> {
        if self.input.trim().is_empty() {
            return Ok(());
        }

        let input = self.input.clone();
        self.push_user_message(&input);
        self.record_history(&input);
        self.input.clear();
        self.cursor_position = 0;
        self.history_index = None;

        if let Some(cmd) = SlashCommand::parse(&input) {
            self.handle_slash_command(cmd).await?;
            return Ok(());
        }

        self.push_assistant_message("AI chat is not yet implemented in CLI.");
        self.activity_status = "idle".to_string();
        Ok(())
    }

    pub fn push_user_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: MessageRole::User,
            content: content.to_string(),
            tool_calls: Vec::new(),
        });
    }

    pub fn push_assistant_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: content.to_string(),
            tool_calls: Vec::new(),
        });
    }

    pub fn push_system_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: MessageRole::System,
            content: content.to_string(),
            tool_calls: Vec::new(),
        });
    }

    pub fn push_error_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: MessageRole::Error,
            content: content.to_string(),
            tool_calls: Vec::new(),
        });
    }

    pub fn switch_agent(&mut self, id: &str) -> bool {
        if let Some((idx, agent)) = self
            .agents
            .iter()
            .enumerate()
            .find(|(_, agent)| agent.id == id || agent.name.eq_ignore_ascii_case(id))
        {
            self.current_agent_id = agent.id.clone();
            self.current_agent_name = agent.name.clone();
            if let Some(model) = &agent.model {
                self.current_model = model.clone();
            }
            self.agent_index = idx;
            self.activity_status = format!("agent: {}", self.current_agent_name);
            return true;
        }
        false
    }

    pub fn switch_model(&mut self, name: &str) -> bool {
        if let Some((idx, model)) = self
            .models
            .iter()
            .enumerate()
            .find(|(_, model)| model.eq_ignore_ascii_case(name))
        {
            self.current_model = model.clone();
            self.model_index = idx;
            self.activity_status = format!("model: {}", self.current_model);
            return true;
        }
        false
    }

    pub fn load_session(&mut self, id: &str) -> bool {
        if let Some((idx, session)) = self
            .sessions
            .iter()
            .enumerate()
            .find(|(_, session)| session.id == id || session.label.eq_ignore_ascii_case(id))
        {
            self.current_session_id = Some(session.id.clone());
            self.session_index = idx;
            self.activity_status = format!("session: {}", session.label);
            return true;
        }
        false
    }

    pub fn new_session(&mut self) {
        self.messages.clear();
        self.push_system_message("New session started.");
        self.activity_status = "new session".to_string();
    }

    pub fn cancel_streaming(&mut self) {
        self.is_streaming = false;
        self.streaming_text.clear();
        self.activity_status = "stream canceled".to_string();
    }

    pub async fn handle_slash_command(&mut self, cmd: SlashCommand) -> Result<()> {
        match cmd {
            SlashCommand::Help => {
                self.mode = TuiMode::Help;
            }
            SlashCommand::Clear => {
                self.messages.clear();
                self.push_system_message("Chat cleared.");
            }
            SlashCommand::Exit => {
                self.should_quit = true;
            }
            SlashCommand::Agent(id) => {
                if !self.switch_agent(&id) {
                    self.push_error_message(&format!("Unknown agent: {}", id));
                }
            }
            SlashCommand::Model(name) => {
                if !self.switch_model(&name) {
                    self.push_error_message(&format!("Unknown model: {}", name));
                }
            }
            SlashCommand::Session(id) => {
                if !self.load_session(&id) {
                    self.push_error_message(&format!("Unknown session: {}", id));
                }
            }
            SlashCommand::New => {
                self.new_session();
            }
            SlashCommand::History => {
                self.push_system_message("Session history is not implemented yet.");
            }
            SlashCommand::Memory(query) => {
                self.push_system_message(&format!("Memory search not implemented: {}", query));
            }
            SlashCommand::Export => {
                self.push_system_message("Export not implemented yet.");
            }
            SlashCommand::Think(enabled) => {
                self.show_thinking = enabled;
                self.activity_status = if enabled {
                    "thinking: on".to_string()
                } else {
                    "thinking: off".to_string()
                };
            }
            SlashCommand::Verbose(enabled) => {
                self.activity_status = if enabled {
                    "verbose: on".to_string()
                } else {
                    "verbose: off".to_string()
                };
            }
        }

        Ok(())
    }

    fn record_history(&mut self, input: &str) {
        if self.input_history.last().is_some_and(|last| last == input) {
            return;
        }
        self.input_history.push(input.to_string());
    }

    fn char_to_byte_idx(&self, char_idx: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_idx)
            .map(|(idx, _)| idx)
            .unwrap_or_else(|| self.input.len())
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

    pub fn cursor_visual_position(&self, width: u16) -> (u16, u16) {
        if width == 0 {
            return (0, 0);
        }

        let mut line = 0u16;
        let mut col = 0u16;
        let cursor_byte = self.char_to_byte_idx(self.cursor_position);
        for ch in self.input[..cursor_byte].chars() {
            Self::apply_visual_char(&mut line, &mut col, width, ch);
        }

        (line, col)
    }

    pub fn input_line_count(&self, width: u16) -> u16 {
        if width == 0 {
            return 1;
        }

        let mut line = 0u16;
        let mut col = 0u16;
        for ch in self.input.chars() {
            Self::apply_visual_char(&mut line, &mut col, width, ch);
        }

        line.saturating_add(1)
    }

    pub fn scroll_offset(&self, visible_lines: u16, width: u16) -> u16 {
        if visible_lines == 0 || width == 0 {
            return 0;
        }

        let total_lines = self.input_line_count(width);
        let cursor_line = self.cursor_visual_position(width).0;
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let desired = cursor_line.saturating_sub(visible_lines.saturating_sub(1));
        desired.min(max_scroll)
    }
}
