use std::io::{Result as IoResult, Stdout, Write};

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{
    Attribute, Color as CrosstermColor, Colors, Print, SetAttribute, SetBackgroundColor, SetColors,
    SetForegroundColor,
};
use crossterm::terminal::{self, Clear, ClearType};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::render::render_shell_bottom_viewport;
use crate::scrollback::ScrollbackWriter;
use crate::slash_command::SLASH_COMMAND_SPECS;
use crate::state::AppState;
use crate::transcript::{TranscriptCell, TranscriptCellKind};

const CONTINUATION_PREFIX: &str = "  ";
const TOOL_SUMMARY_LIMIT: usize = 120;
const PROMPT_MIN_VISIBLE_ROWS: u16 = 1;
const PROMPT_MAX_VISIBLE_ROWS: u16 = 6;
const OVERLAY_MAX_ROWS: u16 = 10;

pub struct ShellRenderer {
    stdout: Stdout,
    scrollback: ScrollbackWriter,
    last_viewport: Option<ViewportSnapshot>,
    last_terminal_size: Option<(u16, u16)>,
    last_message_line_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ViewportSnapshot {
    top: u16,
    lines: Vec<Line<'static>>,
    cursor_x: u16,
    cursor_y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptSnapshot {
    lines: Vec<Line<'static>>,
    cursor_column: u16,
    cursor_row: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalViewport {
    size: (u16, u16),
    snapshot: ViewportSnapshot,
}

impl ShellRenderer {
    pub fn new() -> Self {
        Self {
            stdout: std::io::stdout(),
            scrollback: ScrollbackWriter::default(),
            last_viewport: None,
            last_terminal_size: None,
            last_message_line_count: None,
        }
    }

    pub fn clear_screen(&mut self) -> IoResult<()> {
        queue_clear_visible(&mut self.stdout)?;
        self.scrollback.reset();
        self.last_viewport = None;
        self.last_terminal_size = None;
        self.last_message_line_count = None;
        self.stdout.flush()
    }

    pub fn purge_screen(&mut self) -> IoResult<()> {
        queue_purge_visible_and_scrollback(&mut self.stdout)?;
        self.scrollback.reset();
        self.last_viewport = None;
        self.last_terminal_size = None;
        self.last_message_line_count = None;
        self.stdout.flush()
    }

    pub fn sync(&mut self, state: &mut AppState) -> IoResult<()> {
        let size = normalize_terminal_size(terminal::size().unwrap_or((80, 24)));
        self.preserve_scrolled_message_anchor(state, size);
        let terminal_viewport = TerminalViewport::build(state, size);
        let viewport = terminal_viewport.snapshot;
        let stable_cells = build_stable_history_cells(state);

        let mut force_full_redraw = false;
        if !self.scrollback.is_prefix_of(&stable_cells) {
            self.scrollback.reset();
            self.last_viewport = None;
            self.last_message_line_count = None;
            queue_purge_visible_and_scrollback(&mut self.stdout)?;
            force_full_redraw = true;
        }

        self.scrollback
            .sync_history(&stable_cells, size.0, render_history_append_lines);

        if force_full_redraw || self.needs_full_redraw(size, &viewport) {
            let clear_from = self
                .last_viewport
                .as_ref()
                .map(|previous| previous.top.min(viewport.top))
                .unwrap_or(viewport.top);
            self.clear_rows_from(clear_from, size.1, size.0)?;
            self.scrollback
                .insert_pending(&mut self.stdout, viewport.top, size.0)?;
            self.redraw_history_tail(viewport.top, size.0, &stable_cells)?;
            self.redraw_viewport_full(&viewport, size.0)?;
        } else {
            self.scrollback
                .insert_pending(&mut self.stdout, viewport.top, size.0)?;
            self.redraw_history_tail(viewport.top, size.0, &stable_cells)?;
            match self.last_viewport.clone() {
                Some(previous) if previous == viewport => {
                    self.restore_cursor(&viewport)?;
                }
                Some(previous) => {
                    self.redraw_viewport_diff(&previous, &viewport, size.0)?;
                }
                None => {
                    self.redraw_viewport_full(&viewport, size.0)?;
                }
            }
        }

        self.last_viewport = Some(viewport);
        self.last_terminal_size = Some(terminal_viewport.size);
        self.stdout.flush()
    }

    pub fn sync_viewport_only(&mut self, state: &mut AppState) -> IoResult<()> {
        let size = normalize_terminal_size(terminal::size().unwrap_or((80, 24)));
        self.preserve_scrolled_message_anchor(state, size);
        let terminal_viewport = TerminalViewport::build(state, size);
        let viewport = terminal_viewport.snapshot;
        let stable_cells = build_stable_history_cells(state);

        if !self.scrollback.is_prefix_of(&stable_cells) {
            return self.sync(state);
        }

        let can_update_viewport_only = self.last_terminal_size == Some(size)
            && self.last_viewport.as_ref().is_some_and(|previous| {
                previous.top == viewport.top && previous.lines.len() == viewport.lines.len()
            });
        if !can_update_viewport_only {
            return self.sync(state);
        }

        self.scrollback
            .sync_history(&stable_cells, size.0, render_history_append_lines);
        self.scrollback
            .insert_pending(&mut self.stdout, viewport.top, size.0)?;
        self.redraw_history_tail(viewport.top, size.0, &stable_cells)?;

        match self.last_viewport.clone() {
            Some(previous) if previous == viewport => {
                self.restore_cursor(&viewport)?;
            }
            Some(previous) => {
                self.redraw_viewport_diff(&previous, &viewport, size.0)?;
            }
            None => {
                self.redraw_viewport_full(&viewport, size.0)?;
            }
        }

        self.last_viewport = Some(viewport);
        self.last_terminal_size = Some(terminal_viewport.size);
        self.stdout.flush()
    }

    fn needs_full_redraw(&self, size: (u16, u16), viewport: &ViewportSnapshot) -> bool {
        self.last_terminal_size != Some(size)
            || self.last_viewport.as_ref().is_none_or(|previous| {
                previous.top != viewport.top || previous.lines.len() != viewport.lines.len()
            })
    }

    fn preserve_scrolled_message_anchor(&mut self, state: &mut AppState, size: (u16, u16)) {
        let message_line_count = message_layout_line_count(state, size);
        if let Some(previous_count) = self.last_message_line_count
            && state.message_scroll_from_bottom > 0
        {
            state.message_scroll_from_bottom = preserve_scrolled_offset(
                previous_count,
                message_line_count,
                state.message_scroll_from_bottom,
            );
        }
        self.last_message_line_count = Some(message_line_count);
    }

    fn clear_rows_from(&mut self, start_row: u16, height: u16, width: u16) -> IoResult<()> {
        for row in start_row..height {
            self.write_row(row, &Line::from(""), width)?;
        }
        Ok(())
    }

    fn redraw_history_tail(
        &mut self,
        viewport_top: u16,
        width: u16,
        stable_cells: &[TranscriptCell],
    ) -> IoResult<()> {
        if viewport_top == 0 || stable_cells.is_empty() {
            return Ok(());
        }
        let visible = visible_history_tail_lines(stable_cells, width, viewport_top as usize);
        for row in 0..viewport_top {
            let empty = Line::from("");
            let line = visible.get(row as usize).unwrap_or(&empty);
            self.write_row(row, line, width)?;
        }
        Ok(())
    }

    fn redraw_viewport_full(&mut self, viewport: &ViewportSnapshot, width: u16) -> IoResult<()> {
        for (offset, line) in viewport.lines.iter().enumerate() {
            self.write_row(viewport.top + offset as u16, line, width)?;
        }
        self.restore_cursor(viewport)
    }

    fn redraw_viewport_diff(
        &mut self,
        previous: &ViewportSnapshot,
        viewport: &ViewportSnapshot,
        width: u16,
    ) -> IoResult<()> {
        for row in changed_row_indices(&previous.lines, &viewport.lines) {
            self.write_row(viewport.top + row as u16, &viewport.lines[row], width)?;
        }
        self.restore_cursor(viewport)
    }

    fn restore_cursor(&mut self, viewport: &ViewportSnapshot) -> IoResult<()> {
        queue!(self.stdout, MoveTo(viewport.cursor_x, viewport.cursor_y))
    }

    fn write_row(&mut self, row: u16, line: &Line<'static>, width: u16) -> IoResult<()> {
        queue!(
            self.stdout,
            MoveTo(0, row),
            SetForegroundColor(CrosstermColor::Reset),
            SetBackgroundColor(CrosstermColor::Reset),
            SetAttribute(Attribute::Reset),
            Clear(ClearType::CurrentLine),
        )?;
        write_styled_line(&mut self.stdout, &truncate_line_to_width(line, width))
    }
}

impl TerminalViewport {
    fn build(state: &AppState, size: (u16, u16)) -> Self {
        let (width, height) = size;
        let prompt = build_prompt_snapshot(state, width, height);
        let prompt_height = prompt.lines.len() as u16 + 2;
        let available_above_prompt = height.saturating_sub(prompt_height);
        let overlay_capacity = available_above_prompt.min(OVERLAY_MAX_ROWS);
        let overlay_lines = build_overlay_lines(state, width, overlay_capacity).unwrap_or_default();
        let overlay_height = overlay_lines.len() as u16;
        let available_above_prompt = available_above_prompt.saturating_sub(overlay_height);
        let spacer_height = u16::from(available_above_prompt > 0);
        let message_height = available_above_prompt.saturating_sub(spacer_height);
        let message_lines = build_message_lines(state, width, message_height);
        let mut visible_message_lines = tail_lines(
            message_lines,
            message_height as usize,
            state.message_scroll_from_bottom,
        );
        if spacer_height > 0 && !visible_message_lines.is_empty() {
            visible_message_lines.push(Line::from(""));
        }
        visible_message_lines.extend(overlay_lines);
        let rendered = render_shell_bottom_viewport(
            width,
            visible_message_lines,
            &prompt.lines,
            prompt.cursor_column,
            prompt.cursor_row,
            &footer_status_line(state),
        );
        let top = height.saturating_sub(rendered.lines.len() as u16);

        Self {
            size,
            snapshot: ViewportSnapshot {
                top,
                lines: rendered.lines,
                cursor_x: rendered.cursor_x.min(width.saturating_sub(1)),
                cursor_y: (top + rendered.cursor_y).min(height.saturating_sub(1)),
            },
        }
    }
}

#[cfg(test)]
fn build_viewport_snapshot(state: &AppState, size: (u16, u16)) -> ViewportSnapshot {
    TerminalViewport::build(state, size).snapshot
}

fn build_prompt_snapshot(state: &AppState, width: u16, height: u16) -> PromptSnapshot {
    let content_width = prompt_content_width(width);
    let max_visible_rows = height
        .saturating_sub(2)
        .clamp(PROMPT_MIN_VISIBLE_ROWS, PROMPT_MAX_VISIBLE_ROWS);
    let show_placeholder = state.composer.is_blank();
    let visible_rows = if show_placeholder {
        1
    } else {
        state
            .composer
            .visible_row_count(content_width)
            .clamp(PROMPT_MIN_VISIBLE_ROWS, max_visible_rows)
    };

    let lines = if show_placeholder {
        vec![placeholder_line(content_width)]
    } else {
        state
            .composer
            .visible_lines(content_width, visible_rows)
            .into_iter()
            .map(Line::from)
            .collect()
    };

    let (cursor_column, cursor_row) = if show_placeholder {
        (0, 0)
    } else {
        state.composer.cursor_position(content_width, visible_rows)
    };

    PromptSnapshot {
        lines,
        cursor_column,
        cursor_row,
    }
}

fn message_layout_line_count(state: &AppState, size: (u16, u16)) -> usize {
    let (width, height) = size;
    let prompt = build_prompt_snapshot(state, width, height);
    let prompt_height = prompt.lines.len() as u16 + 2;
    let available_above_prompt = height.saturating_sub(prompt_height);
    let overlay_capacity = available_above_prompt.min(OVERLAY_MAX_ROWS);
    let overlay_height = build_overlay_lines(state, width, overlay_capacity)
        .map(|lines| lines.len() as u16)
        .unwrap_or_default();
    let available_above_prompt = available_above_prompt.saturating_sub(overlay_height);
    let spacer_height = u16::from(available_above_prompt > 0);
    let message_height = available_above_prompt.saturating_sub(spacer_height);
    build_message_lines(state, width, message_height).len()
}

fn build_stable_history_cells(state: &AppState) -> Vec<TranscriptCell> {
    let mut cells = Vec::with_capacity(
        state.conversation_cells.len() + state.pending_user_cells.len() + state.runtime_cells.len(),
    );
    let include_pending = state.active_cell.is_none() && state.active_turn_cells.is_empty();

    let mut pending = state.pending_user_cells.iter().peekable();
    let mut runtime = state.runtime_cells.iter().peekable();
    for (index, cell) in state.conversation_cells.iter().enumerate() {
        if include_pending {
            while let Some(entry) = pending.peek() {
                if entry.base_cell_index <= index {
                    cells.push(entry.cell.clone());
                    pending.next();
                } else {
                    break;
                }
            }
        }

        if runtime
            .peek()
            .is_some_and(|entry| entry.base_cell_index == index)
            && cell.kind == TranscriptCellKind::User
        {
            cells.push(cell.clone());
            while let Some(entry) = runtime.peek() {
                if entry.base_cell_index == index {
                    cells.push(entry.cell.clone());
                    runtime.next();
                } else {
                    break;
                }
            }
            continue;
        }

        while let Some(entry) = runtime.peek() {
            if entry.base_cell_index == index {
                cells.push(entry.cell.clone());
                runtime.next();
            } else {
                break;
            }
        }

        cells.push(cell.clone());
    }

    if include_pending {
        for entry in pending {
            cells.push(entry.cell.clone());
        }
    }
    for entry in runtime {
        cells.push(entry.cell.clone());
    }
    cells
}

fn visible_history_tail_lines(
    stable_cells: &[TranscriptCell],
    width: u16,
    height: usize,
) -> Vec<Line<'static>> {
    let history_lines = render_history_append_lines(stable_cells, width, false);
    bottom_anchor_lines(history_lines, height, 0)
}

fn build_message_lines(state: &AppState, width: u16, max_rows: u16) -> Vec<Line<'static>> {
    if max_rows == 0 {
        return Vec::new();
    }

    build_cell_lines(&build_live_message_cells(state), width)
}

fn build_live_message_cells(state: &AppState) -> Vec<TranscriptCell> {
    let mut cells = Vec::with_capacity(
        state.pending_user_cells.len()
            + state.active_turn_cells.len()
            + usize::from(state.active_cell.is_some()),
    );
    if state.active_cell.is_some() || !state.active_turn_cells.is_empty() {
        cells.extend(
            state
                .pending_user_cells
                .iter()
                .map(|entry| entry.cell.clone()),
        );
        cells.extend(state.active_turn_cells.iter().cloned());
        if let Some(active_cell) = state.active_cell.as_ref() {
            cells.push(active_cell.clone());
        }
    }
    cells
}

fn build_overlay_lines(state: &AppState, width: u16, max_rows: u16) -> Option<Vec<Line<'static>>> {
    if let Some(lines) = build_session_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_task_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_task_action_picker_lines(state, width) {
        return Some(lines);
    }

