use crate::tui::{
    app::{ChatTui, TuiMode},
    messages::build_message_items,
    selectors::{draw_agent_selector, draw_model_selector, draw_session_selector},
    theme,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, Paragraph, Wrap},
};

const MIN_INPUT_HEIGHT: u16 = 3;
const MAX_INPUT_HEIGHT: u16 = 9;

pub fn draw(f: &mut Frame, app: &mut ChatTui) {
    let area = f.area();
    let input_height = input_height(app, area.width);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(input_height),
        ])
        .split(area);

    draw_header(f, chunks[0], app);
    draw_messages(f, chunks[1], app);
    draw_status(f, chunks[2], app);
    draw_input(f, chunks[3], app);

    match app.mode {
        TuiMode::AgentSelect => draw_agent_selector(f, app),
        TuiMode::ModelSelect => draw_model_selector(f, app),
        TuiMode::SessionSelect => draw_session_selector(f, app),
        TuiMode::Help => draw_help(f),
        TuiMode::Chat => {}
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &ChatTui) {
    let header = format!(
        " RestFlow | Agent: {} | Model: {}",
        app.current_agent_name, app.current_model
    );
    let widget = Paragraph::new(header).style(theme::header_style());
    f.render_widget(widget, area);
}

fn draw_messages(f: &mut Frame, area: Rect, app: &ChatTui) {
    let items = build_message_items(app);
    let list = List::new(items).block(Block::default().borders(Borders::NONE));
    f.render_widget(list, area);
}

fn draw_status(f: &mut Frame, area: Rect, app: &ChatTui) {
    let status = format!(
        " {} | {} tokens | Ctrl+G: agent | Ctrl+L: model | Ctrl+N: new | /help",
        app.activity_status,
        app.token_count.total()
    );
    let widget = Paragraph::new(status).style(theme::status_style());
    f.render_widget(widget, area);
}

fn draw_input(f: &mut Frame, area: Rect, app: &mut ChatTui) {
    let border_style = theme::border_style();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Input ");

    let content_area = block.inner(area);
    f.render_widget(block, area);

    if content_area.height == 0 || content_area.width == 0 {
        return;
    }

    let scroll = app.scroll_offset(content_area.height, content_area.width);
    let input = Paragraph::new(app.input.as_str())
        .style(theme::input_style())
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(input, content_area);

    let (cursor_line, cursor_col) = app.cursor_visual_position(content_area.width);
    let visible_line = cursor_line.saturating_sub(scroll);
    let clamped_line = visible_line.min(content_area.height.saturating_sub(1));
    let clamped_col = cursor_col.min(content_area.width.saturating_sub(1));
    f.set_cursor_position((content_area.x + clamped_col, content_area.y + clamped_line));
}

fn draw_help(f: &mut Frame) {
    let area = centered_rect(70, 60, f.area());
    let lines = vec![
        Line::from(Span::styled("Keyboard", Style::default().fg(Color::Yellow))),
        Line::from("  Ctrl+G: Agent selector"),
        Line::from("  Ctrl+L: Model selector"),
        Line::from("  Ctrl+P: Session selector"),
        Line::from("  Ctrl+N: New session"),
        Line::from("  Ctrl+O: Toggle tools"),
        Line::from("  Ctrl+T: Toggle thinking"),
        Line::from("  Ctrl+C: Exit"),
        Line::from("  Esc: Cancel/close"),
        Line::from("  Shift+Enter: New line"),
        Line::from(""),
        Line::from(Span::styled(
            "Slash commands",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  /help, /clear, /exit"),
        Line::from("  /agent <id>, /model <name>, /session <id>"),
        Line::from("  /new, /history, /memory <query>, /export"),
        Line::from("  /think on|off, /verbose on|off"),
    ];

    let help = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Help (Esc to close) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(Clear, area);
    f.render_widget(help, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn input_height(app: &ChatTui, width: u16) -> u16 {
    let lines = app.input_line_count(width.max(1)).max(1);
    let desired = lines.saturating_add(2);
    desired.clamp(MIN_INPUT_HEIGHT, MAX_INPUT_HEIGHT)
}
