use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use restflow_traits::{TeamApprovalStatus, TeamAssignmentStatus, TeamMemberStatus, TeamMessageKind, TeamRole};

use super::composer::ComposerMode;
use super::state::{AppState, OverlayState, TeamOverlayTab};
use super::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    if state.is_startup_mode() {
        render_startup(frame, state);
        return;
    }

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

fn render_startup(frame: &mut Frame<'_>, state: &AppState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(8)])
        .split(frame.area());

    render_header(frame, layout[0], state);

    let startup = state.startup_state().expect("startup state");
    let mut lines = vec![
        Line::from(Span::styled(
            "RestFlow daemon is not running",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::default(),
    ];

    if startup.starting_daemon {
        lines.push(Line::from("Starting daemon..."));
    } else {
        lines.push(Line::from("Enter  Start Daemon"));
        lines.push(Line::from("Esc    Exit"));
        lines.push(Line::default());
        lines.push(Line::from("Use `restflow daemon start` for manual control."));
    }

    if let Some(error) = startup.error.as_ref() {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(Color::Red),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Startup")),
        centered_rect(layout[1]),
    );
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(12), Constraint::Min(20), Constraint::Length(24)])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "RestFlow",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )])),
        chunks[0],
    );

    let agent = state
        .default_agent_name
        .clone()
        .unwrap_or_else(|| "No Agent".to_string());
    let session = state
        .current_session()
        .map(|session| session.name.clone())
        .unwrap_or_else(|| "No Session".to_string());
    frame.render_widget(
        Paragraph::new(Line::from(format!("{agent} · {session}"))),
        chunks[1],
    );

    frame.render_widget(
        Paragraph::new(shell_status_text(state)).alignment(Alignment::Right),
        chunks[2],
    );
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let cells = state.transcript_cells_for_render();
    let lines = if cells.is_empty() {
        Vec::new()
    } else {
        let cell_lines = cells
            .iter()
            .flat_map(render_transcript_cell)
            .collect::<Vec<_>>();
        bottom_anchor_lines(cell_lines, area.height as usize, state.transcript_scroll as usize)
    };

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left),
        area,
    );
}

fn render_transcript_cell(cell: &TranscriptCell) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    let mut title_spans = vec![Span::styled(
        cell.title.clone(),
        Style::default()
            .fg(cell_color(cell))
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(subtitle) = &cell.subtitle {
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled(
            subtitle.clone(),
            Style::default().fg(Color::DarkGray),
        ));
    }
    lines.push(Line::from(title_spans));

    let indent = match cell.group {
        MessageGroup::Conversation | MessageGroup::RuntimeNotice | MessageGroup::ToolActivity => "  ",
    };
    for line in cell.body.lines() {
        lines.push(Line::from(vec![
            Span::raw(indent),
            Span::raw(line.to_string()),
        ]));
    }

    if cell.body.is_empty() {
        lines.push(Line::from("  "));
    }

    lines.push(Line::default());
    lines
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(2)])
        .split(area);

    let composer_title = match state.composer.mode() {
        ComposerMode::Compose => "Message",
        ComposerMode::Command => "Command",
    };
    let composer_lines = if state.composer.draft().is_empty() {
        vec![Line::default()]
    } else {
        state
            .composer
            .draft()
            .lines()
            .map(|line| Line::from(line.to_string()))
            .collect::<Vec<_>>()
    };

    frame.render_widget(
        Paragraph::new(composer_lines)
            .block(Block::default().borders(Borders::ALL).title(composer_title))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(state.status.clone()),
            Line::from(footer_hint_line(state)),
        ])
        .block(Block::default().borders(Borders::TOP)),
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

fn bottom_anchor_lines(
    lines: Vec<Line<'static>>,
    height: usize,
    scroll_from_bottom: usize,
) -> Vec<Line<'static>> {
    if height == 0 {
        return Vec::new();
    }
    let total = lines.len();
    let end = total.saturating_sub(scroll_from_bottom);
    let start = end.saturating_sub(height);
    let mut visible = lines[start..end].to_vec();
    if visible.len() < height {
        let mut padding = vec![Line::default(); height - visible.len()];
        padding.append(&mut visible);
        return padding;
    }
    visible
}