    if let Some(lines) = build_team_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_provider_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_model_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_daemon_picker_lines(state, width) {
        return Some(lines);
    }

    if let Some(lines) = build_command_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    None
}

#[cfg(test)]
fn build_transient_lines(state: &AppState, width: u16, max_rows: u16) -> Vec<Line<'static>> {
    if max_rows == 0 {
        return Vec::new();
    }

    if let Some(lines) = build_overlay_lines(state, width, max_rows) {
        return lines;
    }

    if state.active_cell.is_none() && state.active_turn_cells.is_empty() {
        return Vec::new();
    }

    let pending_cells = state
        .pending_user_cells
        .iter()
        .map(|entry| entry.cell.clone())
        .collect::<Vec<_>>();
    let pending_lines = build_cell_lines(&pending_cells, width);
    let mut live_cells = Vec::with_capacity(
        state.active_turn_cells.len() + usize::from(state.active_cell.is_some()),
    );
    live_cells.extend(state.active_turn_cells.iter().cloned());
    if let Some(active_cell) = state.active_cell.as_ref() {
        live_cells.push(active_cell.clone());
    }
    let active_lines = build_cell_lines(&live_cells, width);
    preserve_live_turn_lines(
        pending_lines,
        active_lines,
        max_rows as usize,
        stable_history_has_rendered_lines(state),
    )
}

fn build_session_picker_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::SessionPicker { selected }) = state.overlay.as_ref()
    else {
        return None;
    };

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Resume session", tool_title_style()),
        Span::styled(
            "  Up/Down select, Enter resume, d delete, Esc close",
            muted_style(),
        ),
    ]));
    if state.sessions.is_empty() {
        lines.push(styled_line("  No sessions to resume yet.", muted_style()));
        return Some(lines);
    }

    let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
    let rows_per_session = 3usize;
    let visible_sessions = (visible_capacity / rows_per_session).max(1);
    let selected_index = (*selected).min(state.sessions.len().saturating_sub(1));
    let start = selected_index
        .saturating_sub(visible_sessions / 2)
        .min(state.sessions.len().saturating_sub(visible_sessions));
    let end = (start + visible_sessions).min(state.sessions.len());

    for (index, session) in state.sessions[start..end].iter().enumerate() {
        let index = start + index;
        let is_selected = index == selected_index;
        let marker = if is_selected { "› " } else { "  " };
        let title_style = if is_selected {
            tool_title_style()
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let preview = session
            .last_message_preview
            .as_deref()
            .map(compact_session_preview)
            .filter(|preview| !preview.is_empty())
            .unwrap_or_else(|| "No messages yet".to_string());
        let mut title_spans = vec![
            Span::styled(
                marker,
                if is_selected {
                    tool_title_style()
                } else {
                    muted_style()
                },
            ),
            Span::styled(session.name.clone(), title_style),
            Span::styled(
                format!(" · {} messages", session.message_count),
                muted_style(),
            ),
        ];
        if is_selected && state.is_session_delete_pending(&session.id) {
            title_spans.push(Span::styled(" · press d again to delete", error_style()));
        }
        let title = Line::from(title_spans);
        lines.extend(wrap_styled_line(title, width));
        let detail = Line::from(vec![
            Span::styled("    Last: ", muted_style()),
            Span::styled(preview, muted_style()),
        ]);
        lines.extend(wrap_styled_line(detail, width));
        let id_line = Line::from(vec![
            Span::styled("    id: ", muted_style()),
            Span::styled(session.id.clone(), muted_style()),
        ]);
        lines.extend(wrap_styled_line(id_line, width));
    }

    if end < state.sessions.len() {
        lines.push(styled_line(
            format!("  ... {} more", state.sessions.len() - end),
            muted_style(),
        ));
    }

    lines.truncate(max_rows as usize);
    Some(lines)
}

