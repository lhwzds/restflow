mod state;
mod stream;
mod viewport;
mod welcome;

use anyhow::Result;
use crossterm::{
    QueueableCommand,
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{
        BeginSynchronizedUpdate, Clear as TermClear, ClearType, EndSynchronizedUpdate,
        disable_raw_mode, enable_raw_mode,
    },
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use restflow_core::AppCore;
use state::TuiApp;
use std::{io::Write, sync::Arc, time::Duration};
use viewport::ViewportTerminal;

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

    let mut terminal = ViewportTerminal::new()?;
    let (_, cursor_y) = crossterm::cursor::position()?;

    terminal.setup_viewport_from(cursor_y, MIN_INPUT_HEIGHT)?;

    let mut app = TuiApp::new(core);

    let res = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    terminal.show_cursor()?;
    println!();

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app(terminal: &mut ViewportTerminal, app: &mut TuiApp) -> Result<()> {
    loop {
        if app.should_clear {
            terminal.insert_history_line("")?;
            terminal.insert_history_line("═══════════════════════════════════════════════════")?;
            terminal.insert_history_line("")?;
            app.should_clear = false;
        }

        for msg in app.new_messages.drain(..) {
            terminal.insert_history_line(&format_message(&msg))?;
        }

        let viewport_height = app
            .last_total_height
            .max(MIN_INPUT_HEIGHT)
            .min(app.last_terminal_height.max(MIN_INPUT_HEIGHT))
            .min(VIEWPORT_MAX_HEIGHT);

        std::io::stdout().queue(BeginSynchronizedUpdate)?.flush()?;

        terminal.adjust_viewport_height(viewport_height)?;

        let clear_from_y = terminal.viewport_start_y() + viewport_height;
        let term = terminal.terminal_mut();

        execute!(term.backend_mut(), MoveTo(0, clear_from_y))?;
        execute!(term.backend_mut(), TermClear(ClearType::FromCursorDown))?;

        term.current_buffer_mut().reset();

        let viewport_start_y = terminal.viewport_start_y();
        terminal
            .terminal_mut()
            .draw(|f| render_bottom_ui(f, app, viewport_start_y))?;

        std::io::stdout().queue(EndSynchronizedUpdate)?.flush()?;

        if crossterm::event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(());
                }
                KeyCode::Esc => {
                    app.cancel_streaming();
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

        app.poll_events().await;

        if app.input == "/exit" {
            return Ok(());
        }
    }
}

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

fn render_bottom_ui(f: &mut Frame, app: &mut TuiApp, viewport_start_y: u16) {
    let terminal_area = f.area();
    let terminal_height = terminal_area.height;
    let terminal_width = terminal_area.width;

    let available_width = terminal_width.max(1);
    let content_lines = app
        .visual_line_count(available_width)
        .max(MIN_INPUT_CONTENT_LINES);

    let mut input_height = content_lines
        .saturating_add(INPUT_DECORATION_LINES)
        .clamp(MIN_INPUT_HEIGHT, MAX_INPUT_HEIGHT);

    if terminal_height > 0 {
        input_height = input_height.min(terminal_height);
    }

    let input_y = viewport_start_y;

    let mut panel_height = 0;
    if app.show_commands {
        let num_commands = app.commands.len() as u16;
        let border_lines = 2;
        panel_height = (num_commands + border_lines).min(COMMAND_PANEL_MAX_HEIGHT);
    }

    let total_height = input_height
        .saturating_add(panel_height)
        .min(terminal_height);

    app.last_total_height = total_height;
    app.last_terminal_height = terminal_height;

    let input_area = Rect {
        x: 0,
        y: input_y,
        width: terminal_width,
        height: input_height,
    };

    render_input(f, input_area, app);

    let panel_area = Rect {
        x: 0,
        y: input_y + input_height,
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

    let agent_name = if app.current_agent_name.is_empty() {
        "No agent"
    } else {
        app.current_agent_name.as_str()
    };
    let status_text = format!(
        " {} | {} | Tokens: {}/{} ",
        agent_name,
        app.activity_status,
        app.token_counter.input,
        app.token_counter.output
    );
    let mut top_line = status_text;
    if top_line.len() < area.width as usize {
        top_line.push_str(&"─".repeat(area.width as usize - top_line.len()));
    } else {
        top_line.truncate(area.width as usize);
    }

    let top = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    f.render_widget(Paragraph::new(top_line).style(line_style), top);

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
        f.set_cursor_position(Position::new(
            content_area.x + clamped_col,
            content_area.y + clamped_line,
        ));
    } else {
        let (_, cursor_col) = app.cursor_visual_position_for_width(area.width);
        let clamped_col = cursor_col.min(area.width.saturating_sub(1));
        f.set_cursor_position(Position::new(area.x + clamped_col, area.y));
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