fn render_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState, overlay: &OverlayState) {
    frame.render_widget(Clear, area);
    match overlay {
        OverlayState::SessionPicker { selected } => render_list_overlay(
            frame,
            area,
            overlay_title(overlay),
            state
                .sessions
                .iter()
                .enumerate()
                .map(|(index, session)| {
                    let prefix = if index == *selected { "▸ " } else { "  " };
                    format!("{prefix}{}", session.name)
                })
                .collect(),
            "No sessions yet",
            overlay_hint(overlay),
        ),
        OverlayState::RunPicker { selected } => render_list_overlay(
            frame,
            area,
            overlay_title(overlay),
            state
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
                    format!("{prefix}{title} · {status} · {}", short_id(&run_id))
                })
                .collect(),
            "No runs in this session",
            overlay_hint(overlay),
        ),
        OverlayState::ApprovalPicker { selected } => render_list_overlay(
            frame,
            area,
            overlay_title(overlay),
            state
                .current_team_approvals
                .iter()
                .enumerate()
                    .map(|(index, approval)| {
                        let prefix = if index == *selected { "▸ " } else { "  " };
                        format!(
                            "{prefix}{} · {} · #{}",
                            approval.member_id,
                            approval.content,
                            short_id(&approval.approval_id)
                        )
                    })
                    .collect(),
            "No pending approvals",
            overlay_hint(overlay),
        ),
        OverlayState::TeamView { tab, scroll } => render_team_overlay(frame, area, state, *tab, *scroll),
        OverlayState::Help => render_help_overlay(frame, area),
    }
}

fn render_list_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: Vec<String>,
    empty_state: &str,
    hint: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(area);

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(empty_state)
                .block(Block::default().borders(Borders::ALL).title(title))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false }),
            chunks[0],
        );
    } else {
        frame.render_widget(
            List::new(items.into_iter().map(ListItem::new).collect::<Vec<_>>())
                .block(Block::default().borders(Borders::ALL).title(title)),
            chunks[0],
        );
    }

    frame.render_widget(
        Paragraph::new(hint).block(Block::default().borders(Borders::TOP)),
        chunks[1],
    );
}

fn render_team_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    tab: TeamOverlayTab,
    scroll: u16,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(area);

    let mut lines = vec![team_tab_line(tab), Line::default()];
    lines.extend(match tab {
        TeamOverlayTab::Members => {
            if let Some(team) = state.current_team_state.as_ref() {
                team.members
                    .iter()
                    .map(|member| {
                        Line::from(format!(
                            "{} · {} · {}",
                            member.member_id,
                            team_role_label(member.role),
                            team_member_status_label(member.status)
                        ))
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![Line::from("No team context loaded")]
            }
        }
        TeamOverlayTab::Messages => {
            if state.current_team_messages.is_empty() {
                vec![Line::from("No team messages")]
            } else {
                state
                    .current_team_messages
                    .iter()
                    .map(|message| {
                        let destination = message
                            .to_member_id
                            .as_deref()
                            .unwrap_or("everyone");
                        Line::from(format!(
                            "{} · {} → {} · {}",
                            team_message_kind_label(message.kind),
                            message.from_member_id,
                            destination,
                            message.content
                        ))
                    })
                    .collect::<Vec<_>>()
            }
        }
        TeamOverlayTab::Assignments => {
            if state.current_team_assignments.is_empty() {
                vec![Line::from("No team assignments")]
            } else {
                state
                    .current_team_assignments
                    .iter()
                    .map(|assignment| {
                        Line::from(format!(
                            "{} · {} · {}",
                            assignment.assignee_member_id,
                            team_assignment_status_label(assignment.status),
                            assignment.content
                        ))
                    })
                    .collect::<Vec<_>>()
            }
        }
        TeamOverlayTab::Approvals => {
            if state.current_team_approvals.is_empty() {
                vec![Line::from("No pending approvals")]
            } else {
                state
                    .current_team_approvals
                    .iter()
                    .map(|approval| {
                        Line::from(format!(
                            "#{} · {} · {}",
                            approval.approval_id,
                            team_approval_status_label(approval.status),
                            approval.content
                        ))
                    })
                    .collect::<Vec<_>>()
            }
        }
    });

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Team"))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(overlay_hint(&OverlayState::TeamView { tab, scroll }))
            .block(Block::default().borders(Borders::TOP)),
        chunks[1],
    );
}