fn build_task_picker_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::TaskPicker { selected }) = state.overlay.as_ref() else {
        return None;
    };

    let mut lines = vec![Line::from(vec![
        Span::styled("Tasks", tool_title_style()),
        Span::styled("  Up/Down select, Enter actions, Esc close", muted_style()),
    ])];
    if state.tasks.is_empty() {
        lines.push(styled_line("  No tasks available.", muted_style()));
        return Some(lines);
    }

    let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
    let rows_per_task = 2usize;
    let visible_tasks = (visible_capacity / rows_per_task).max(1);
    let selected_index = (*selected).min(state.tasks.len().saturating_sub(1));
    let start = selected_index
        .saturating_sub(visible_tasks / 2)
        .min(state.tasks.len().saturating_sub(visible_tasks));
    let end = (start + visible_tasks).min(state.tasks.len());

    for (index, task) in state.tasks[start..end].iter().enumerate() {
        let index = start + index;
        let is_selected = index == selected_index;
        let marker = if is_selected { "› " } else { "  " };
        let title_style = if is_selected {
            tool_title_style()
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let title = Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    tool_title_style()
                } else {
                    muted_style()
                },
            ),
            Span::styled(task.name.clone(), title_style),
            Span::styled(format!(" · {}", task.status), muted_style()),
        ]);
        lines.extend(wrap_styled_line(title, width));
        let id_line = Line::from(vec![
            Span::styled("    id: ", muted_style()),
            Span::styled(task.task_id.clone(), muted_style()),
        ]);
        lines.extend(wrap_styled_line(id_line, width));
    }

    if end < state.tasks.len() {
        lines.push(styled_line(
            format!("  ... {} more", state.tasks.len() - end),
            muted_style(),
        ));
    }
    lines.truncate(max_rows as usize);
    Some(lines)
}

fn build_task_action_picker_lines(state: &AppState, width: u16) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::TaskActionPicker { task_id, selected }) =
        state.overlay.as_ref()
    else {
        return None;
    };

    let actions = [
        ("pause", "Pause task scheduling"),
        ("resume", "Resume task scheduling"),
        ("stop", "Interrupt current/future execution"),
    ];
    let mut lines = vec![Line::from(vec![
        Span::styled("Task actions", tool_title_style()),
        Span::styled("  Up/Down select, Enter run, Esc close", muted_style()),
    ])];
    lines.push(styled_line(format!("  task: {task_id}"), muted_style()));
    for (index, (action, description)) in actions.iter().enumerate() {
        let selected = index == *selected;
        let line = Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                if selected {
                    tool_title_style()
                } else {
                    muted_style()
                },
            ),
            Span::styled(
                format!("/task {action}"),
                if selected {
                    tool_title_style()
                } else {
                    Style::default().add_modifier(Modifier::BOLD)
                },
            ),
            Span::styled("  ", muted_style()),
            Span::styled(*description, muted_style()),
        ]);
        lines.extend(wrap_styled_line(line, width));
    }
    Some(lines)
}

fn build_team_picker_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::TeamPicker { selected }) = state.overlay.as_ref() else {
        return None;
    };

    let mut lines = vec![Line::from(vec![
        Span::styled("Teams", tool_title_style()),
        Span::styled(
            "  Up/Down select, Enter open/start, Esc close",
            muted_style(),
        ),
    ])];
    if state.team_items.is_empty() {
        lines.push(styled_line("  No teams available.", muted_style()));
        return Some(lines);
    }

    let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
    let selected_index = (*selected).min(state.team_items.len().saturating_sub(1));
    let start = selected_index
        .saturating_sub(visible_capacity / 2)
        .min(state.team_items.len().saturating_sub(visible_capacity));
    let end = (start + visible_capacity).min(state.team_items.len());

    for (index, item) in state.team_items[start..end].iter().enumerate() {
        let index = start + index;
        let is_selected = index == selected_index;
        let marker = if is_selected { "› " } else { "  " };
        let title_style = if is_selected {
            tool_title_style()
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let line = match item {
            crate::state::TeamPickerItem::Current {
                team_run_id,
                status,
                members,
            } => Line::from(vec![
                Span::styled(
                    marker,
                    if is_selected {
                        tool_title_style()
                    } else {
                        muted_style()
                    },
                ),
                Span::styled(format!("Current team {team_run_id}"), title_style),
                Span::styled(format!(" · {status} · {members} members"), muted_style()),
            ]),
            crate::state::TeamPickerItem::Saved {
                name,
                member_groups,
                total_instances,
            } => Line::from(vec![
                Span::styled(
                    marker,
                    if is_selected {
                        tool_title_style()
                    } else {
                        muted_style()
                    },
                ),
                Span::styled(format!("Saved team {name}"), title_style),
                Span::styled(
                    format!(" · {member_groups} groups · {total_instances} members"),
                    muted_style(),
                ),
            ]),
        };
        lines.extend(wrap_styled_line(line, width));
    }
    if end < state.team_items.len() {
        lines.push(styled_line(
            format!("  ... {} more", state.team_items.len() - end),
            muted_style(),
        ));
    }
    lines.truncate(max_rows as usize);
    Some(lines)
}

fn build_model_picker_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::ModelPicker { provider, selected }) =
        state.overlay.as_ref()
    else {
        return None;
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{provider} models"), tool_title_style()),
        Span::styled("  Up/Down select, Enter switch, Esc close", muted_style()),
    ])];
    if state.model_items.is_empty() {
        lines.push(styled_line("  No available models.", muted_style()));
        return Some(lines);
    }

    let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
    let rows_per_model = 2usize;
    let visible_models = (visible_capacity / rows_per_model).max(1);
    let selected_index = (*selected).min(state.model_items.len().saturating_sub(1));
    let start = selected_index
        .saturating_sub(visible_models / 2)
        .min(state.model_items.len().saturating_sub(visible_models));
    let end = (start + visible_models).min(state.model_items.len());

    let mut previous_category = None;
    for (index, item) in state.model_items[start..end].iter().enumerate() {
        let index = start + index;
        let is_selected = index == selected_index;
        if previous_category != Some(item.category) {
            previous_category = Some(item.category);
            lines.push(styled_line(
                format!("  {}", model_category_label(item.category)),
                muted_style(),
            ));
        }
        let marker = if is_selected { "› " } else { "  " };
        let title_style = if is_selected {
            tool_title_style()
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let current = if item.is_current { " · current" } else { "" };
        let usage = if item.usage_count > 0 {
            format!(" · used {}", item.usage_count)
        } else {
            String::new()
        };
        let title = Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    tool_title_style()
                } else {
                    muted_style()
                },
            ),
            Span::styled(format!("{} · {}", item.provider, item.name), title_style),
            Span::styled(format!("{current}{usage}"), muted_style()),
        ]);
        lines.extend(wrap_styled_line(title, width));
        let model_line = Line::from(vec![
            Span::styled("    model: ", muted_style()),
            Span::styled(item.model.clone(), muted_style()),
        ]);
        lines.extend(wrap_styled_line(model_line, width));
    }

    if end < state.model_items.len() {
        lines.push(styled_line(
            format!("  ... {} more", state.model_items.len() - end),
            muted_style(),
        ));
    }
    lines.truncate(max_rows as usize);
    Some(lines)
}

fn build_provider_picker_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::ProviderPicker { selected }) = state.overlay.as_ref()
    else {
        return None;
    };

    let mut lines = vec![Line::from(vec![
        Span::styled("Providers", tool_title_style()),
        Span::styled("  Up/Down select, Enter models, Esc close", muted_style()),
    ])];
    if state.provider_items.is_empty() {
        lines.push(styled_line("  No available providers.", muted_style()));
        return Some(lines);
    }

    let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
    let selected_index = (*selected).min(state.provider_items.len().saturating_sub(1));
    let start = selected_index
        .saturating_sub(visible_capacity / 2)
        .min(state.provider_items.len().saturating_sub(visible_capacity));
    let end = (start + visible_capacity).min(state.provider_items.len());
    let mut previous_category = None;

    for (index, item) in state.provider_items[start..end].iter().enumerate() {
        let index = start + index;
        let is_selected = index == selected_index;
        if previous_category != Some(item.category) {
            previous_category = Some(item.category);
            lines.push(styled_line(
                format!("  {}", provider_category_label(item.category)),
                muted_style(),
            ));
        }
        let marker = if is_selected { "› " } else { "  " };
        let title_style = if is_selected {
            tool_title_style()
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let current = if item.is_current { " · current" } else { "" };
        let usage = if item.usage_count > 0 {
            format!(" · used {}", item.usage_count)
        } else {
            String::new()
        };
        let line = Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    tool_title_style()
                } else {
                    muted_style()
                },
            ),
            Span::styled(item.label.clone(), title_style),
            Span::styled(format!("{current}{usage}"), muted_style()),
        ]);
        lines.extend(wrap_styled_line(line, width));
    }

    if end < state.provider_items.len() {
        lines.push(styled_line(
            format!("  ... {} more", state.provider_items.len() - end),
            muted_style(),
        ));
    }
    lines.truncate(max_rows as usize);
    Some(lines)
}

fn provider_category_label(category: crate::state::ModelPickerCategory) -> &'static str {
    match category {
        crate::state::ModelPickerCategory::Recent => "Recently used providers",
        crate::state::ModelPickerCategory::Frequent => "Most used providers",
        crate::state::ModelPickerCategory::Available => "Available providers",
    }
}

fn model_category_label(category: crate::state::ModelPickerCategory) -> &'static str {
    match category {
        crate::state::ModelPickerCategory::Recent => "Recently used",
        crate::state::ModelPickerCategory::Frequent => "Most used",
        crate::state::ModelPickerCategory::Available => "Available with API key",
    }
}

fn build_daemon_picker_lines(state: &AppState, width: u16) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::DaemonPicker { selected }) = state.overlay.as_ref() else {
        return None;
    };

    let actions = [
        ("start", "Start the local daemon"),
        ("stop", "Stop the local daemon"),
    ];
    let mut lines = vec![Line::from(vec![
        Span::styled("Daemon", tool_title_style()),
        Span::styled("  Up/Down select, Enter run, Esc close", muted_style()),
    ])];
    for (index, (action, description)) in actions.iter().enumerate() {
        let selected = index == *selected;
        let line = Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                if selected {
                    tool_title_style()
                } else {
                    muted_style()
                },
            ),
            Span::styled(
                format!("/daemon {action}"),
                if selected {
                    tool_title_style()
                } else {
                    Style::default().add_modifier(Modifier::BOLD)
                },
            ),
            Span::styled("  ", muted_style()),
            Span::styled(*description, muted_style()),
        ]);
        lines.extend(wrap_styled_line(line, width));
    }
    Some(lines)
}

