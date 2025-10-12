mod app;
mod custom_terminal;
mod welcome;

use anyhow::Result;
use app::TuiApp;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use custom_terminal::CustomTerminal;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use restflow_core::AppCore;
use std::{sync::Arc, time::Duration};

const COLOR_USER: &str = "\x1b[32m";
const COLOR_ERROR: &str = "\x1b[31m";
const COLOR_SUCCESS: &str = "\x1b[32m";
const COLOR_INFO: &str = "\x1b[90m";
const COLOR_RESET: &str = "\x1b[0m";

pub(super) const INPUT_DECORATION_LINES: u16 = 2;
pub(super) const MIN_INPUT_CONTENT_LINES: u16 = 1;
pub(super) const MIN_INPUT_HEIGHT: u16 = MIN_INPUT_CONTENT_LINES + INPUT_DECORATION_LINES;
const MAX_INPUT_CONTENT_LINES: u16 = 8;
const MAX_INPUT_HEIGHT: u16 = MAX_INPUT_CONTENT_LINES + INPUT_DECORATION_LINES;
const COMMAND_PANEL_MAX_HEIGHT: u16 = 10;
const VIEWPORT_MAX_HEIGHT: u16 = MAX_INPUT_HEIGHT + COMMAND_PANEL_MAX_HEIGHT;

/// Run the TUI interface using inline viewport mode without AlternateScreen
pub async fn run(core: Arc<AppCore>) -> Result<()> {
    welcome::show_welcome(false)?;
    enable_raw_mode()?;

    let mut custom_term = CustomTerminal::new()?;
    let (_, cursor_y) = crossterm::cursor::position()?;

    custom_term.setup_viewport_from(cursor_y, MIN_INPUT_HEIGHT)?;

    let mut app = TuiApp::new(core);
    let res = run_app(&mut custom_term, &mut app).await;

    disable_raw_mode()?;
    custom_term.show_cursor()?;
    println!();

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app(custom_term: &mut CustomTerminal, app: &mut TuiApp) -> Result<()> {
    loop {
        if app.should_clear {
            custom_term.insert_history_line("")?;
            custom_term
                .insert_history_line("═══════════════════════════════════════════════════")?;
            custom_term.insert_history_line("")?;
            app.should_clear = false;
        }

        for msg in app.new_messages.drain(..) {
            custom_term.insert_history_line(&format_message(&msg))?;
        }

        custom_term
            .terminal_mut()
            .draw(|f| render_bottom_ui(f, app))?;

        let viewport_height = app
            .last_total_height
            .max(MIN_INPUT_HEIGHT)
            .min(app.last_terminal_height.max(MIN_INPUT_HEIGHT))
            .min(VIEWPORT_MAX_HEIGHT);
        custom_term.adjust_viewport_height(viewport_height)?;

        if crossterm::event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Down | KeyCode::Tab if app.show_commands => {
                        app.next_command();
                    }
                    KeyCode::Up if app.show_commands => {
                        app.previous_command();
                    }
                    KeyCode::Enter => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            app.enter_char('\n');
                        } else {
                            app.submit().await;
                        }
                    }
                    KeyCode::Backspace => {
                        app.delete_char();
                    }
                    KeyCode::Left => {
                        app.move_cursor_left();
                    }
                    KeyCode::Right => {
                        app.move_cursor_right();
                    }
                    KeyCode::Char(c) => {
                        app.enter_char(c);
                    }
                    _ => {}
                }
            }
        }

        if app.input == "/exit" {
            return Ok(());
        }
    }
}

/// Format messages with colors
fn format_message(msg: &str) -> String {
    if msg.starts_with('>') {
        format!("{}{}{}", COLOR_USER, msg, COLOR_RESET)
    } else if msg.contains("error") || msg.contains("Error") || msg.contains("❌") {
        format!("{}{}{}", COLOR_ERROR, msg, COLOR_RESET)
    } else if msg.contains("✅") || msg.contains("success") || msg.contains("Success") {
        format!("{}{}{}", COLOR_SUCCESS, msg, COLOR_RESET)
    } else if msg.starts_with("  ") {
        format!("{}{}{}", COLOR_INFO, msg, COLOR_RESET)
    } else {
        msg.to_string()
    }
}