fn render_help_overlay(frame: &mut Frame<'_>, area: Rect) {
    let lines = vec![
        Line::from("Ctrl+P  Switch Session"),
        Line::from("Ctrl+R  Open Run"),
        Line::from("Ctrl+A  Pending Approvals"),
        Line::from("Ctrl+G  Team"),
        Line::from("Enter   Send / Select"),
        Line::from("Ctrl+J  New Line"),
        Line::from("Esc     Close / Back"),
        Line::from("?       Keyboard Shortcuts"),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Keyboard Shortcuts"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn shell_status_text(state: &AppState) -> String {
    if let Some(startup) = state.startup_state() {
        return if startup.starting_daemon {
            "Starting Daemon".to_string()
        } else {
            "Daemon Offline".to_string()
        };
    }

    match state.overlay.as_ref() {
        Some(OverlayState::SessionPicker { .. }) => "Switching Session".to_string(),
        Some(OverlayState::RunPicker { .. }) => "Opening Run".to_string(),
        Some(OverlayState::ApprovalPicker { .. }) => "Pending Approvals".to_string(),
        Some(OverlayState::TeamView { .. }) => "Viewing Team".to_string(),
        Some(OverlayState::Help) => "Keyboard Shortcuts".to_string(),
        None if state.is_streaming => "Streaming".to_string(),
        None if state.focus_label().starts_with("run:") => "Viewing Run".to_string(),
        None if !state.current_team_approvals.is_empty() => "Pending Approvals".to_string(),
        None => "Idle".to_string(),
    }
}

fn footer_hint_line(state: &AppState) -> String {
    footer_shortcuts(state).join(" · ")
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

fn footer_shortcuts(state: &AppState) -> Vec<&'static str> {
    match state.overlay.as_ref() {
        Some(OverlayState::SessionPicker { .. }) | Some(OverlayState::RunPicker { .. }) => {
            vec!["Enter Select", "Esc Close", "↑↓ Move"]
        }
        Some(OverlayState::ApprovalPicker { .. }) => {
            vec!["Enter Approve", "R Reject", "Esc Close", "↑↓ Move"]
        }
        Some(OverlayState::TeamView { .. }) => vec!["Esc Close", "↑↓ Scroll", "←→ Tabs"],
        Some(OverlayState::Help) => vec!["Esc Close"],
        None => match state.composer.mode() {
            ComposerMode::Compose => vec![
                "Enter Send",
                "Ctrl+J New Line",
                "Ctrl+P Sessions",
                "Ctrl+R Runs",
                "Ctrl+A Approvals",
                "? Shortcuts",
            ],
            ComposerMode::Command => vec![
                "Enter Run Command",
                "Esc Back",
                "/help",
                "/run open",
                "/team state",
            ],
        },
    }
}

fn overlay_title(overlay: &OverlayState) -> &'static str {
    match overlay {
        OverlayState::SessionPicker { .. } => "Switch Session",
        OverlayState::RunPicker { .. } => "Open Run",
        OverlayState::ApprovalPicker { .. } => "Pending Approvals",
        OverlayState::TeamView { .. } => "Team",
        OverlayState::Help => "Keyboard Shortcuts",
    }
}

fn overlay_hint(overlay: &OverlayState) -> &'static str {
    match overlay {
        OverlayState::SessionPicker { .. } | OverlayState::RunPicker { .. } => {
            "Enter Select · Esc Close · ↑↓ Move"
        }
        OverlayState::ApprovalPicker { .. } => {
            "Enter Approve · R Reject · Esc Close · ↑↓ Move"
        }
        OverlayState::TeamView { .. } => "Esc Close · ↑↓ Scroll · ←→ Tabs",
        OverlayState::Help => "Esc Close",
    }
}

fn cell_color(cell: &TranscriptCell) -> Color {
    match cell.kind {
        TranscriptCellKind::User => Color::Yellow,
        TranscriptCellKind::Assistant => Color::Green,
        TranscriptCellKind::System => Color::Blue,
        TranscriptCellKind::Notice => Color::Cyan,
        TranscriptCellKind::Tool => Color::Magenta,
    }
}

fn team_tab_line(active: TeamOverlayTab) -> Line<'static> {
    let tabs = [
        (TeamOverlayTab::Members, "Members"),
        (TeamOverlayTab::Messages, "Messages"),
        (TeamOverlayTab::Assignments, "Assignments"),
        (TeamOverlayTab::Approvals, "Approvals"),
    ];

    let mut spans = Vec::new();
    for (index, (tab, label)) in tabs.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" · "));
        }
        spans.push(Span::styled(
            label,
            if tab == active {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
        ));
    }
    Line::from(spans)
}

fn team_role_label(role: TeamRole) -> &'static str {
    match role {
        TeamRole::Leader => "Leader",
        TeamRole::Member => "Member",
    }
}