fn build_command_picker_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::CommandPicker { selected }) = state.overlay.as_ref()
    else {
        return None;
    };

    let mut lines = Vec::with_capacity(SLASH_COMMAND_SPECS.len() + 1);
    lines.push(Line::from(vec![
        Span::styled("Slash commands", tool_title_style()),
        Span::styled("  Enter to run, Esc to clear", muted_style()),
    ]));
    let command_width = SLASH_COMMAND_SPECS
        .iter()
        .map(|spec| command_display(spec.command, spec.args).chars().count())
        .max()
        .unwrap_or(0);

    for (index, spec) in SLASH_COMMAND_SPECS.iter().enumerate() {
        let selected = index == *selected;
        let display = command_display(spec.command, spec.args);
        let padding = " ".repeat(command_width.saturating_sub(display.chars().count()) + 2);
        let line = Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                if selected {
                    tool_title_style()
                } else {
                    muted_style()
                },
            ),
            Span::styled(
                display,
                if selected {
                    tool_title_style()
                } else {
                    Style::default().add_modifier(Modifier::BOLD)
                },
            ),
            Span::raw(padding),
            Span::styled(spec.description, muted_style()),
        ]);
        lines.extend(wrap_styled_line(line, width));
    }

    lines.truncate(max_rows as usize);
    Some(lines)
}

fn compact_session_preview(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn command_display(command: &str, args: &str) -> String {
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {args}")
    }
}

fn render_history_append_lines(
    cells: &[TranscriptCell],
    width: u16,
    prepend_separator: bool,
) -> Vec<Line<'static>> {
    let mut lines = build_cell_lines(cells, width);
    if prepend_separator && !lines.is_empty() {
        lines.insert(0, Line::from(""));
    }
    lines
}

fn build_cell_lines(cells: &[TranscriptCell], width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for cell in cells {
        if !should_render_cell(cell) {
            continue;
        }

        match cell.kind {
            TranscriptCellKind::Tool => {
                let title = format_title(cell);
                let summary = summarize_tool_body(cell.body.as_str());
                let line = if summary.is_empty() {
                    styled_line(title, tool_title_style())
                } else {
                    Line::from(vec![
                        Span::styled(title, tool_title_style()),
                        Span::raw(" "),
                        Span::styled(summary, tool_body_style()),
                    ])
                };
                lines.extend(wrap_styled_line(line, width));
            }
            _ => {
                lines.extend(wrap_display_line(
                    &format_title(cell),
                    width,
                    cell_title_style(cell),
                ));
                for line in normalize_body_lines(cell.body.as_str()) {
                    lines.extend(wrap_prefixed_line(
                        CONTINUATION_PREFIX,
                        &line,
                        width,
                        cell_body_style(cell),
                    ));
                }
            }
        }

        lines.push(Line::from(""));
    }

    if lines.last().is_some_and(line_is_empty) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
fn is_cell_prefix(previous: &[TranscriptCell], current: &[TranscriptCell]) -> bool {
    previous.len() <= current.len()
        && previous
            .iter()
            .zip(current.iter())
            .all(|(left, right)| left == right)
}

fn should_render_cell(cell: &TranscriptCell) -> bool {
    match cell.kind {
        TranscriptCellKind::Tool => !summarize_tool_body(cell.body.as_str()).is_empty(),
        TranscriptCellKind::Assistant => cell.is_active || !cell.body.trim().is_empty(),
        TranscriptCellKind::User | TranscriptCellKind::System | TranscriptCellKind::Notice => {
            !cell.body.trim().is_empty()
        }
    }
}

fn cell_title_style(cell: &TranscriptCell) -> Style {
    match cell.kind {
        TranscriptCellKind::User => Style::default()
            .fg(Color::LightBlue)
            .add_modifier(Modifier::BOLD),
        TranscriptCellKind::Assistant => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        TranscriptCellKind::System | TranscriptCellKind::Notice if cell.title == "Error" => {
            error_style()
        }
        TranscriptCellKind::System | TranscriptCellKind::Notice => muted_style(),
        TranscriptCellKind::Tool => tool_title_style(),
    }
}

fn cell_body_style(cell: &TranscriptCell) -> Style {
    match cell.kind {
        TranscriptCellKind::User => Style::default(),
        TranscriptCellKind::System | TranscriptCellKind::Notice if cell.title == "Error" => {
            error_style()
        }
        TranscriptCellKind::System | TranscriptCellKind::Notice => muted_style(),
        TranscriptCellKind::Tool => tool_body_style(),
        TranscriptCellKind::Assistant => Style::default(),
    }
}

fn tool_title_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn tool_body_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn muted_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn error_style() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

#[cfg(test)]
fn stable_history_has_rendered_lines(state: &AppState) -> bool {
    build_stable_history_cells(state)
        .iter()
        .any(should_render_cell)
}

fn queue_clear_visible(writer: &mut impl Write) -> IoResult<()> {
    queue!(writer, Clear(ClearType::All), MoveTo(0, 0))
}

fn queue_purge_visible_and_scrollback(writer: &mut impl Write) -> IoResult<()> {
    queue!(
        writer,
        Clear(ClearType::All),
        Clear(ClearType::Purge),
        MoveTo(0, 0)
    )
}

fn normalize_terminal_size((width, height): (u16, u16)) -> (u16, u16) {
    (width.max(3), height.max(3))
}

fn prompt_content_width(total_width: u16) -> u16 {
    total_width.saturating_sub(2).max(1)
}

fn format_title(cell: &TranscriptCell) -> String {
    match (&cell.subtitle, cell.is_active) {
        (Some(subtitle), true) => format!("{} · {}", cell.title, subtitle),
        (Some(subtitle), false) => format!("{} {}", cell.title, subtitle),
        (None, _) => cell.title.clone(),
    }
}

fn normalize_body_lines(body: &str) -> Vec<String> {
    if body.trim().is_empty() {
        return Vec::new();
    }

    let raw = body.lines().collect::<Vec<_>>();
    let first_non_blank = raw.iter().position(|line| !line.trim().is_empty());
    let last_non_blank = raw.iter().rposition(|line| !line.trim().is_empty());
    let (Some(start), Some(end)) = (first_non_blank, last_non_blank) else {
        return Vec::new();
    };

    let mut lines = Vec::new();
    let mut previous_blank = false;
    for line in &raw[start..=end] {
        if line.trim().is_empty() {
            if !previous_blank {
                lines.push(String::new());
                previous_blank = true;
            }
        } else {
            lines.push((*line).to_string());
            previous_blank = false;
        }
    }
    lines
}

fn summarize_tool_body(body: &str) -> String {
    let compact = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if compact.is_empty() {
        return String::new();
    }

    let mut summary = compact.chars().take(TOOL_SUMMARY_LIMIT).collect::<String>();
    if compact.chars().count() > TOOL_SUMMARY_LIMIT {
        summary.push('…');
    }
    summary
}

fn footer_status_line(state: &AppState) -> String {
    let Some(session) = state.current_session() else {
        if let Some(pending_session) = state.pending_session.as_ref() {
            return pending_session.model_label();
        }
        return state.status.clone();
    };

    let provider = session.provider.trim();
    let model = session.model.trim();
    match (provider, model.is_empty()) {
        (provider, false) if !provider.is_empty() => format!("{provider} · {model}"),
        (_, false) => model.to_string(),
        _ => state.status.clone(),
    }
}

fn placeholder_line(inner_width: u16) -> Line<'static> {
    styled_line(
        truncate_to_width("Type your message or use /help", inner_width),
        muted_style(),
    )
}

#[cfg(test)]
fn preserve_first_line_tail(lines: Vec<Line<'static>>, max_rows: usize) -> Vec<Line<'static>> {
    if max_rows == 0 || lines.len() <= max_rows {
        return lines;
    }
    if max_rows == 1 {
        return lines.into_iter().take(1).collect();
    }

    let mut visible = vec![lines[0].clone()];
    let body_start = lines.len().saturating_sub(max_rows - 1);
    visible.extend_from_slice(&lines[body_start.max(1)..]);
    visible
}

#[cfg(test)]
fn preserve_active_cell_separator(
    lines: Vec<Line<'static>>,
    max_rows: usize,
    prepend_separator: bool,
) -> Vec<Line<'static>> {
    if !prepend_separator || max_rows <= 1 || lines.is_empty() {
        return preserve_first_line_tail(lines, max_rows);
    }

    let mut visible = vec![Line::from("")];
    visible.extend(preserve_first_line_tail(lines, max_rows - 1));
    visible
}

#[cfg(test)]
fn preserve_live_turn_lines(
    pending_lines: Vec<Line<'static>>,
    active_lines: Vec<Line<'static>>,
    max_rows: usize,
    prepend_separator: bool,
) -> Vec<Line<'static>> {
    if max_rows == 0 {
        return Vec::new();
    }

    let mut visible = Vec::new();
    let mut remaining = max_rows;
    if prepend_separator && remaining > 1 {
        visible.push(Line::from(""));
        remaining -= 1;
    }

    if active_lines.len() >= remaining {
        visible.extend(preserve_first_line_tail(active_lines, remaining));
        return visible;
    }

    let pending_capacity = remaining.saturating_sub(active_lines.len());
    if pending_capacity > 0 {
        let start = pending_lines.len().saturating_sub(pending_capacity);
        visible.extend_from_slice(&pending_lines[start..]);
    }
    visible.extend(active_lines);
    visible
}

fn wrap_display_line(value: &str, width: u16, style: Style) -> Vec<Line<'static>> {
    wrap_styled_line(styled_line(value.to_string(), style), width)
}

fn wrap_prefixed_line(prefix: &str, value: &str, width: u16, style: Style) -> Vec<Line<'static>> {
    let prefix_width = display_width(prefix) as usize;
    let content_width = (width as usize).saturating_sub(prefix_width).max(1);
    wrap_styled_line(styled_line(value.to_string(), style), content_width as u16)
        .into_iter()
        .enumerate()
        .map(|(index, line)| prefix_styled_line(if index == 0 { prefix } else { "  " }, line))
        .collect()
}