fn render_bottom_ui(f: &mut Frame, app: &mut TuiApp) {
    let terminal_height = f.size().height;
    let terminal_width = f.size().width;

    let available_width = terminal_width.max(1);
    let content_lines = app
        .visual_line_count(available_width)
        .max(MIN_INPUT_CONTENT_LINES);

    let mut input_height = content_lines
        .saturating_add(INPUT_DECORATION_LINES)
        .max(MIN_INPUT_HEIGHT);

    if terminal_height > 0 {
        input_height = input_height.min(terminal_height);
    }

    let mut panel_height = 0;
    if app.show_commands && terminal_height > input_height {
        let available_for_panel = terminal_height.saturating_sub(input_height);
        panel_height = available_for_panel.min(COMMAND_PANEL_MAX_HEIGHT);
    }

    let total_height = input_height
        .saturating_add(panel_height)
        .min(terminal_height);
    let input_y = terminal_height.saturating_sub(total_height);

    app.last_total_height = total_height;
    app.last_terminal_height = terminal_height;

    let input_area = Rect {
        x: 0,
        y: input_y,
        width: terminal_width,
        height: input_height.min(total_height),
    };

    render_input(f, input_area, app);

    let panel_area = Rect {
        x: 0,
        y: input_y + input_area.height,
        width: terminal_width,
        height: panel_height,
    };

    if app.show_commands {
        render_command_list(f, panel_area, app);
    } else if panel_area.height > 0 {
        f.render_widget(Clear, panel_area);
    }
}

fn render_input(f: &mut Frame, area: Rect, app: &TuiApp) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    f.render_widget(Clear, area);

    let horizontal = "─".repeat(area.width as usize);
    let line_style = Style::default().fg(Color::DarkGray);

    let top = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    f.render_widget(Paragraph::new(horizontal.clone()).style(line_style), top);

    if area.height > 1 {
        let bottom = Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        };
        f.render_widget(Paragraph::new(horizontal.clone()).style(line_style), bottom);
    }

    let content_height = area.height.saturating_sub(INPUT_DECORATION_LINES);
    let content_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: content_height,
    };

    if content_height > 0 {
        let text_width = area.width;
        let scroll = app.scroll_offset_for_width(content_height, text_width);
        let input = Paragraph::new(app.input.as_str())
            .style(Style::default().fg(Color::Yellow))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));
        f.render_widget(input, content_area);

        let (cursor_line, cursor_col) = app.cursor_visual_position_for_width(text_width);
        let visible_line = cursor_line.saturating_sub(scroll);
        let clamped_line = visible_line.min(content_height.saturating_sub(1));
        let clamped_col = cursor_col.min(area.width.saturating_sub(1));
        f.set_cursor(content_area.x + clamped_col, content_area.y + clamped_line);
    } else {
        let (_, cursor_col) = app.cursor_visual_position_for_width(area.width);
        let clamped_col = cursor_col.min(area.width.saturating_sub(1));
        f.set_cursor(area.x + clamped_col, area.y);
    }
}

fn render_command_list(f: &mut Frame, area: Rect, app: &TuiApp) {
    let items: Vec<ListItem> = app
        .commands
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let style = if i == app.selected_command {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let content = vec![
                Span::styled(
                    format!("{:<12}", cmd.name),
                    style.add_modifier(Modifier::BOLD),
                ),
                Span::raw(" - "),
                Span::styled(&cmd.description, style),
            ];

            ListItem::new(Line::from(content))
        })
        .collect();

    let commands_list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("[◉─◉] RestFlow commands (↑↓ select, Enter confirm)"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(commands_list, area);
}
