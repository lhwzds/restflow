use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use super::composer::ComposerMode;
use super::state::{AppState, OverlayState, TeamOverlayTab};
use super::transcript::ShellMessage;

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(5),
        ])
        .split(frame.area());

    render_header(frame, layout[0], state);
    render_transcript(frame, layout[1], state);
    render_composer(frame, layout[2], state);

    if let Some(overlay) = &state.overlay {
        render_overlay(frame, centered_rect(frame.area()), state, overlay);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let agent = state
        .default_agent_name
        .clone()
        .unwrap_or_else(|| "No default agent".to_string());
    let session = state
        .current_session()
        .map(|session| session.name.clone())
        .unwrap_or_else(|| "No session".to_string());
    let overlay = match state.overlay.as_ref() {
        Some(OverlayState::SessionPicker { .. }) => "sessions",
        Some(OverlayState::RunPicker { .. }) => "runs",
        Some(OverlayState::ApprovalPicker { .. }) => "approvals",
        Some(OverlayState::TeamView { .. }) => "team",
        Some(OverlayState::Help) => "help",
        None => "none",
    };
    let status = if state.is_streaming { "streaming" } else { "idle" };
    let mode = match state.composer.mode() {
        ComposerMode::Compose => "compose",
        ComposerMode::Command => "command",
    };
    let line = Line::from(vec![
        Span::styled("RestFlow ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(format!(
            "agent={agent}  session={session}  focus={}  overlay={overlay}  state={status}  mode={mode}",
            state.focus_label()
        )),
    ]);
    frame.render_widget(Paragraph::new(vec![line]), area);
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let lines = if state.transcript.is_empty() {
        vec![Line::from("Start typing to talk to the default agent.")]
    } else {
        state
            .transcript
            .iter()
            .flat_map(render_transcript_lines)
            .collect::<Vec<_>>()
    };

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Conversation"))
            .wrap(Wrap { trim: false })
            .scroll((state.transcript_scroll, 0)),
        area,
    );
}

fn render_transcript_lines(message: &ShellMessage) -> Vec<Line<'static>> {
    let (label, color, content) = match message {
        ShellMessage::UserMessage { content } => ("user", Color::Yellow, content.as_str()),
        ShellMessage::AssistantMessage { content } => {
            ("assistant", Color::Green, content.as_str())
        }
        ShellMessage::SystemMessage { content } => ("system", Color::Blue, content.as_str()),
        ShellMessage::AssistantStream { content } => {
            ("assistant...", Color::Green, content.as_str())
        }
        ShellMessage::StreamAck { content } => ("ack", Color::Gray, content.as_str()),
        ShellMessage::ToolCall {
            call_id,
            name,
            arguments,
        } => {
            return vec![Line::from(vec![
                Span::styled("[tool] ", Style::default().fg(Color::Magenta)),
                Span::raw(format!("{name}#{call_id} {arguments}")),
            ])];
        }
        ShellMessage::ToolResult {
            call_id,
            success,
            result,
        } => {
            return vec![Line::from(vec![
                Span::styled("[result] ", Style::default().fg(Color::Magenta)),
                Span::raw(format!("#{call_id} success={success} {result}")),
            ])];
        }
        ShellMessage::SessionNotice { content } => ("session", Color::Cyan, content.as_str()),
        ShellMessage::TaskNotice { content } => ("task", Color::LightBlue, content.as_str()),
        ShellMessage::ApprovalNotice {
            approval_id,
            content,
        } => {
            let suffix = approval_id
                .as_ref()
                .map(|approval_id| format!(" ({approval_id})"))
                .unwrap_or_default();
            return vec![Line::from(vec![
                Span::styled("[approval] ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{content}{suffix}")),
            ])];
        }
        ShellMessage::TeamNotice { content } => ("team", Color::Cyan, content.as_str()),
        ShellMessage::InfoNotice { content } => ("info", Color::Gray, content.as_str()),
        ShellMessage::ErrorNotice { content } => ("error", Color::Red, content.as_str()),
    };

    let mut lines = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if index == 0 {
            lines.push(Line::from(vec![
                Span::styled(format!("[{label}] "), Style::default().fg(color)),
                Span::raw(line.to_string()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("          "),
                Span::raw(line.to_string()),
            ]));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(format!("[{label}] "), Style::default().fg(color)),
            Span::raw(String::new()),
        ]));
    }

    lines
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(2)])
        .split(area);

    frame.render_widget(
        Paragraph::new(state.composer.draft())
            .block(
                Block::default().borders(Borders::ALL).title(match state.composer.mode() {
                    ComposerMode::Compose => "Compose",
                    ComposerMode::Command => "Command",
                }),
            )
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let helper = match state.composer.mode() {
        ComposerMode::Compose => {
            "Enter send  Ctrl+J newline  Ctrl+P sessions  Ctrl+R runs  Ctrl+A approvals  Ctrl+G team  ? help"
        }
        ComposerMode::Command => "Slash command mode: /help /task /run /team /approve /reject",
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(state.status.clone()),
            Line::from(helper),
        ])
        .block(Block::default().borders(Borders::TOP).title("Status")),
        chunks[1],
    );

    let (cursor_column, cursor_row) = state.composer.cursor_position();
    let cursor_x = chunks[0].x + 1 + cursor_column;
    let cursor_y = chunks[0].y + 1 + cursor_row;
    frame.set_cursor_position((
        cursor_x.min(chunks[0].right().saturating_sub(2)),
        cursor_y.min(chunks[0].bottom().saturating_sub(2)),
    ));
}