fn wrap_styled_line(line: Line<'static>, width: u16) -> Vec<Line<'static>> {
    let width = width.max(1) as usize;
    let line_style = line.style;
    let line_alignment = line.alignment;
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut current_width = 0usize;

    for span in line.spans {
        let span_style = span.style;
        for ch in span.content.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if current_width > 0 && ch_width > 0 && current_width + ch_width > width {
                lines.push(line_from_spans(
                    std::mem::take(&mut current),
                    line_style,
                    line_alignment,
                ));
                current_width = 0;
            }
            push_char_span(&mut current, ch, span_style);
            current_width += ch_width;
        }
    }

    lines.push(line_from_spans(current, line_style, line_alignment));
    lines
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
    let scroll_from_bottom = clamp_history_scroll(total, height, scroll_from_bottom);
    let end = total.saturating_sub(scroll_from_bottom);
    let start = end.saturating_sub(height);
    let mut visible = lines[start..end].to_vec();
    if visible.len() < height {
        let mut padding = vec![Line::from(""); height - visible.len()];
        padding.append(&mut visible);
        return padding;
    }
    visible
}

fn tail_lines(
    lines: Vec<Line<'static>>,
    height: usize,
    scroll_from_bottom: usize,
) -> Vec<Line<'static>> {
    if height == 0 || lines.is_empty() {
        return Vec::new();
    }
    let total = lines.len();
    let scroll_from_bottom = clamp_history_scroll(total, height, scroll_from_bottom);
    let end = total.saturating_sub(scroll_from_bottom);
    let start = end.saturating_sub(height);
    lines[start..end].to_vec()
}

fn clamp_history_scroll(total_lines: usize, viewport_height: usize, requested: usize) -> usize {
    requested.min(total_lines.saturating_sub(viewport_height))
}

fn preserve_scrolled_offset(
    previous_line_count: usize,
    current_line_count: usize,
    current_scroll_from_bottom: usize,
) -> usize {
    if current_line_count > previous_line_count {
        current_scroll_from_bottom.saturating_add(current_line_count - previous_line_count)
    } else {
        current_scroll_from_bottom.saturating_sub(previous_line_count - current_line_count)
    }
}

fn changed_row_indices(previous: &[Line<'static>], current: &[Line<'static>]) -> Vec<usize> {
    let max_len = previous.len().max(current.len());
    let mut rows = Vec::new();
    for index in 0..max_len {
        if previous.get(index) != current.get(index) {
            rows.push(index);
        }
    }
    rows
}

#[cfg(test)]
fn protected_append_top(previous: Option<&ViewportSnapshot>, current: &ViewportSnapshot) -> u16 {
    previous
        .map(|previous| previous.top.min(current.top))
        .unwrap_or(current.top)
}

#[cfg(test)]
fn visible_history_fill_count(
    history_line_count: usize,
    viewport_top: u16,
    incoming_line_count: usize,
) -> usize {
    (viewport_top as usize)
        .saturating_sub(history_line_count)
        .min(incoming_line_count)
}

fn styled_line(value: impl Into<String>, style: Style) -> Line<'static> {
    Line::from(Span::styled(value.into(), style))
}

fn line_from_spans(
    spans: Vec<Span<'static>>,
    style: Style,
    alignment: Option<ratatui::layout::Alignment>,
) -> Line<'static> {
    let mut line = Line::from(spans);
    line.style = style;
    line.alignment = alignment;
    line
}

fn push_char_span(spans: &mut Vec<Span<'static>>, ch: char, style: Style) {
    if let Some(last) = spans.last_mut()
        && last.style == style
    {
        last.content.to_mut().push(ch);
        return;
    }
    spans.push(Span::styled(ch.to_string(), style));
}

fn prefix_styled_line(prefix: &str, line: Line<'static>) -> Line<'static> {
    let mut spans = Vec::with_capacity(line.spans.len() + 1);
    spans.push(Span::raw(prefix.to_string()));
    spans.extend(line.spans);
    line_from_spans(spans, line.style, line.alignment)
}

fn line_is_empty(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .all(|span| span.content.as_ref().is_empty())
}

#[cfg(test)]
fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

fn truncate_line_to_width(line: &Line<'static>, width: u16) -> Line<'static> {
    let width = width as usize;
    let mut spans = Vec::new();
    let mut current_width = 0usize;

    for span in &line.spans {
        for ch in span.content.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if current_width + ch_width > width {
                return line_from_spans(spans, line.style, line.alignment);
            }
            push_char_span(&mut spans, ch, span.style);
            current_width += ch_width;
        }
    }

    line_from_spans(spans, line.style, line.alignment)
}

fn write_styled_line(writer: &mut impl Write, line: &Line<'static>) -> IoResult<()> {
    for span in &line.spans {
        let style = if line.style == Style::default() {
            span.style
        } else {
            span.style.patch(line.style)
        };
        queue!(
            writer,
            SetAttribute(Attribute::Reset),
            SetColors(Colors::new(
                style
                    .fg
                    .map(std::convert::Into::into)
                    .unwrap_or(CrosstermColor::Reset),
                style
                    .bg
                    .map(std::convert::Into::into)
                    .unwrap_or(CrosstermColor::Reset),
            ))
        )?;
        queue_modifiers(writer, style.add_modifier - style.sub_modifier)?;
        queue!(writer, Print(span.content.clone()))?;
    }
    queue!(
        writer,
        SetForegroundColor(CrosstermColor::Reset),
        SetBackgroundColor(CrosstermColor::Reset),
        SetAttribute(Attribute::Reset),
    )
}

fn queue_modifiers(writer: &mut impl Write, modifiers: Modifier) -> IoResult<()> {
    if modifiers.contains(Modifier::BOLD) {
        queue!(writer, SetAttribute(Attribute::Bold))?;
    }
    if modifiers.contains(Modifier::DIM) {
        queue!(writer, SetAttribute(Attribute::Dim))?;
    }
    if modifiers.contains(Modifier::ITALIC) {
        queue!(writer, SetAttribute(Attribute::Italic))?;
    }
    if modifiers.contains(Modifier::UNDERLINED) {
        queue!(writer, SetAttribute(Attribute::Underlined))?;
    }
    if modifiers.contains(Modifier::REVERSED) {
        queue!(writer, SetAttribute(Attribute::Reverse))?;
    }
    if modifiers.contains(Modifier::CROSSED_OUT) {
        queue!(writer, SetAttribute(Attribute::CrossedOut))?;
    }
    if modifiers.contains(Modifier::SLOW_BLINK) {
        queue!(writer, SetAttribute(Attribute::SlowBlink))?;
    }
    if modifiers.contains(Modifier::RAPID_BLINK) {
        queue!(writer, SetAttribute(Attribute::RapidBlink))?;
    }
    Ok(())
}

fn truncate_to_width(value: &str, width: u16) -> String {
    let width = width as usize;
    let mut out = String::new();
    let mut current = 0usize;
    for ch in value.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current + ch_width > width {
            break;
        }
        out.push(ch);
        current += ch_width;
    }
    out
}

fn display_width(value: &str) -> u16 {
    unicode_width::UnicodeWidthStr::width(value) as u16
}

#[cfg(test)]
mod tests {
    use super::{
        ViewportSnapshot, bottom_anchor_lines, build_stable_history_cells, build_transient_lines,
        build_viewport_snapshot, cell_title_style, changed_row_indices, clamp_history_scroll,
        compact_session_preview, footer_status_line, format_title, is_cell_prefix, line_text,
        normalize_body_lines, preserve_active_cell_separator, preserve_first_line_tail,
        preserve_scrolled_offset, protected_append_top, queue_clear_visible,
        queue_purge_visible_and_scrollback, render_history_append_lines, summarize_tool_body,
        visible_history_fill_count, visible_history_tail_lines, write_styled_line,
    };
    use crossterm::queue;
    use crossterm::style::{Attribute, Color as CrosstermColor, Colors, SetAttribute, SetColors};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Line;
    use ratatui::text::Span;

