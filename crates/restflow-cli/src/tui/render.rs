use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use super::state::{AppState, OverlayState, RunPickerItem, TeamOverlayTab, TranscriptKind};

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
        .current_session
        .as_ref()
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
    let line = Line::from(vec![
        Span::styled("RestFlow ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(format!("agent={agent}  session={session}  overlay={overlay}  state={status}")),
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
            .map(|entry| render_transcript_line(entry.kind, &entry.text))
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

fn render_transcript_line(kind: TranscriptKind, text: &str) -> Line<'_> {
    let (label, color) = match kind {
        TranscriptKind::User => ("user", Color::Yellow),
        TranscriptKind::Assistant => ("assistant", Color::Green),
        TranscriptKind::System => ("system", Color::Blue),
        TranscriptKind::Ack => ("ack", Color::Gray),
        TranscriptKind::Data => ("stream", Color::White),
        TranscriptKind::ToolCall => ("tool", Color::Magenta),
        TranscriptKind::ToolResult => ("result", Color::Magenta),
        TranscriptKind::SessionEvent => ("session", Color::Cyan),
        TranscriptKind::TaskEvent => ("task", Color::LightBlue),
        TranscriptKind::Info => ("info", Color::Gray),
        TranscriptKind::Error => ("error", Color::Red),
    };
    Line::from(vec![
        Span::styled(format!("[{label}] "), Style::default().fg(color)),
        Span::raw(text.to_string()),
    ])
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(2)])
        .split(area);

    frame.render_widget(
        Paragraph::new(state.input.text.as_str())
            .block(Block::default().borders(Borders::ALL).title("Compose"))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(state.status.clone()),
            Line::from("Enter send  Ctrl+J newline  Ctrl+P sessions  Ctrl+R runs  Ctrl+A approvals  Ctrl+G team  ? help"),
        ])
        .block(Block::default().borders(Borders::TOP).title("Status")),
        chunks[1],
    );

    let cursor_x = chunks[0].x + 1 + state.input.cursor as u16;
    let cursor_y = chunks[0].y + 1;
    frame.set_cursor_position((cursor_x.min(chunks[0].right().saturating_sub(2)), cursor_y));
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
                    match item {
                        RunPickerItem::Task { id, title, status } => {
                            ListItem::new(format!("{prefix}[task] {title} ({status}) {id}"))
                        }
                        RunPickerItem::Run {
                            run_id,
                            title,
                            status,
                        } => ListItem::new(format!("{prefix}[run] {title} ({status}) {run_id}")),
                    }
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                List::new(items).block(Block::default().borders(Borders::ALL).title("Runs & Tasks")),
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
                    Line::from("Ctrl+R runs/tasks"),
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
