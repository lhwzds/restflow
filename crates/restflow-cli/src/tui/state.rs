//! TUI application state

use super::MIN_INPUT_HEIGHT;
use restflow_core::AppCore;
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

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
    pub last_total_height: u16,
    pub last_terminal_height: u16,
}

impl TuiApp {
    pub fn new(core: Arc<AppCore>) -> Self {
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
        } else {
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

#[cfg(test)]
mod tests {
    use super::TuiApp;
    use restflow_core::AppCore;
    use std::sync::Arc;

    async fn test_core() -> Arc<AppCore> {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let db_path = temp_dir.path().join("test.db");
        Arc::new(
            AppCore::new(db_path.to_str().expect("db path"))
                .await
                .expect("core"),
        )
    }

    #[tokio::test]
    async fn test_input_insert_delete() {
        let core = test_core().await;
        let mut app = TuiApp::new(core);
        app.enter_char('H');
        app.enter_char('i');
        assert_eq!(app.input, "Hi");
        assert_eq!(app.cursor_position, 2);

        app.delete_char();
        assert_eq!(app.input, "H");
        assert_eq!(app.cursor_position, 1);
    }

    #[tokio::test]
    async fn test_command_menu_visibility() {
        let core = test_core().await;
        let mut app = TuiApp::new(core);
        app.enter_char('/');
        assert!(app.show_commands);

        app.enter_char('h');
        assert!(!app.show_commands);
    }

    #[tokio::test]
    async fn test_submit_help_command() {
        let core = test_core().await;
        let mut app = TuiApp::new(core);
        app.input = "/help".to_string();
        app.cursor_position = app.input.chars().count();
        app.submit().await;

        assert!(
            app.new_messages
                .iter()
                .any(|msg| msg.contains("Available commands"))
        );
        assert!(app.input.is_empty());
    }

    #[tokio::test]
    async fn test_submit_unknown_command() {
        let core = test_core().await;
        let mut app = TuiApp::new(core);
        app.input = "/unknown".to_string();
        app.cursor_position = app.input.chars().count();
        app.submit().await;

        assert!(
            app.new_messages
                .iter()
                .any(|msg| msg.contains("Unknown command"))
        );
    }
}