fn team_member_status_label(status: TeamMemberStatus) -> &'static str {
    match status {
        TeamMemberStatus::Idle => "Idle",
        TeamMemberStatus::Pending => "Pending",
        TeamMemberStatus::Running => "Running",
        TeamMemberStatus::WaitingApproval => "Waiting Approval",
        TeamMemberStatus::Completed => "Completed",
        TeamMemberStatus::Failed => "Failed",
        TeamMemberStatus::Cancelled => "Cancelled",
    }
}

fn team_assignment_status_label(status: TeamAssignmentStatus) -> &'static str {
    match status {
        TeamAssignmentStatus::Pending => "Pending",
        TeamAssignmentStatus::InProgress => "In Progress",
        TeamAssignmentStatus::Completed => "Completed",
        TeamAssignmentStatus::Failed => "Failed",
        TeamAssignmentStatus::Cancelled => "Cancelled",
    }
}

fn team_approval_status_label(status: TeamApprovalStatus) -> &'static str {
    match status {
        TeamApprovalStatus::Pending => "Pending",
        TeamApprovalStatus::Approved => "Approved",
        TeamApprovalStatus::Rejected => "Rejected",
    }
}

fn team_message_kind_label(kind: TeamMessageKind) -> &'static str {
    match kind {
        TeamMessageKind::Note => "Note",
        TeamMessageKind::ApprovalRequest => "Approval",
        TeamMessageKind::ApprovalResolution => "Resolution",
        TeamMessageKind::Assignment => "Assignment",
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

#[cfg(test)]
mod tests {
    use super::{bottom_anchor_lines, cell_color, footer_shortcuts, overlay_title, shell_status_text, short_id};
    use crate::state::{AppState, OverlayState, TeamOverlayTab, ThreadFocus};
    use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};
    use ratatui::style::Color;
    use ratatui::text::Line;

    #[test]
    fn status_text_prefers_streaming() {
        let mut state = AppState::empty();
        state.is_streaming = true;
        assert_eq!(shell_status_text(&state), "Streaming");
    }

    #[test]
    fn status_text_uses_run_focus() {
        let mut state = AppState::empty();
        state.thread.focus = ThreadFocus::Run {
            run_id: "run-1".to_string(),
        };
        assert_eq!(shell_status_text(&state), "Viewing Run");
    }

    #[test]
    fn footer_shortcuts_change_with_mode() {
        let mut state = AppState::empty();
        for ch in "/help".chars() {
            state.composer.insert_char(ch);
        }
        assert_eq!(
            footer_shortcuts(&state),
            vec![
                "Enter Run Command",
                "Esc Back",
                "/help",
                "/run open",
                "/team state",
            ]
        );
    }

    #[test]
    fn overlay_titles_are_user_facing() {
        assert_eq!(
            overlay_title(&OverlayState::RunPicker { selected: 0 }),
            "Open Run"
        );
        assert_eq!(overlay_title(&OverlayState::Help), "Keyboard Shortcuts");
    }

    #[test]
    fn overlay_shortcuts_override_composer_shortcuts() {
        let mut state = AppState::empty();
        state.overlay = Some(OverlayState::TeamView {
            tab: TeamOverlayTab::Members,
            scroll: 0,
        });
        assert_eq!(
            footer_shortcuts(&state),
            vec!["Esc Close", "↑↓ Scroll", "←→ Tabs"]
        );
    }

    #[test]
    fn short_id_truncates_visible_identifiers() {
        assert_eq!(short_id("1234567890abcdef"), "12345678");
        assert_eq!(short_id("short"), "short");
    }

    #[test]
    fn bottom_anchor_lines_keeps_tail_visible() {
        let lines = vec![
            Line::from("a"),
            Line::from("b"),
            Line::from("c"),
            Line::from("d"),
        ];
        let visible = bottom_anchor_lines(lines, 3, 0);
        let strings = visible
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert_eq!(strings, vec!["b", "c", "d"]);
    }

    #[test]
    fn bottom_anchor_lines_pads_top_when_short() {
        let visible = bottom_anchor_lines(vec![Line::from("hello")], 3, 0);
        let strings = visible
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert_eq!(strings, vec!["", "", "hello"]);
    }

    #[test]
    fn active_assistant_cell_uses_assistant_color() {
        let cell = TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "RestFlow".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "hello".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        };
        assert_eq!(cell_color(&cell), Color::Green);
    }

    #[test]
    fn startup_status_overrides_shell_status() {
        let mut state = AppState::empty();
        state.enter_startup(None, None);
        assert_eq!(shell_status_text(&state), "Daemon Offline");
        state.mark_starting_daemon();
        assert_eq!(shell_status_text(&state), "Starting Daemon");
    }
}