fn render_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState, overlay: &OverlayState) {
    frame.render_widget(Clear, area);
    match overlay {
        OverlayState::SessionPicker { selected } => {
            let items = state
                .sessions
                .iter()
                .enumerate()
                .map(|(index, session)| {
                    let prefix = if index == *selected { "▸ " } else { "  " };
                    ListItem::new(format!("{prefix}{}", session.name))
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                List::new(items).block(Block::default().borders(Borders::ALL).title("Sessions")),
                area,
            );
        }
        OverlayState::RunPicker { selected } => {
            let items = state
                .run_picker_items()
                .into_iter()
                .enumerate()
                .map(|(index, item)| {
                    let prefix = if index == *selected { "▸ " } else { "  " };
                    let super::state::RunPickerItem::Run {
                        run_id,
                        title,
                        status,
                    } = item;
                    ListItem::new(format!("{prefix}[run] {title} ({status}) {run_id}"))
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                List::new(items).block(Block::default().borders(Borders::ALL).title("Session Runs")),
                area,
            );
        }
        OverlayState::ApprovalPicker { selected } => {
            let items = if state.current_team_approvals.is_empty() {
                vec![ListItem::new("No pending approvals.")]
            } else {
                state
                    .current_team_approvals
                    .iter()
                    .enumerate()
                    .map(|(index, approval)| {
                        let prefix = if index == *selected { "▸ " } else { "  " };
                        ListItem::new(format!(
                            "{prefix}{} {} {}",
                            approval.approval_id, approval.member_id, approval.content
                        ))
                    })
                    .collect::<Vec<_>>()
            };
            frame.render_widget(
                List::new(items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Approvals (Enter approve, r reject)"),
                ),
                area,
            );
        }
        OverlayState::TeamView { tab, scroll } => {
            let lines = match tab {
                TeamOverlayTab::Members => state
                    .current_team_state
                    .as_ref()
                    .map(|team| {
                        team.members
                            .iter()
                            .map(|member| {
                                Line::from(format!(
                                    "{} {:?} task={:?} assignment={:?}",
                                    member.member_id, member.status, member.task_id, member.current_assignment_id
                                ))
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| vec![Line::from("No active team loaded.")]),
                TeamOverlayTab::Messages => state
                    .current_team_messages
                    .iter()
                    .map(|message| {
                        Line::from(format!(
                            "{:?} {} -> {:?}: {}",
                            message.kind, message.from_member_id, message.to_member_id, message.content
                        ))
                    })
                    .collect::<Vec<_>>(),
                TeamOverlayTab::Assignments => state
                    .current_team_assignments
                    .iter()
                    .map(|assignment| {
                        Line::from(format!(
                            "{} {:?} {}",
                            assignment.assignee_member_id, assignment.status, assignment.content
                        ))
                    })
                    .collect::<Vec<_>>(),
                TeamOverlayTab::Approvals => state
                    .current_team_approvals
                    .iter()
                    .map(|approval| {
                        Line::from(format!(
                            "{} {:?} {}",
                            approval.approval_id, approval.status, approval.content
                        ))
                    })
                    .collect::<Vec<_>>(),
            };
            let title = match tab {
                TeamOverlayTab::Members => "Team / Members",
                TeamOverlayTab::Messages => "Team / Messages",
                TeamOverlayTab::Assignments => "Team / Assignments",
                TeamOverlayTab::Approvals => "Team / Approvals",
            };
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("{title} (←/→ switch tab, Esc close)")),
                    )
                    .wrap(Wrap { trim: false })
                    .scroll((*scroll, 0)),
                area,
            );
        }
        OverlayState::Help => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from("RestFlow Agent Shell"),
                    Line::from("Ctrl+P sessions"),
                    Line::from("Ctrl+R runs"),
                    Line::from("Ctrl+A approvals"),
                    Line::from("Ctrl+G team"),
                    Line::from("Enter send, Ctrl+J newline, Esc close overlay"),
                    Line::from("Slash: /help /task /run /team /approve /reject"),
                ])
                .block(Block::default().borders(Borders::ALL).title("Help")),
                area,
            );
        }
    }
}

fn centered_rect(area: Rect) -> Rect {
    let popup = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ])
        .split(popup[1])[1]
}