    use crate::render::render_shell_bottom_viewport;
    use crate::slash_command::SLASH_COMMAND_SPECS;
    use crate::state::{
        AnchoredRuntimeCell, AppState, ModelPickerCategory, ModelPickerItem, PendingSessionState,
        PendingUserCell, ProviderPickerItem, TaskPickerItem, TeamPickerItem,
    };
    use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};
    use restflow_core::models::ChatSessionSummary;

    fn line_texts(lines: &[Line<'static>]) -> Vec<String> {
        lines.iter().map(line_text).collect()
    }

    #[test]
    fn tool_summary_compacts_and_truncates_multiline_content() {
        let summary = summarize_tool_body(" \n {\"ok\": true}\n\nsecond line");
        assert_eq!(summary, "{\"ok\": true} second line");
    }

    #[test]
    fn footer_uses_pending_session_model_when_no_persisted_session() {
        let mut state = AppState::empty();
        state.set_pending_session(Some(PendingSessionState::new(
            "agent-1".to_string(),
            "Agent".to_string(),
            "gpt-5.4".to_string(),
        )));

        assert_eq!(footer_status_line(&state), "codex · gpt-5.4");
    }

    #[test]
    fn active_titles_include_typing_subtitle_inline() {
        let cell = TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Default Assistant".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "hello".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        };
        assert_eq!(format_title(&cell), "Default Assistant · typing…");
    }

    #[test]
    fn transcript_lines_style_user_assistant_and_tool_titles() {
        let lines = super::build_cell_lines(
            &[
                TranscriptCell {
                    kind: TranscriptCellKind::User,
                    title: "You".to_string(),
                    subtitle: None,
                    body: "hello".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
                TranscriptCell {
                    kind: TranscriptCellKind::Assistant,
                    title: "Agent".to_string(),
                    subtitle: None,
                    body: "hi".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
                TranscriptCell {
                    kind: TranscriptCellKind::Tool,
                    title: "Tool · bash".to_string(),
                    subtitle: None,
                    body: "ok".to_string(),
                    group: MessageGroup::ToolActivity,
                    is_active: false,
                },
            ],
            80,
        );

        assert_eq!(lines[0].spans[0].style.fg, Some(Color::LightBlue));
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[1].spans[1].style.fg, None);
        assert!(
            !lines[1].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[3].spans[0].style.fg, Some(Color::Yellow));
        assert!(
            lines[3].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[4].spans[1].style.fg, None);
        assert!(
            !lines[4].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[6].spans[0].style.fg, Some(Color::Cyan));
        assert!(
            lines[6].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn active_assistant_title_uses_active_style() {
        let cell = TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "hello".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        };

        let style = cell_title_style(&cell);

        assert_eq!(style.fg, Some(Color::Yellow));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn write_styled_line_emits_ansi_style_sequences() {
        let line = Line::from(Span::styled(
            "Styled",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        let mut output = Vec::new();

        write_styled_line(&mut output, &line).expect("styled line should render");

        let mut expected_color = Vec::new();
        queue!(
            expected_color,
            SetColors(Colors::new(CrosstermColor::Cyan, CrosstermColor::Reset))
        )
        .expect("expected color sequence should render");
        let mut expected_bold = Vec::new();
        queue!(expected_bold, SetAttribute(Attribute::Bold))
            .expect("expected bold sequence should render");
        assert!(
            output
                .windows(expected_color.len())
                .any(|window| window == expected_color)
        );
        assert!(
            output
                .windows(expected_bold.len())
                .any(|window| window == expected_bold)
        );
        assert!(String::from_utf8_lossy(&output).contains("Styled"));
    }

    #[test]
    fn bottom_viewport_renderer_keeps_footer_and_borders() {
        let prompt_lines = vec![Line::from("hello")];
        let rendered =
            render_shell_bottom_viewport(20, Vec::new(), &prompt_lines, 0, 0, "openai · gpt-5");

        assert_eq!(rendered.lines.len(), 3);
        assert!(line_text(&rendered.lines[0]).starts_with('┌'));
        assert!(line_text(&rendered.lines[1]).starts_with('│'));
        assert!(line_text(&rendered.lines[2]).starts_with('└'));
        assert!(line_text(&rendered.lines[2]).contains("openai"));
    }

    #[test]
    fn bottom_viewport_renderer_preserves_cjk_without_spacer_cells() {
        let prompt_lines = vec![Line::from("帮我打开浏览器")];
        let rendered =
            render_shell_bottom_viewport(32, Vec::new(), &prompt_lines, 0, 0, "openai · gpt-5");

        assert!(line_text(&rendered.lines[1]).contains("帮我打开浏览器"));
        assert!(!line_text(&rendered.lines[1]).contains("帮 我"));
    }

    #[test]
    fn clear_visible_homes_cursor_before_redraw() {
        let mut output = Vec::new();
        queue_clear_visible(&mut output).expect("clear sequence should render");
        let ansi = String::from_utf8(output).expect("clear sequence should be utf8");
        assert!(ansi.contains("\u{1b}[2J"));
        assert!(ansi.contains("\u{1b}[1;1H") || ansi.contains("\u{1b}[H"));
    }

    #[test]
    fn purge_visible_and_scrollback_emits_scrollback_clear() {
        let mut output = Vec::new();
        queue_purge_visible_and_scrollback(&mut output).expect("purge sequence should render");
        let ansi = String::from_utf8(output).expect("purge sequence should be utf8");
        assert!(ansi.contains("\u{1b}[2J"));
        assert!(ansi.contains("\u{1b}[3J"));
        assert!(ansi.contains("\u{1b}[1;1H") || ansi.contains("\u{1b}[H"));
    }

    #[test]
    fn stable_history_tail_keeps_user_message_visible() {
        let cells = vec![
            TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "132".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
            TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Agent".to_string(),
                subtitle: None,
                body: "assistant reply".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
        ];

        let rendered = line_texts(&visible_history_tail_lines(&cells, 80, 8));

        assert!(rendered.iter().any(|line| line.contains("You")));
        assert!(rendered.iter().any(|line| line.contains("132")));
        assert!(rendered.iter().any(|line| line.contains("assistant reply")));
    }

    #[test]
    fn viewport_stays_anchored_to_bottom() {
        let state = AppState::empty();
        let viewport = build_viewport_snapshot(&state, (40, 10));
        assert_eq!(viewport.top, 7);
        assert_eq!(viewport.cursor_y, 8);
        assert_eq!(viewport.lines.len(), 3);
    }

    #[test]
    fn latest_message_stays_above_composer_rows() {
        let mut state = AppState::empty();
        state.active_cell = Some(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "first\nlatest visible message".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        });

        let viewport = build_viewport_snapshot(&state, (60, 10));
        let rendered = line_texts(&viewport.lines);
        let composer_top = rendered
            .iter()
            .position(|line| line.starts_with('┌'))
            .expect("composer top border");
        let latest_row = rendered
            .iter()
            .position(|line| line.contains("latest visible message"))
            .expect("latest message row");

        assert!(latest_row < composer_top);
        assert_eq!(rendered[composer_top - 1], "");
        assert!(
            !rendered[composer_top..]
                .iter()
                .any(|line| line.contains("latest visible message"))
        );
    }

    #[test]
    fn overlay_renders_between_message_and_composer() {
        let mut state = AppState::empty();
        state.active_cell = Some(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "live still visible".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        });
        state.open_command_picker();

        let viewport = build_viewport_snapshot(&state, (80, 16));
        let rendered = line_texts(&viewport.lines);
        let live_row = rendered
            .iter()
            .position(|line| line.contains("live still visible"))
            .expect("live row");
        let overlay_row = rendered
            .iter()
            .position(|line| line.contains("Slash commands"))
            .expect("overlay row");
        let composer_top = rendered
            .iter()
            .position(|line| line.starts_with('┌'))
            .expect("composer top border");

        assert!(live_row < overlay_row);
        assert!(overlay_row < composer_top);
    }

    #[test]
    fn protected_append_top_uses_current_top_without_previous_viewport() {
        let current = ViewportSnapshot {
            top: 12,
            lines: vec![Line::from(""); 3],
            cursor_x: 0,
            cursor_y: 13,
        };

        assert_eq!(protected_append_top(None, &current), 12);
    }

    #[test]
    fn protected_append_top_preserves_previous_streaming_viewport() {
        let previous = ViewportSnapshot {
            top: 8,
            lines: vec![Line::from(""); 8],
            cursor_x: 0,
            cursor_y: 15,
        };
        let current = ViewportSnapshot {
            top: 13,
            lines: vec![Line::from(""); 3],
            cursor_x: 0,
            cursor_y: 14,
        };

        assert_eq!(protected_append_top(Some(&previous), &current), 8);
    }

    #[test]
    fn protected_append_top_preserves_current_expanded_viewport() {
        let previous = ViewportSnapshot {
            top: 13,
            lines: vec![Line::from(""); 3],
            cursor_x: 0,
            cursor_y: 14,
        };
        let current = ViewportSnapshot {
            top: 8,
            lines: vec![Line::from(""); 8],
            cursor_x: 0,
            cursor_y: 15,
        };

        assert_eq!(protected_append_top(Some(&previous), &current), 8);
    }

    #[test]
    fn visible_history_fill_count_fills_padding_before_scrolling() {
        assert_eq!(visible_history_fill_count(0, 10, 4), 4);
        assert_eq!(visible_history_fill_count(7, 10, 5), 3);
        assert_eq!(visible_history_fill_count(10, 10, 5), 0);
        assert_eq!(visible_history_fill_count(12, 10, 5), 0);
    }

    #[test]
    fn stable_history_inserts_pending_user_in_order() {
        let mut state = AppState::empty();
        state.conversation_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: None,
            body: "hi".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        });
        state.pending_user_cells.push(PendingUserCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "hello".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
        });

        let cells = build_stable_history_cells(&state);
        assert_eq!(cells[0].kind, TranscriptCellKind::User);
        assert_eq!(cells[1].kind, TranscriptCellKind::Assistant);
    }

    #[test]
    fn stable_history_includes_runtime_tool_and_notice_cells() {
        let mut state = AppState::empty();
        state.runtime_cells.push(AnchoredRuntimeCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::Tool,
                title: "Tool · switch_model".to_string(),
                subtitle: None,
                body: "{\"ok\":true}".to_string(),
                group: MessageGroup::ToolActivity,
                is_active: false,
            },
        });
        state.runtime_cells.push(AnchoredRuntimeCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::Notice,
                title: "Info".to_string(),
                subtitle: None,
                body: "Listed sessions".to_string(),
                group: MessageGroup::RuntimeNotice,
                is_active: false,
            },
        });

        let cells = build_stable_history_cells(&state);

        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].kind, TranscriptCellKind::Tool);
        assert_eq!(cells[1].kind, TranscriptCellKind::Notice);
    }

    #[test]
    fn stable_history_keeps_runtime_cells_between_user_and_final_assistant() {
        let mut state = AppState::empty();
        state.pending_user_cells.push(PendingUserCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "run a tool".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
        });
        state.runtime_cells.push(AnchoredRuntimeCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::Tool,
                title: "Tool · bash".to_string(),
                subtitle: None,
                body: "ok".to_string(),
                group: MessageGroup::ToolActivity,
                is_active: false,
            },
        });
        state.conversation_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: None,
            body: "done".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        });

        let cells = build_stable_history_cells(&state);

        assert_eq!(
            cells.iter().map(|cell| cell.kind).collect::<Vec<_>>(),
            vec![
                TranscriptCellKind::User,
                TranscriptCellKind::Tool,
                TranscriptCellKind::Assistant,
            ]
        );
    }

    #[test]
    fn stable_history_keeps_runtime_cells_after_persisted_user() {
        let mut state = AppState::empty();
        state.conversation_cells.push(TranscriptCell {
            kind: TranscriptCellKind::User,
            title: "You".to_string(),
            subtitle: None,
            body: "run a tool".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        });
        state.conversation_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: None,
            body: "done".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        });
        state.runtime_cells.push(AnchoredRuntimeCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::Tool,
                title: "Tool · bash".to_string(),
                subtitle: None,
                body: "ok".to_string(),
                group: MessageGroup::ToolActivity,
                is_active: false,
            },
        });

        let cells = build_stable_history_cells(&state);

        assert_eq!(
            cells.iter().map(|cell| cell.kind).collect::<Vec<_>>(),
            vec![
                TranscriptCellKind::User,
                TranscriptCellKind::Tool,
                TranscriptCellKind::Assistant,
            ]
        );
    }

    #[test]
    fn transient_view_only_contains_active_assistant() {
        let mut state = AppState::empty();
        state.runtime_cells.push(AnchoredRuntimeCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::Notice,
                title: "Info".to_string(),
                subtitle: None,
                body: "This should be committed history".to_string(),
                group: MessageGroup::RuntimeNotice,
                is_active: false,
            },
        });
        state.active_cell = Some(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "streaming".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        });

        let lines = build_transient_lines(&state, 80, 8);

        let rendered = line_texts(&lines);
        assert!(rendered.iter().any(|line| line.contains("typing")));
        assert!(
            !rendered
                .iter()
                .any(|line| line.contains("committed history"))
        );
    }

    #[test]
    fn slash_command_dropdown_lists_all_commands_when_composer_starts_with_slash() {
        let mut state = AppState::empty();
        state.composer.insert_char('/');
        state.open_command_picker();

        let lines = build_transient_lines(&state, 100, 32);
        let rendered = line_texts(&lines);

        assert_eq!(rendered.len(), SLASH_COMMAND_SPECS.len() + 1);
        assert!(rendered[0].contains("Slash commands"));
        assert!(rendered.iter().any(|line| line.contains("/daemon")));
        assert!(!rendered.iter().any(|line| line.contains("/start")));
        assert!(!rendered.iter().any(|line| line.contains("/stop")));
        assert!(rendered.iter().any(|line| line.contains("/resume")));
        assert!(rendered.iter().any(|line| line.contains("/task")));
        assert!(rendered.iter().any(|line| line.contains("/team")));
        assert!(
            !rendered
                .iter()
                .any(|line| line.contains("/session open <session_id>"))
        );
        assert!(!rendered.iter().any(|line| line.contains("/runs")));
        assert!(
            !rendered
                .iter()
                .any(|line| line.contains("/run open <run_id>"))
        );
        assert!(
            !rendered
                .iter()
                .any(|line| line.contains("/reject <approval_id> [reason]"))
        );
    }

    #[test]
    fn slash_command_dropdown_uses_command_title_style() {
        let mut state = AppState::empty();
        state.composer.insert_char('/');
        state.open_command_picker();

        let lines = build_transient_lines(&state, 100, 32);

        assert_eq!(lines[1].spans[0].style.fg, Some(Color::Cyan));
        assert!(lines[1].spans[0].content.contains("/daemon"));
        assert!(
            lines[1].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn slash_command_dropdown_is_not_shown_for_plain_input() {
        let mut state = AppState::empty();
        state.composer.insert_char('h');
        state.composer.insert_char('i');

        let lines = build_transient_lines(&state, 100, 32);

        assert!(lines.is_empty());
    }

    #[test]
    fn resume_picker_lists_sessions_with_last_message_preview() {
        let mut state = AppState::empty();
        state.sessions = vec![
            ChatSessionSummary {
                id: "session-1".to_string(),
                name: "First chat".to_string(),
                agent_id: "agent-1".to_string(),
                provider: "provider".to_string(),
                model: "model".to_string(),
                skill_id: None,
                message_count: 2,
                updated_at: 1,
                last_message_preview: Some("hello\nworld".to_string()),
                source_channel: None,
                source_conversation_id: None,
                archived_at: None,
            },
            ChatSessionSummary {
                id: "session-2".to_string(),
                name: "Second chat".to_string(),
                agent_id: "agent-1".to_string(),
                provider: "provider".to_string(),
                model: "model".to_string(),
                skill_id: None,
                message_count: 0,
                updated_at: 2,
                last_message_preview: None,
                source_channel: None,
                source_conversation_id: None,
                archived_at: None,
            },
        ];
        state.open_session_picker();

        let lines = build_transient_lines(&state, 120, 16);
        let rendered = line_texts(&lines);

        assert!(rendered[0].contains("Resume session"));
        assert!(rendered.iter().any(|line| line.contains("First chat")));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("Last: hello world"))
        );
        assert!(rendered.iter().any(|line| line.contains("id: session-1")));
        assert!(rendered.iter().any(|line| line.contains("No messages yet")));
    }

    #[test]
    fn task_picker_lists_tasks_and_actions() {
        let mut state = AppState::empty();
        state.tasks = vec![TaskPickerItem {
            task_id: "task-1".to_string(),
            name: "Daily digest".to_string(),
            status: "Active".to_string(),
            next_run_at: None,
        }];
        state.open_task_picker();

        let lines = build_transient_lines(&state, 80, 8);
        let text = line_texts(&lines).join("\n");
        assert!(text.contains("Tasks"));
        assert!(text.contains("Daily digest"));
        assert!(text.contains("task-1"));

        state.open_task_action_picker("task-1");
        let lines = build_transient_lines(&state, 80, 8);
        let text = line_texts(&lines).join("\n");
        assert!(text.contains("Task actions"));
        assert!(text.contains("/task pause"));
        assert!(text.contains("/task resume"));
        assert!(text.contains("/task stop"));
    }

    #[test]
    fn team_picker_lists_current_and_saved_teams() {
        let mut state = AppState::empty();
        state.team_items = vec![
            TeamPickerItem::Current {
                team_run_id: "team-run-1".to_string(),
                status: "Running".to_string(),
                members: 2,
            },
            TeamPickerItem::Saved {
                name: "reviewers".to_string(),
                member_groups: 1,
                total_instances: 3,
            },
        ];
        state.open_team_picker();

        let lines = build_transient_lines(&state, 80, 8);
        let text = line_texts(&lines).join("\n");
        assert!(text.contains("Teams"));
        assert!(text.contains("Current team team-run-1"));
        assert!(text.contains("Saved team reviewers"));
    }

    #[test]
    fn provider_picker_lists_grouped_providers() {
        let mut state = AppState::empty();
        state.provider_items = vec![
            ProviderPickerItem {
                provider: "codex".to_string(),
                label: "codex".to_string(),
                category: ModelPickerCategory::Recent,
                usage_count: 2,
                last_used_at: Some(10),
                is_current: true,
            },
            ProviderPickerItem {
                provider: "openai".to_string(),
                label: "openai".to_string(),
                category: ModelPickerCategory::Available,
                usage_count: 0,
                last_used_at: None,
                is_current: false,
            },
        ];
        state.open_provider_picker();

        let lines = build_transient_lines(&state, 80, 8);
        let text = line_texts(&lines).join("\n");
        assert!(text.contains("Providers"));
        assert!(text.contains("Recently used providers"));
        assert!(text.contains("codex"));
        assert!(text.contains("current"));
    }

    #[test]
    fn model_picker_lists_provider_models() {
        let mut state = AppState::empty();
        state.model_items = vec![
            ModelPickerItem {
                provider: "codex".to_string(),
                model: "gpt-5.4".to_string(),
                name: "GPT-5.4".to_string(),
                category: ModelPickerCategory::Recent,
                usage_count: 2,
                last_used_at: Some(10),
                is_current: true,
            },
            ModelPickerItem {
                provider: "openai".to_string(),
                model: "gpt-5".to_string(),
                name: "GPT-5".to_string(),
                category: ModelPickerCategory::Available,
                usage_count: 0,
                last_used_at: None,
                is_current: false,
            },
        ];
        state.open_model_picker("codex");

        let lines = build_transient_lines(&state, 80, 8);
        let text = line_texts(&lines).join("\n");
        assert!(text.contains("codex models"));
        assert!(text.contains("Recently used"));
        assert!(text.contains("codex · GPT-5.4"));
        assert!(text.contains("current"));
        assert!(text.contains("model: gpt-5.4"));
    }

    #[test]
    fn resume_picker_scrolls_to_selected_session() {
        let mut state = AppState::empty();
        state.sessions = (0..8)
            .map(|index| ChatSessionSummary {
                id: format!("session-{index}"),
                name: format!("Session {index}"),
                agent_id: "agent-1".to_string(),
                provider: "provider".to_string(),
                model: "model".to_string(),
                skill_id: None,
                message_count: index,
                updated_at: index as i64,
                last_message_preview: Some(format!("preview {index}")),
                source_channel: None,
                source_conversation_id: None,
                archived_at: None,
            })
            .collect();
        state.open_session_picker();
        for _ in 0..6 {
            state.move_overlay_selection(1);
        }

        let lines = build_transient_lines(&state, 120, 7);
        let rendered = line_texts(&lines);

        assert!(rendered.iter().any(|line| line.contains("› Session 6")));
        assert!(!rendered.iter().any(|line| line.contains("Session 0")));
        assert!(rendered.iter().any(|line| line.contains("id: session-6")));
    }

    #[test]
    fn resume_picker_scrolls_before_selected_reaches_bottom() {
        let mut state = AppState::empty();
        state.sessions = (0..8)
            .map(|index| ChatSessionSummary {
                id: format!("session-{index}"),
                name: format!("Session {index}"),
                agent_id: "agent-1".to_string(),
                provider: "provider".to_string(),
                model: "model".to_string(),
                skill_id: None,
                message_count: index,
                updated_at: index as i64,
                last_message_preview: Some(format!("preview {index}")),
                source_channel: None,
                source_conversation_id: None,
                archived_at: None,
            })
            .collect();
        state.open_session_picker();
        for _ in 0..3 {
            state.move_overlay_selection(1);
        }

        let lines = build_transient_lines(&state, 120, 16);
        let rendered = line_texts(&lines);

        assert!(rendered.iter().any(|line| line.contains("› Session 3")));
        assert!(!rendered.iter().any(|line| line.contains("Session 0")));
        assert!(rendered.iter().any(|line| line.contains("Session 5")));
    }

    #[test]
    fn daemon_picker_lists_start_and_stop_actions() {
        let mut state = AppState::empty();
        state.open_daemon_picker();

        let lines = build_transient_lines(&state, 100, 8);
        let rendered = line_texts(&lines);

        assert!(rendered[0].contains("Daemon"));
        assert!(rendered.iter().any(|line| line.contains("/daemon start")));
        assert!(rendered.iter().any(|line| line.contains("/daemon stop")));
        assert!(rendered[1].contains('›'));
    }

    #[test]
    fn compact_session_preview_collapses_whitespace() {
        assert_eq!(compact_session_preview("hello\n  world"), "hello world");
    }

    #[test]
    fn transient_view_separates_active_assistant_from_prior_history() {
        let mut state = AppState::empty();
        state.pending_user_cells.push(PendingUserCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "hello".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
        });
        state.active_cell = Some(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "streaming".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        });

        let lines = build_transient_lines(&state, 80, 8);
        let rendered = line_texts(&lines);

        assert!(rendered.iter().any(|line| line.contains("You")));
        assert!(rendered.iter().any(|line| line.contains("hello")));
        assert!(rendered.iter().any(|line| line.contains("Agent")));
        assert!(rendered.iter().any(|line| line.contains("typing")));
    }

    #[test]
    fn transient_view_prioritizes_title_when_separator_would_exhaust_space() {
        let mut state = AppState::empty();
        state.pending_user_cells.push(PendingUserCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "hello".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
        });
        state.active_cell = Some(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "streaming".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        });

        let lines = build_transient_lines(&state, 80, 1);
        let rendered = line_texts(&lines);

        assert_eq!(lines.len(), 1);
        assert!(rendered[0].contains("Agent"));
        assert!(rendered[0].contains("typing"));
    }

    #[test]
    fn transient_view_preserves_active_assistant_title_when_tail_clipped() {
        let mut state = AppState::empty();
        state.active_cell = Some(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "line one\nline two\nline three\nline four".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        });

        let lines = build_transient_lines(&state, 80, 3);
        let rendered = line_texts(&lines);

        assert_eq!(lines.len(), 3);
        assert!(rendered[0].contains("Agent"));
        assert!(rendered[0].contains("typing"));
        assert!(!rendered.iter().any(|line| line.contains("line one")));
        assert!(rendered.iter().any(|line| line.contains("line four")));
    }

    #[test]
    fn live_turn_renders_tool_as_a_separate_cell() {
        let mut state = AppState::empty();
        state.active_turn_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: None,
            body: "Checking first".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        });
        state.active_turn_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Tool,
            title: "Tool · web_search".to_string(),
            subtitle: Some("#call-1".to_string()),
            body: "Input: {\"query\":\"离骚全文 屈原\"}\nOutput: {\"ok\":true}".to_string(),
            group: MessageGroup::ToolActivity,
            is_active: false,
        });
        state.active_cell = Some(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: "Final answer".to_string(),
            group: MessageGroup::Conversation,
            is_active: true,
        });

        let lines = build_transient_lines(&state, 100, 12);
        let rendered = line_texts(&lines);

        assert!(rendered.iter().any(|line| line.contains("Agent")));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("Tool · web_search #call-1"))
        );
        assert!(rendered.iter().any(|line| line.contains("Input:")));
        assert!(rendered.iter().any(|line| line.contains("Final answer")));
    }

    #[test]
    fn live_turn_can_fill_the_full_message_viewport() {
        let mut state = AppState::empty();
        state.active_cell = Some(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: (1..=30)
                .map(|index| format!("line {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
            group: MessageGroup::Conversation,
            is_active: true,
        });

        let viewport = build_viewport_snapshot(&state, (80, 24));
        let rendered = line_texts(&viewport.lines);

        assert_eq!(viewport.top, 0);
        assert_eq!(viewport.lines.len(), 24);
        assert!(rendered.iter().any(|line| line.contains("line 30")));
        assert!(rendered.iter().any(|line| line.starts_with('┌')));
    }

    #[test]
    fn message_viewport_only_scrolls_live_turn() {
        let mut state = AppState::empty();
        state.conversation_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: None,
            body: (1..=20)
                .map(|index| format!("stable {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
            group: MessageGroup::Conversation,
            is_active: false,
        });
        state.active_cell = Some(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: Some("typing…".to_string()),
            body: (1..=20)
                .map(|index| format!("live {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
            group: MessageGroup::Conversation,
            is_active: true,
        });

        let bottom = line_texts(&build_viewport_snapshot(&state, (80, 12)).lines);
        assert!(bottom.iter().any(|line| line.contains("live 20")));
        assert!(!bottom.iter().any(|line| line.contains("stable")));

        state.message_scroll_from_bottom = 5;
        let scrolled = line_texts(&build_viewport_snapshot(&state, (80, 12)).lines);
        assert!(!scrolled.iter().any(|line| line.contains("stable")));
        assert!(!scrolled.iter().any(|line| line.contains("live 20")));
        assert!(scrolled.iter().any(|line| line.contains("live 15")));
    }

    #[test]
    fn preserve_first_line_tail_keeps_header_and_latest_body() {
        let lines = preserve_first_line_tail(
            vec![
                Line::from("Header"),
                Line::from("old"),
                Line::from("middle"),
                Line::from("new"),
            ],
            3,
        );

        assert_eq!(line_texts(&lines), vec!["Header", "middle", "new"]);
    }

    #[test]
    fn preserve_active_cell_separator_keeps_header_after_separator() {
        let lines = preserve_active_cell_separator(
            vec![
                Line::from("Header"),
                Line::from("old"),
                Line::from("middle"),
                Line::from("new"),
            ],
            3,
            true,
        );

        assert_eq!(line_texts(&lines), vec!["", "Header", "new"]);
    }

    #[test]
    fn transcript_view_filters_empty_user_cells() {
        let lines = super::build_cell_lines(
            &[
                TranscriptCell {
                    kind: TranscriptCellKind::User,
                    title: "You".to_string(),
                    subtitle: None,
                    body: String::new(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
                TranscriptCell {
                    kind: TranscriptCellKind::User,
                    title: "You".to_string(),
                    subtitle: None,
                    body: "hello".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
            ],
            40,
        );
        let rendered = line_texts(&lines);
        assert_eq!(rendered[0], "You");
        assert_eq!(rendered[1], "  hello");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn transcript_wraps_long_body_lines() {
        let lines = super::build_cell_lines(
            &[TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Agent".to_string(),
                subtitle: None,
                body: "abcdefghijklmno".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            }],
            8,
        );

        assert_eq!(
            line_texts(&lines),
            vec!["Agent", "  abcdef", "  ghijkl", "  mno"]
        );
    }

    #[test]
    fn normalize_body_lines_trims_edges_and_compacts_blank_runs() {
        let lines = normalize_body_lines("\n\nhello\n\n\nworld\n\n");
        assert_eq!(lines, vec!["hello", "", "world"]);
    }

    #[test]
    fn append_lines_prepend_separator_when_history_exists() {
        let cells = vec![TranscriptCell {
            kind: TranscriptCellKind::User,
            title: "You".to_string(),
            subtitle: None,
            body: "hello".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        }];
        let lines = render_history_append_lines(&cells, 40, true);
        let rendered = line_texts(&lines);
        assert_eq!(rendered[0], "");
        assert_eq!(rendered[1], "You");
    }

    #[test]
    fn prefix_check_requires_identical_leading_cells() {
        let user = TranscriptCell {
            kind: TranscriptCellKind::User,
            title: "You".to_string(),
            subtitle: None,
            body: "hello".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        };
        let assistant = TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: None,
            body: "hi".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        };
        assert!(is_cell_prefix(
            std::slice::from_ref(&user),
            &[user.clone(), assistant.clone()]
        ));
        assert!(!is_cell_prefix(
            std::slice::from_ref(&assistant),
            &[user, assistant.clone()]
        ));
    }

    #[test]
    fn changed_row_indices_only_returns_modified_rows() {
        let previous = vec![Line::from("a"), Line::from("b"), Line::from("c")];
        let current = vec![Line::from("a"), Line::from("x"), Line::from("c")];
        assert_eq!(changed_row_indices(&previous, &current), vec![1]);
    }

    #[test]
    fn bottom_anchor_lines_pads_from_top() {
        let visible = bottom_anchor_lines(vec![Line::from("one"), Line::from("two")], 4, 0);
        assert_eq!(line_texts(&visible), vec!["", "", "one", "two"]);
    }

    #[test]
    fn bottom_anchor_lines_supports_scrollback_offset() {
        let visible = bottom_anchor_lines(
            vec![
                Line::from("one"),
                Line::from("two"),
                Line::from("three"),
                Line::from("four"),
                Line::from("five"),
            ],
            2,
            1,
        );

        assert_eq!(line_texts(&visible), vec!["three", "four"]);
    }

    #[test]
    fn clamp_history_scroll_prevents_empty_overscroll() {
        assert_eq!(clamp_history_scroll(5, 2, 99), 3);
        assert_eq!(clamp_history_scroll(2, 5, 99), 0);
    }

    #[test]
    fn preserve_scrolled_offset_keeps_visible_anchor_when_content_changes() {
        assert_eq!(preserve_scrolled_offset(10, 13, 4), 7);
        assert_eq!(preserve_scrolled_offset(13, 10, 7), 4);
        assert_eq!(preserve_scrolled_offset(13, 10, 1), 0);
    }
}
