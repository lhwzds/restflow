use crate::tui::app::ChatTui;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Clear, List, ListItem},
};

pub fn draw_agent_selector(f: &mut Frame, app: &ChatTui) {
    let area = centered_rect(60, 50, f.area());
    let items: Vec<ListItem> = app
        .agents
        .iter()
        .enumerate()
        .map(|(idx, agent)| {
            let selected = idx == app.agent_index;
            let marker = if agent.id == app.current_agent_id {
                "●"
            } else {
                "○"
            };
            let content = format!(
                "{} {} ({})",
                marker,
                agent.name,
                agent.model.as_deref().unwrap_or("default")
            );
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Span::styled(content, style))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Select Agent (Enter to confirm, Esc to cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );

    f.render_widget(Clear, area);
    f.render_widget(list, area);
}

pub fn draw_model_selector(f: &mut Frame, app: &ChatTui) {
    let area = centered_rect(60, 50, f.area());
    let items: Vec<ListItem> = app
        .models
        .iter()
        .enumerate()
        .map(|(idx, model)| {
            let selected = idx == app.model_index;
            let marker = if model == &app.current_model {
                "●"
            } else {
                "○"
            };
            let content = format!("{} {}", marker, model);
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Span::styled(content, style))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Select Model (Enter to confirm, Esc to cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );

    f.render_widget(Clear, area);
    f.render_widget(list, area);
}

pub fn draw_session_selector(f: &mut Frame, app: &ChatTui) {
    let area = centered_rect(60, 50, f.area());
    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(idx, session)| {
            let selected = idx == app.session_index;
            let marker = if Some(&session.id) == app.current_session_id.as_ref() {
                "●"
            } else {
                "○"
            };
            let content = format!("{} {}", marker, session.label);
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Span::styled(content, style))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Select Session (Enter to confirm, Esc to cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );

    f.render_widget(Clear, area);
    f.render_widget(list, area);
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
