//! TUI application state

use super::{MIN_INPUT_HEIGHT, highlight::SyntaxHighlighter, history::InputHistory, shell};
use restflow_core::AppCore;
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

use crate::config::CliConfig;
use crate::tui::highlight::theme_for_config;

#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
}

pub struct TuiApp {
    pub input: String,
    pub cursor_position: usize,
    /// Messages pending flush to history
    pub new_messages: Vec<String>,
    pub show_commands: bool,
    pub commands: Vec<Command>,
    pub selected_command: usize,
    /// App core for future AI chat implementation
    #[allow(dead_code)]
    pub core: Arc<AppCore>,
    pub should_clear: bool,
    pub command_history: Vec<String>,
    pub input_history: InputHistory,
    pub default_agent: Option<String>,
    pub default_model: Option<String>,
    pub syntax_highlighter: Option<SyntaxHighlighter>,
    pub last_total_height: u16,
    pub last_terminal_height: u16,
}

impl TuiApp {
    pub fn new(core: Arc<AppCore>, config: &CliConfig) -> Self {
        let commands = vec![
            Command {
                name: "/list".to_string(),
                description: "List all workflows".to_string(),
            },
            Command {
                name: "/run".to_string(),
                description: "Execute a workflow".to_string(),
            },
            Command {
                name: "/create".to_string(),
                description: "Create a new workflow".to_string(),
            },
            Command {
                name: "/help".to_string(),
                description: "Show help information".to_string(),
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

        let syntax_highlighter = if config.tui.syntax_highlight {
            Some(SyntaxHighlighter::new(theme_for_config(&config.tui.theme)))
        } else {
            None
        };

        Self {
            input: String::new(),
            cursor_position: 0,
            new_messages: Vec::new(),
            show_commands: false,
            commands,
            selected_command: 0,
            core,
            should_clear: false,
            command_history: Vec::new(),
            input_history: InputHistory::new(config.tui.history_size),
            default_agent: config.default.agent.clone(),
            default_model: config.default.model.clone(),
            syntax_highlighter,
            last_total_height: MIN_INPUT_HEIGHT,
            last_terminal_height: MIN_INPUT_HEIGHT,
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
        self.input_history.reset_navigation();
        self.refresh_command_menu();
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            let byte_idx = self.char_to_byte_idx(self.cursor_position);
            self.input.remove(byte_idx);
            self.input_history.reset_navigation();
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

    pub fn history_previous(&mut self) {
        if let Some(entry) = self.input_history.previous(&self.input) {
            self.input = entry;
            self.cursor_position = self.input.chars().count();
            self.show_commands = false;
        }
    }

    pub fn history_next(&mut self) {
        if let Some(entry) = self.input_history.next() {
            self.input = entry;
            self.cursor_position = self.input.chars().count();
            self.show_commands = false;
        }
    }

    async fn execute_shell(&mut self, input: &str) {
        let command = input.trim_start_matches('!').trim();
        if command.is_empty() {
            return;
        }

        self.new_messages.push(format!("$ {}", command));

        match shell::run_shell_command(command).await {
            Ok(output) => {
                if !output.stdout.trim().is_empty() {
                    self.new_messages.push(output.stdout.trim_end().to_string());
                }
                if !output.stderr.trim().is_empty() {
                    self.new_messages
                        .push(format!("❌ {}", output.stderr.trim_end()));
                }
                if let Some(code) = output.status {
                    if code != 0 {
                        self.new_messages.push(format!("Exit code: {code}"));
                    }
                }
            }
            Err(err) => {
                self.new_messages
                    .push(format!("❌ Failed to run command: {err}"));
            }
        }
    }

    pub async fn submit(&mut self) {
        if self.input.is_empty() {
            return;
        }

        // If commands are visible, treat submit as a selection first
        if self.show_commands {
            self.select_command();
            return;
        }

        let input = self.input.clone();
        self.new_messages.push(format!("> {}", input));

        // Skip repeated consecutive commands
        if self.command_history.is_empty() || self.command_history.last() != Some(&input) {
            self.command_history.push(input.clone());
        }

        self.input_history.add(input.clone());
        let _ = self.input_history.save();

        if input.starts_with('!') {
            self.execute_shell(&input).await;
            self.input.clear();
            self.cursor_position = 0;
            self.show_commands = false;
            return;
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
                if let Some(agent) = self.default_agent.as_ref() {
                    self.new_messages.push(format!("  Default agent: {agent}"));
                }
                if let Some(model) = self.default_model.as_ref() {
                    self.new_messages.push(format!("  Default model: {model}"));
                }
            }
            "/list" | "/run" | "/create" => {
                self.new_messages
                    .push("⚠️ Workflow commands are deprecated.".to_string());
                self.new_messages
                    .push("   RestFlow now uses an Agent-centric architecture.".to_string());
                self.new_messages.push(
                    "   Please use the web interface to manage agents and skills.".to_string(),
                );
            }
            cmd if cmd.starts_with("/run ") => {
                self.new_messages
                    .push("⚠️ Workflow commands are deprecated.".to_string());
                self.new_messages
                    .push("   RestFlow now uses an Agent-centric architecture.".to_string());
                self.new_messages.push(
                    "   Please use the web interface to manage agents and skills.".to_string(),
                );
            }
            _ => {
                if input.starts_with('/') {
                    self.new_messages
                        .push(format!("❌ Unknown command: {}", input));
                } else {
                    // AI chat mode - placeholder for future implementation
                    // TODO: Implement using restflow-ai AgentExecutor
                    self.new_messages
                        .push("🤖 AI chat is not yet implemented in CLI.".to_string());
                    self.new_messages.push(
                        "   Please use the web interface for agent interactions.".to_string(),
                    );
                }
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
