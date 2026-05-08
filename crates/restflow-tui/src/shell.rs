use std::io::{Result as IoResult, Stdout, Write};
use std::path::Path;

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{
    Attribute, Color as CrosstermColor, Colors, Print, SetAttribute, SetBackgroundColor, SetColors,
    SetForegroundColor,
};
use crossterm::terminal::{self, Clear, ClearType};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use restflow_core::models::SkillSource;
use restflow_core::models::{
    ChatTurnEventKind, ChatTurnStatus, ExecutionTraceCategory, ExecutionTraceEvent, ToolCallPhase,
};
use serde_json::Value;

use crate::render::render_shell_bottom_viewport;
use crate::scrollback::ScrollbackWriter;
use crate::slash_command::{HELP_TEXT, SLASH_COMMAND_SPECS};
use crate::state::{AppState, WorkPickerItem, work_run_kind_label};
use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};

const CONTINUATION_PREFIX: &str = "  ";
const CLIPPED_CELL_MARKER: &str = "  ...";
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

        let needs_full_redraw = force_full_redraw || self.needs_full_redraw(size, &viewport);
        if needs_full_redraw {
            let clear_from = self
                .last_viewport
                .as_ref()
                .map(|previous| previous.top.min(viewport.top))
                .unwrap_or(viewport.top);
            self.clear_rows_from(clear_from, size.1, size.0)?;
            let inserted =
                self.scrollback
                    .insert_pending(&mut self.stdout, viewport.top, size.0)?;
            if !inserted {
                self.redraw_history_tail(viewport.top, size.0, &stable_cells)?;
            }
            self.redraw_viewport_full(&viewport, size.0)?;
        } else {
            let inserted =
                self.scrollback
                    .insert_pending(&mut self.stdout, viewport.top, size.0)?;
            if !inserted {
                self.redraw_history_tail(viewport.top, size.0, &stable_cells)?;
            }
            if should_force_live_viewport_redraw(state) {
                self.redraw_viewport_full(&viewport, size.0)?;
            } else {
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
        let inserted = self
            .scrollback
            .insert_pending(&mut self.stdout, viewport.top, size.0)?;
        if !inserted {
            self.redraw_history_tail(viewport.top, size.0, &stable_cells)?;
        }

        if should_force_live_viewport_redraw(state) {
            self.redraw_viewport_full(&viewport, size.0)?;
        } else {
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
        let mut visible_message_lines = if state.message_scroll_from_bottom == 0 {
            preserve_first_cell_tail(message_lines, message_height as usize)
        } else {
            tail_lines(
                message_lines,
                message_height as usize,
                state.message_scroll_from_bottom,
            )
        };
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
    let mut cells = Vec::with_capacity(state.conversation_cells.len() + state.runtime_cells.len());
    let mut runtime = state.runtime_cells.iter().peekable();
    for (index, cell) in state.conversation_cells.iter().enumerate() {
        if runtime
            .peek()
            .is_some_and(|entry| entry.base_cell_index == index)
            && cell.kind == TranscriptCellKind::User
        {
            cells.push(cell.clone());
            while let Some(entry) = runtime.peek() {
                if entry.base_cell_index == index {
                    if !should_hide_stable_runtime_cell(state, &entry.cell) {
                        cells.push(entry.cell.clone());
                    }
                    runtime.next();
                } else {
                    break;
                }
            }
            continue;
        }

        while let Some(entry) = runtime.peek() {
            if entry.base_cell_index == index {
                if !should_hide_stable_runtime_cell(state, &entry.cell) {
                    cells.push(entry.cell.clone());
                }
                runtime.next();
            } else {
                break;
            }
        }

        cells.push(cell.clone());
    }

    for entry in runtime {
        if !should_hide_stable_runtime_cell(state, &entry.cell) {
            cells.push(entry.cell.clone());
        }
    }
    if let Some(index) = active_turn_projection_start_index(state, &cells) {
        cells.truncate(index);
    }
    cells
}

fn should_hide_stable_runtime_cell(state: &AppState, cell: &TranscriptCell) -> bool {
    state.is_streaming && cell.kind == TranscriptCellKind::Subagent
}

fn active_turn_projection_start_index(state: &AppState, cells: &[TranscriptCell]) -> Option<usize> {
    let active_user = state
        .active_turn
        .as_ref()?
        .cells
        .iter()
        .find(|cell| cell.kind == TranscriptCellKind::User)?
        .body
        .trim_end();
    let session = state.thread.session.as_ref()?;
    let projected_by_running_turn = session.turns.last().is_some_and(|turn| {
        turn.status == ChatTurnStatus::Running
            && turn.events.iter().any(|event| {
                matches!(
                    &event.kind,
                    ChatTurnEventKind::UserMessage { content } if content.trim_end() == active_user
                )
            })
    });
    let projected_by_pending_legacy_message = session.messages.last().is_some_and(|message| {
        message.role == restflow_core::models::ChatRole::User
            && message.content.trim_end() == active_user
    });
    if !projected_by_running_turn && !projected_by_pending_legacy_message {
        return None;
    }
    cells.iter().rposition(|cell| {
        cell.kind == TranscriptCellKind::User && cell.body.trim_end() == active_user
    })
}

fn visible_history_tail_lines(
    stable_cells: &[TranscriptCell],
    width: u16,
    height: usize,
) -> Vec<Line<'static>> {
    let history_lines = render_history_append_lines(stable_cells, width);
    if stable_cells
        .iter()
        .filter(|cell| cell.kind == TranscriptCellKind::User)
        .count()
        <= 1
    {
        return bottom_pad_lines(preserve_first_cell_tail(history_lines, height), height);
    }
    bottom_anchor_lines(history_lines, height, 0)
}

fn build_message_lines(state: &AppState, width: u16, max_rows: u16) -> Vec<Line<'static>> {
    if max_rows == 0 {
        return Vec::new();
    }

    build_cell_lines(&build_live_message_cells(state), width)
}

fn build_live_message_cells(state: &AppState) -> Vec<TranscriptCell> {
    let Some(active_turn) = state.active_turn.as_ref() else {
        return Vec::new();
    };
    let has_assistant_cell = active_turn.cells.iter().any(|cell| {
        cell.kind == TranscriptCellKind::Assistant
            && (cell.is_active || !cell.body.trim().is_empty())
    });
    let has_runtime_cell = active_turn.cells.iter().any(|cell| {
        matches!(
            cell.kind,
            TranscriptCellKind::Tool | TranscriptCellKind::Subagent
        )
    });
    let subagent_activity_cells = state.activity.subagent_live_cells();
    if !state.is_streaming
        && !has_assistant_cell
        && !has_runtime_cell
        && active_turn.queued_updates.is_empty()
        && subagent_activity_cells.is_empty()
    {
        return Vec::new();
    }
    let mut cells = active_turn.cells.clone();
    if let Some(cell) = queued_update_notice_cell(&active_turn.queued_updates) {
        cells.push(cell);
    }
    cells.extend(subagent_activity_cells);
    cells
}

fn queued_update_notice_cell(queued_updates: &[String]) -> Option<TranscriptCell> {
    if queued_updates.is_empty() {
        return None;
    }
    let body = queued_updates
        .iter()
        .enumerate()
        .map(|(index, update)| {
            let update = update.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("{}. {}", index + 1, update)
        })
        .collect::<Vec<_>>()
        .join("\n");
    Some(TranscriptCell {
        kind: TranscriptCellKind::Notice,
        title: if queued_updates.len() == 1 {
            "Queued update".to_string()
        } else {
            "Queued updates".to_string()
        },
        subtitle: Some("waiting".to_string()),
        body,
        group: MessageGroup::RuntimeNotice,
        is_active: false,
    })
}

fn should_force_live_viewport_redraw(state: &AppState) -> bool {
    state.active_turn.is_some()
}

fn build_overlay_lines(state: &AppState, width: u16, max_rows: u16) -> Option<Vec<Line<'static>>> {
    if let Some(lines) = build_session_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_task_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_run_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_run_detail_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_skill_mention_picker_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_skill_manager_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_skill_detail_lines(state, width, max_rows) {
        return Some(lines);
    }

    if let Some(lines) = build_task_action_picker_lines(state, width) {
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

    if let Some(lines) = build_help_overlay_lines(state, width, max_rows) {
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

    if state.active_turn.is_none() {
        return Vec::new();
    }
    let pending_lines = Vec::new();
    let live_cells = build_live_message_cells(state);
    let mut active_lines = build_cell_lines(&live_cells, width);
    if live_cells.len() > 1 && active_lines.len() >= max_rows as usize {
        active_lines = live_cells
            .last()
            .map(|cell| build_cell_lines(std::slice::from_ref(cell), width))
            .unwrap_or_default();
    }
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
                session_message_count_label(session.message_count),
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

fn build_run_picker_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::RunPicker { selected }) = state.overlay.as_ref() else {
        return None;
    };

    let items = state.work_picker_items();
    let mut lines = vec![Line::from(vec![
        Span::styled("Work", tool_title_style()),
        Span::styled("  Up/Down select, Enter open, Esc close", muted_style()),
    ])];
    if items.is_empty() {
        lines.push(styled_line(
            "  No active work, runs, or background tasks.",
            muted_style(),
        ));
        return Some(lines);
    }

    let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
    let rows_per_item = 2usize;
    let visible_items = (visible_capacity / rows_per_item).max(1);
    let selected_index = (*selected).min(items.len().saturating_sub(1));
    let start = selected_index
        .saturating_sub(visible_items / 2)
        .min(items.len().saturating_sub(visible_items));
    let end = (start + visible_items).min(items.len());

    for (index, item) in items[start..end].iter().enumerate() {
        let index = start + index;
        let is_selected = index == selected_index;
        let marker = if is_selected { "› " } else { "  " };
        let title_style = if is_selected {
            tool_title_style()
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        match item {
            WorkPickerItem::BackgroundTask {
                task_id,
                title,
                status,
                latest_run_id,
                next_run_at,
                ..
            } => {
                let next_run = next_run_at
                    .map(|value| format!(" · next {value}"))
                    .unwrap_or_default();
                lines.extend(wrap_styled_line(
                    Line::from(vec![
                        Span::styled(
                            marker,
                            if is_selected {
                                tool_title_style()
                            } else {
                                muted_style()
                            },
                        ),
                        Span::styled("background task · ", muted_style()),
                        Span::styled(title.clone(), title_style),
                        Span::styled(format!(" · {status}{next_run}"), muted_style()),
                    ]),
                    width,
                ));
                lines.extend(wrap_styled_line(
                    Line::from(vec![
                        Span::styled("    id: ", muted_style()),
                        Span::styled(task_id.clone(), muted_style()),
                    ]),
                    width,
                ));
                if let Some(run_id) = latest_run_id {
                    lines.extend(wrap_styled_line(
                        Line::from(vec![
                            Span::styled("    run: ", muted_style()),
                            Span::styled(run_id.clone(), muted_style()),
                        ]),
                        width,
                    ));
                }
            }
            WorkPickerItem::Run {
                run_id,
                kind,
                title,
                status,
                ..
            } => {
                lines.extend(wrap_styled_line(
                    Line::from(vec![
                        Span::styled(
                            marker,
                            if is_selected {
                                tool_title_style()
                            } else {
                                muted_style()
                            },
                        ),
                        Span::styled(format!("{} · ", work_run_kind_label(*kind)), muted_style()),
                        Span::styled(title.clone(), title_style),
                        Span::styled(format!(" · {status}"), muted_style()),
                    ]),
                    width,
                ));
                lines.extend(wrap_styled_line(
                    Line::from(vec![
                        Span::styled("    run: ", muted_style()),
                        Span::styled(run_id.clone(), muted_style()),
                    ]),
                    width,
                ));
            }
        }
    }

    if end < items.len() {
        lines.push(styled_line(
            format!("  ... {} more", items.len() - end),
            muted_style(),
        ));
    }
    lines.truncate(max_rows as usize);
    Some(lines)
}

fn build_run_detail_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    if !matches!(
        state.overlay.as_ref(),
        Some(crate::state::OverlayState::RunDetail)
    ) {
        return None;
    }
    let Some(thread) = state.thread.execution_thread.as_ref() else {
        return Some(vec![styled_line("Run detail unavailable", muted_style())]);
    };
    let focus = &thread.focus;
    let run_id = focus
        .run_id
        .as_deref()
        .unwrap_or(focus.id.as_str())
        .to_string();
    let mut lines = vec![Line::from(vec![
        Span::styled("Run detail", tool_title_style()),
        Span::styled("  Esc close", muted_style()),
    ])];

    lines.extend(wrap_styled_line(
        Line::from(vec![
            Span::styled("  ", muted_style()),
            Span::styled(work_run_kind_label(focus.kind), muted_style()),
            Span::styled(" · ", muted_style()),
            Span::styled(
                focus.title.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" · {}", focus.status), muted_style()),
        ]),
        width,
    ));
    lines.extend(wrap_styled_line(
        Line::from(vec![
            Span::styled("  run: ", muted_style()),
            Span::styled(run_id, muted_style()),
            Span::styled(format!(" · events {}", focus.event_count), muted_style()),
        ]),
        width,
    ));
    if let Some(model) =
        run_model_label(focus.provider.as_deref(), focus.effective_model.as_deref())
    {
        lines.extend(wrap_styled_line(
            Line::from(vec![
                Span::styled("  model: ", muted_style()),
                Span::styled(model, muted_style()),
            ]),
            width,
        ));
    }
    if let Some(parent) = focus.parent_run_id.as_deref() {
        lines.extend(wrap_styled_line(
            Line::from(vec![
                Span::styled("  parent: ", muted_style()),
                Span::styled(parent.to_string(), muted_style()),
            ]),
            width,
        ));
    }
    if !state.thread.child_runs.is_empty() {
        lines.push(styled_line(
            format!("  child runs: {}", state.thread.child_runs.len()),
            muted_style(),
        ));
        for run in state.thread.child_runs.iter().take(3) {
            lines.extend(wrap_styled_line(
                Line::from(vec![
                    Span::styled("    - ", muted_style()),
                    Span::styled(work_run_kind_label(run.kind), muted_style()),
                    Span::styled(format!(" · {} · {}", run.title, run.status), muted_style()),
                ]),
                width,
            ));
        }
    }

    lines.push(styled_line("  Timeline", muted_style()));
    let events = thread
        .timeline
        .events
        .iter()
        .rev()
        .take(6)
        .collect::<Vec<_>>();
    if events.is_empty() {
        lines.push(styled_line(
            "    No timeline events recorded.",
            muted_style(),
        ));
    } else {
        for event in events.into_iter().rev() {
            lines.extend(wrap_styled_line(
                Line::from(vec![
                    Span::styled("    - ", muted_style()),
                    Span::styled(trace_event_label(event), muted_style()),
                ]),
                width,
            ));
        }
    }

    lines.truncate(max_rows as usize);
    Some(lines)
}

fn run_model_label(provider: Option<&str>, model: Option<&str>) -> Option<String> {
    match (provider, model) {
        (Some(provider), Some(model)) => Some(format!("{provider} · {model}")),
        (Some(provider), None) => Some(provider.to_string()),
        (None, Some(model)) => Some(model.to_string()),
        (None, None) => None,
    }
}

fn trace_event_label(event: &ExecutionTraceEvent) -> String {
    match event.category {
        ExecutionTraceCategory::ToolCall => {
            if let Some(tool) = event.tool_call.as_ref() {
                let phase = match tool.phase {
                    ToolCallPhase::Started => "started",
                    ToolCallPhase::Completed => {
                        if tool.success == Some(false) {
                            "failed"
                        } else {
                            "completed"
                        }
                    }
                };
                return format!("tool · {} · {phase}", tool.tool_name);
            }
            "tool".to_string()
        }
        ExecutionTraceCategory::Lifecycle => event
            .lifecycle
            .as_ref()
            .map(|lifecycle| {
                lifecycle
                    .message
                    .as_ref()
                    .or(lifecycle.error.as_ref())
                    .map(|message| format!("lifecycle · {} · {message}", lifecycle.status))
                    .unwrap_or_else(|| format!("lifecycle · {}", lifecycle.status))
            })
            .unwrap_or_else(|| "lifecycle".to_string()),
        ExecutionTraceCategory::Message => event
            .message
            .as_ref()
            .map(|message| {
                message
                    .content_preview
                    .as_ref()
                    .map(|preview| format!("message · {} · {preview}", message.role))
                    .unwrap_or_else(|| format!("message · {}", message.role))
            })
            .unwrap_or_else(|| "message".to_string()),
        ExecutionTraceCategory::LlmCall => event
            .llm_call
            .as_ref()
            .map(|llm| format!("llm · {}", llm.model))
            .unwrap_or_else(|| "llm".to_string()),
        ExecutionTraceCategory::ModelSwitch => event
            .model_switch
            .as_ref()
            .map(|switch| format!("model · {} -> {}", switch.from_model, switch.to_model))
            .unwrap_or_else(|| "model switch".to_string()),
        ExecutionTraceCategory::MetricSample => event
            .metric_sample
            .as_ref()
            .map(|metric| format!("metric · {} {}", metric.name, metric.value))
            .unwrap_or_else(|| "metric".to_string()),
        ExecutionTraceCategory::ProviderHealth => event
            .provider_health
            .as_ref()
            .map(|health| format!("provider · {} · {}", health.provider, health.status))
            .unwrap_or_else(|| "provider health".to_string()),
        ExecutionTraceCategory::LogRecord => event
            .log_record
            .as_ref()
            .map(|record| format!("log · {} · {}", record.level, record.message))
            .unwrap_or_else(|| "log".to_string()),
    }
}

fn build_skill_manager_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::SkillManager { selected }) = state.overlay.as_ref() else {
        return None;
    };

    let mut lines = vec![Line::from(vec![
        Span::styled("Skill Manager", tool_title_style()),
        Span::styled("  Up/Down select, Enter details, Esc close", muted_style()),
    ])];
    if state.skills.is_empty() {
        lines.push(styled_line("  No installed skills yet.", muted_style()));
        return Some(lines);
    }

    let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
    let rows_per_skill = 2usize;
    let visible_skills = (visible_capacity / rows_per_skill).max(1);
    let selected_skill_index = (*selected).min(state.skills.len().saturating_sub(1));
    let start = selected_skill_index
        .saturating_sub(visible_skills / 2)
        .min(state.skills.len().saturating_sub(visible_skills));
    let end = (start + visible_skills).min(state.skills.len());
    let mut previous_source = None;

    for (index, skill) in state.skills[start..end].iter().enumerate() {
        let index = start + index;
        let is_selected = index == selected_skill_index;
        if previous_source != Some(skill.source) {
            previous_source = Some(skill.source);
            push_if_space(
                &mut lines,
                max_rows,
                styled_line(
                    format!("  {}", skill_source_group_label(skill.source)),
                    muted_style(),
                ),
            );
        }
        let marker = if is_selected { "› " } else { "  " };
        let title_style = if is_selected {
            tool_title_style()
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let readonly = if skill.read_only { " · read-only" } else { "" };
        let delete_pending = "";
        let title = Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    tool_title_style()
                } else {
                    muted_style()
                },
            ),
            Span::styled(skill.name.clone(), title_style),
            Span::styled(
                format!(" · {}{readonly}{delete_pending}", skill.id),
                muted_style(),
            ),
        ]);
        lines.extend(wrap_styled_line(title, width));
        let description = skill
            .description
            .clone()
            .unwrap_or_else(|| "No description".to_string());
        let detail = Line::from(vec![
            Span::styled("    ", muted_style()),
            Span::styled(description, muted_style()),
        ]);
        lines.extend(wrap_styled_line(detail, width));
    }

    if end < state.skills.len() {
        push_if_space(
            &mut lines,
            max_rows,
            styled_line(
                format!("  ... {} more", state.skills.len() - end),
                muted_style(),
            ),
        );
    }
    lines.truncate(max_rows as usize);
    Some(lines)
}

fn build_skill_mention_picker_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    let Some(crate::state::OverlayState::SkillMentionPicker { selected }) = state.overlay.as_ref()
    else {
        return None;
    };
    let matches = state.skill_mention_matches();
    let query = state
        .composer
        .current_skill_mention_query()
        .unwrap_or_default();
    let mut lines = vec![Line::from(vec![
        Span::styled("Skill mentions", tool_title_style()),
        Span::styled("  Up/Down select, Enter insert, Esc close", muted_style()),
    ])];
    if matches.is_empty() {
        let message = if query.is_empty() {
            "  No skills installed."
        } else {
            "  No matching skills."
        };
        lines.push(styled_line(message, muted_style()));
        return Some(lines);
    }

    let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
    let selected_index = (*selected).min(matches.len().saturating_sub(1));
    let start = selected_index
        .saturating_sub(visible_capacity / 2)
        .min(matches.len().saturating_sub(visible_capacity));
    let end = (start + visible_capacity).min(matches.len());
    for (index, skill) in matches[start..end].iter().enumerate() {
        let index = start + index;
        let is_selected = index == selected_index;
        let title_style = if is_selected {
            tool_title_style()
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let line = Line::from(vec![
            Span::styled(
                if is_selected { "› " } else { "  " },
                if is_selected {
                    tool_title_style()
                } else {
                    muted_style()
                },
            ),
            Span::styled(format!("@{}", skill.id), title_style),
            Span::styled(format!("  {}", skill.name), muted_style()),
        ]);
        lines.extend(wrap_styled_line(line, width));
    }
    lines.truncate(max_rows as usize);
    Some(lines)
}

fn build_skill_detail_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    if !matches!(state.overlay, Some(crate::state::OverlayState::SkillDetail)) {
        return None;
    }
    let Some(skill) = state.selected_skill.as_ref() else {
        return Some(vec![
            Line::from(vec![
                Span::styled("Skill", tool_title_style()),
                Span::styled("  Esc close", muted_style()),
            ]),
            styled_line("  Skill details are unavailable.", muted_style()),
        ]);
    };

    let mut lines = vec![Line::from(vec![
        Span::styled("Skill", tool_title_style()),
        Span::styled("  Esc close", muted_style()),
    ])];
    let read_only = if skill.read_only { " · read-only" } else { "" };
    lines.extend(wrap_styled_line(
        Line::from(vec![
            Span::styled("  ", muted_style()),
            Span::styled(
                skill.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" · {}{read_only}", skill.id), muted_style()),
        ]),
        width,
    ));
    lines.extend(wrap_styled_line(
        Line::from(vec![
            Span::styled("  source: ", muted_style()),
            Span::styled(skill.source.to_string(), muted_style()),
        ]),
        width,
    ));
    if let Some(description) = skill.description.as_ref().filter(|value| !value.is_empty()) {
        lines.extend(wrap_styled_line(
            Line::from(vec![
                Span::styled("  description: ", muted_style()),
                Span::styled(description.clone(), muted_style()),
            ]),
            width,
        ));
    }
    if !skill.suggested_tools.is_empty() {
        lines.extend(wrap_styled_line(
            Line::from(vec![
                Span::styled("  suggested_tools: ", muted_style()),
                Span::styled(skill.suggested_tools.join(", "), muted_style()),
            ]),
            width,
        ));
    }
    if let Some(source_ref) = skill.source_ref.as_ref().filter(|value| !value.is_empty()) {
        lines.extend(wrap_styled_line(
            Line::from(vec![
                Span::styled("  source_ref: ", muted_style()),
                Span::styled(source_ref.clone(), muted_style()),
            ]),
            width,
        ));
    }
    if skill
        .suggested_tools
        .iter()
        .any(|tool| tool == "spawn_subagent_batch")
    {
        lines.extend(wrap_styled_line(
            Line::from(vec![
                Span::styled("  usage: ", muted_style()),
                Span::styled(
                    "Use this skill by asking for parallel/team/subagent work.",
                    muted_style(),
                ),
            ]),
            width,
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
    let selected_index = (*selected).min(state.model_items.len().saturating_sub(1));
    let (start, end) = picker_window_by_rows(
        &state.model_items,
        selected_index,
        visible_capacity,
        2,
        |item| item.category,
    );

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
        push_if_space(
            &mut lines,
            max_rows,
            styled_line(
                format!("  ... {} more", state.model_items.len() - end),
                muted_style(),
            ),
        );
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

    let selected_index = (*selected).min(state.provider_items.len().saturating_sub(1));
    let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
    let (start, end) = picker_window_by_rows(
        &state.provider_items,
        selected_index,
        visible_capacity,
        1,
        |item| item.category,
    );
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
        push_if_space(
            &mut lines,
            max_rows,
            styled_line(
                format!("  ... {} more", state.provider_items.len() - end),
                muted_style(),
            ),
        );
    }
    lines.truncate(max_rows as usize);
    Some(lines)
}

fn push_if_space(lines: &mut Vec<Line<'static>>, max_rows: u16, line: Line<'static>) {
    if lines.len() < max_rows as usize {
        lines.push(line);
    }
}

fn picker_window_by_rows<T>(
    items: &[T],
    selected: usize,
    row_capacity: usize,
    rows_per_item: usize,
    category: impl Fn(&T) -> crate::state::ModelPickerCategory + Copy,
) -> (usize, usize) {
    if items.is_empty() {
        return (0, 0);
    }

    let selected = selected.min(items.len().saturating_sub(1));
    let mut start = selected;
    let mut end = selected + 1;

    while start > 0
        && picker_window_row_count(&items[start - 1..end], rows_per_item, category) <= row_capacity
    {
        start -= 1;
    }

    while end < items.len()
        && picker_window_row_count(&items[start..end + 1], rows_per_item, category) <= row_capacity
    {
        end += 1;
    }

    (start, end)
}

fn picker_window_row_count<T>(
    items: &[T],
    rows_per_item: usize,
    category: impl Fn(&T) -> crate::state::ModelPickerCategory,
) -> usize {
    let mut rows = 0usize;
    let mut previous_category = None;
    for item in items {
        let item_category = category(item);
        if previous_category != Some(item_category) {
            rows += 1;
            previous_category = Some(item_category);
        }
        rows += rows_per_item;
    }
    rows
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

fn skill_source_group_label(source: SkillSource) -> &'static str {
    match source {
        SkillSource::System => "System skills",
        SkillSource::User => "User skills",
        SkillSource::External => "External skills",
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

fn build_help_overlay_lines(
    state: &AppState,
    width: u16,
    max_rows: u16,
) -> Option<Vec<Line<'static>>> {
    if !matches!(state.overlay, Some(crate::state::OverlayState::Help)) {
        return None;
    }

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Help", tool_title_style()),
        Span::styled("  Esc close", muted_style()),
    ]));
    for raw_line in HELP_TEXT.lines() {
        let line = if raw_line.is_empty() {
            Line::from("")
        } else {
            Line::from(vec![Span::styled(raw_line.to_string(), muted_style())])
        };
        lines.extend(wrap_styled_line(line, width));
    }

    lines.truncate(max_rows as usize);
    Some(lines)
}

fn compact_session_preview(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn session_message_count_label(message_count: u32) -> String {
    let noun = if message_count == 1 {
        "chat message"
    } else {
        "chat messages"
    };
    format!(" · {message_count} {noun}")
}

fn command_display(command: &str, args: &str) -> String {
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {args}")
    }
}

fn render_history_append_lines(cells: &[TranscriptCell], width: u16) -> Vec<Line<'static>> {
    let mut lines = build_cell_lines(cells, width);
    if !lines.is_empty() {
        lines.push(Line::from(""));
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
            TranscriptCellKind::Tool | TranscriptCellKind::Subagent
                if cell.group == MessageGroup::ToolActivity =>
            {
                lines.extend(wrap_display_line(
                    &format_title(cell),
                    width,
                    cell_title_style(cell),
                ));
                for line in cell_body_lines(cell) {
                    lines.extend(wrap_prefixed_line(
                        CONTINUATION_PREFIX,
                        &line,
                        width,
                        cell_body_style(cell),
                    ));
                }
            }
            TranscriptCellKind::Tool | TranscriptCellKind::Subagent => {
                let title = format_title(cell);
                let summary = summarize_tool_body(cell.body.as_str());
                let line = if summary.is_empty() {
                    styled_line(title, cell_title_style(cell))
                } else {
                    Line::from(vec![
                        Span::styled(title, cell_title_style(cell)),
                        Span::raw(" "),
                        Span::styled(summary, cell_body_style(cell)),
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
                for line in cell_body_lines(cell) {
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
        TranscriptCellKind::Tool | TranscriptCellKind::Subagent => {
            !summarize_tool_body(cell.body.as_str()).is_empty()
        }
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
        TranscriptCellKind::Subagent => subagent_title_style(),
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
        TranscriptCellKind::Subagent => subagent_body_style(),
        TranscriptCellKind::Assistant => Style::default(),
    }
}

fn tool_title_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn subagent_title_style() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
}

fn subagent_body_style() -> Style {
    Style::default().fg(Color::LightMagenta)
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

fn cell_body_lines(cell: &TranscriptCell) -> Vec<String> {
    if cell.kind == TranscriptCellKind::Assistant && cell.is_active && cell.body.trim().is_empty() {
        return vec![String::new()];
    }
    if matches!(
        cell.kind,
        TranscriptCellKind::Tool | TranscriptCellKind::Subagent
    ) && cell.group == MessageGroup::ToolActivity
    {
        let lines = structured_tool_activity_body_lines(cell.body.as_str());
        if !lines.is_empty() {
            return lines;
        }
    }
    normalize_body_lines(cell.body.as_str())
}

fn structured_tool_activity_body_lines(body: &str) -> Vec<String> {
    let mut lines = Vec::new();

    match json_after_tool_label(body, "Input:") {
        Some(input) => lines.push(
            summarize_tool_input_json(&input)
                .unwrap_or_else(|| format!("Input: {}", compact_json(&input))),
        ),
        None => {
            if let Some(input) = text_after_tool_label(body, "Input:") {
                lines.push(format!("Input: {}", compact_tool_text(input)));
            }
        }
    }

    append_tool_result_lines(&mut lines, body, "Output:");
    append_tool_result_lines(&mut lines, body, "Error:");
    lines
}

fn append_tool_result_lines(lines: &mut Vec<String>, body: &str, label: &str) {
    let display_label = label.trim_end_matches(':');
    match json_after_tool_label(body, label) {
        Some(value) => {
            let summary = match display_label {
                "Output" => summarize_tool_output_json(&value),
                "Error" => summarize_tool_error_json(&value),
                _ => None,
            }
            .unwrap_or_else(|| format!("{display_label}: {}", compact_json(&value)));
            lines.push(summary);
            append_process_stream_lines(lines, &value, "stdout");
            append_process_stream_lines(lines, &value, "stderr");
        }
        None => {
            if let Some(value) = text_after_tool_label(body, label) {
                lines.push(format!("{display_label}: {}", compact_tool_text(value)));
            }
        }
    }
}

fn append_process_stream_lines(lines: &mut Vec<String>, value: &Value, field: &str) {
    let Some(output) = value
        .get(field)
        .and_then(Value::as_str)
        .filter(|output| !output.trim().is_empty())
    else {
        return;
    };
    lines.push(format!("{field}:"));
    lines.extend(normalize_body_lines(output));
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn summarize_tool_body(body: &str) -> String {
    if let Some(summary) = summarize_structured_tool_body(body) {
        return truncate_tool_summary(&summary);
    }

    let compact = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if compact.is_empty() {
        return String::new();
    }

    truncate_tool_summary(&compact)
}

fn truncate_tool_summary(value: &str) -> String {
    let mut summary = value.chars().take(TOOL_SUMMARY_LIMIT).collect::<String>();
    if value.chars().count() > TOOL_SUMMARY_LIMIT {
        summary.push('…');
    }
    summary
}

fn summarize_structured_tool_body(body: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(input) = json_after_tool_label(body, "Input:")
        && let Some(summary) = summarize_tool_input_json(&input)
    {
        parts.push(summary);
    }
    match json_after_tool_label(body, "Output:") {
        Some(output) => {
            if let Some(summary) = summarize_tool_output_json(&output) {
                parts.push(summary);
            }
        }
        None => {
            if let Some(output) = text_after_tool_label(body, "Output:") {
                parts.push(format!("Output: {}", compact_tool_text(output)));
            }
        }
    }
    match json_after_tool_label(body, "Error:") {
        Some(error) => {
            if let Some(summary) = summarize_tool_error_json(&error) {
                parts.push(summary);
            }
        }
        None => {
            if let Some(error) = text_after_tool_label(body, "Error:") {
                parts.push(format!("Error: {}", compact_tool_text(error)));
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn json_after_tool_label(body: &str, label: &str) -> Option<Value> {
    let start = body.find(label)? + label.len();
    let rest = body[start..].trim_start();
    let end = ["\nInput:", "\nOutput:", "\nError:"]
        .iter()
        .filter_map(|marker| rest.find(marker))
        .min()
        .unwrap_or(rest.len());
    serde_json::from_str(rest[..end].trim()).ok()
}

fn text_after_tool_label<'a>(body: &'a str, label: &str) -> Option<&'a str> {
    let start = body.find(label)? + label.len();
    let rest = body[start..].trim_start();
    let end = ["\nInput:", "\nOutput:", "\nError:"]
        .iter()
        .filter_map(|marker| rest.find(marker))
        .min()
        .unwrap_or(rest.len());
    let value = rest[..end].trim();
    (!value.is_empty()).then_some(value)
}

fn compact_tool_text(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn summarize_tool_input_json(value: &Value) -> Option<String> {
    if let Some(summary) = summarize_task_input_json(value) {
        return Some(summary);
    }

    if let Some(command) = value
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(format!("Input: $ {}", compact_command(command)));
    }

    if let Some(pattern) = value
        .get("pattern")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let target = value
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(compact_tool_path)
            .unwrap_or_else(|| "workspace".to_string());
        let mode = value
            .get("output_mode")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        return Some(match mode {
            Some("files_with_matches") => format!("Input: grep files in {target} · {pattern}"),
            Some("count") => format!("Input: grep count in {target} · {pattern}"),
            _ => format!("Input: grep {target} · {pattern}"),
        });
    }

    if let Some(patch) = value
        .get("patch")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && let Some(summary) = summarize_patch_input(patch)
    {
        return Some(summary);
    }

    let path = value
        .get("file_path")
        .or_else(|| value.get("path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let path = compact_tool_path(path);
    if let Some(edits) = value.get("edits").and_then(Value::as_array) {
        return Some(format!(
            "Input: edit {path} · {} {}",
            edits.len(),
            replacement_label(edits.len())
        ));
    }
    if value.get("old_string").is_some() && value.get("new_string").is_some() {
        return Some(format!("Input: edit {path} · 1 replacement"));
    }
    if let Some(action) = value
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(format!("Input: {action} {path}"));
    }
    None
}

fn summarize_tool_output_json(value: &Value) -> Option<String> {
    if let Some(summary) = summarize_bash_result_json("Output", value) {
        return Some(summary);
    }

    if let Some(summary) = summarize_task_output_json(value) {
        return Some(summary);
    }

    if let Some(match_count) = value.get("match_count").and_then(Value::as_u64) {
        let label = count_label(match_count, "match", "matches");
        return Some(format!("Output: {match_count} {label}"));
    }
    if let Some(total) = value.get("total").and_then(Value::as_u64) {
        if value.get("files").and_then(Value::as_array).is_some() {
            let label = count_label(total, "matching file", "matching files");
            return Some(format!("Output: {total} {label}"));
        }
        if value.get("counts").and_then(Value::as_array).is_some() {
            let label = count_label(total, "file with matches", "files with matches");
            return Some(format!("Output: {total} {label}"));
        }
    }

    if let Some(results) = value.get("results").and_then(Value::as_array)
        && let Some(summary) = summarize_patch_results(results)
    {
        return Some(summary);
    }

    let has_edit_result =
        value.get("edits_applied").is_some() || value.get("lines_changed").is_some();
    let path = value
        .get("path")
        .or_else(|| value.get("file_path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let path = compact_tool_path(path);
    if !has_edit_result {
        return None;
    }
    let edits = value
        .get("edits_applied")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1);
    let mut summary = format!(
        "Output: edited {path} · {edits} {}",
        replacement_label(edits)
    );
    if let Some(lines_changed) = value.get("lines_changed").and_then(Value::as_u64) {
        let label = if lines_changed == 1 { "line" } else { "lines" };
        summary.push_str(&format!(" · {lines_changed} {label} changed"));
    }
    Some(summary)
}

fn summarize_task_input_json(value: &Value) -> Option<String> {
    let operation = value
        .get("operation")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let label = task_operation_input_label(operation);
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(short_tool_id);
    let target = name
        .map(compact_tool_text)
        .or(id)
        .unwrap_or_else(|| "task".to_string());
    Some(format!("Input: {label} {target}"))
}

fn summarize_task_output_json(value: &Value) -> Option<String> {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let result = value.get("result")?;
    let task = result.get("task").unwrap_or(result);
    task.get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let task_status = task
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(status)
        .unwrap_or("updated");
    let id = task
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(short_tool_id);
    let label = match status {
        Some("preview") => "previewed task",
        Some("deleted") => "deleted task",
        _ => "task",
    };
    let mut summary = format!("Output: {label} · {task_status}");
    if let Some(id) = id {
        summary.push_str(&format!(" · {id}"));
    }
    Some(summary)
}

fn task_operation_input_label(operation: &str) -> &'static str {
    match operation {
        "create" => "create task",
        "update" => "update task",
        "delete" => "delete task",
        "convert_session" => "convert session",
        "promote_to_background" => "promote background task",
        "run_batch" => "run task batch",
        "start" => "start task",
        "pause" => "pause task",
        "resume" => "resume task",
        "stop" => "stop task",
        "run_now" => "run task",
        _ => "manage task",
    }
}

fn short_tool_id(value: &str) -> String {
    value.chars().take(8).collect()
}

fn summarize_tool_error_json(value: &Value) -> Option<String> {
    summarize_bash_result_json("Error", value).or_else(|| {
        if value.get("pending_approval").and_then(Value::as_bool) == Some(true) {
            Some("Error: approval required".to_string())
        } else if value.get("blocked").and_then(Value::as_bool) == Some(true) {
            Some("Error: blocked by policy".to_string())
        } else {
            value
                .get("error")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|error| format!("Error: {}", compact_tool_text(error)))
        }
    })
}

fn summarize_bash_result_json(label: &str, value: &Value) -> Option<String> {
    let exit_code = value.get("exit_code").and_then(Value::as_i64)?;
    let mut parts = vec![format!("{label}: exit {exit_code}")];
    if let Some(duration_ms) = value.get("duration_ms").and_then(Value::as_u64) {
        parts.push(format!("{duration_ms}ms"));
    }
    if let Some(stdout_lines) = output_line_count(value.get("stdout").and_then(Value::as_str)) {
        parts.push(format!(
            "{stdout_lines} stdout {}",
            line_label(stdout_lines)
        ));
    }
    if let Some(stderr_lines) = output_line_count(value.get("stderr").and_then(Value::as_str)) {
        parts.push(format!(
            "{stderr_lines} stderr {}",
            line_label(stderr_lines)
        ));
    }
    if value.get("truncated").and_then(Value::as_bool) == Some(true) {
        parts.push("truncated".to_string());
    }
    Some(parts.join(" · "))
}

fn output_line_count(value: Option<&str>) -> Option<usize> {
    let count = value?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    (count > 0).then_some(count)
}

fn line_label(count: usize) -> &'static str {
    if count == 1 { "line" } else { "lines" }
}

fn count_label(count: u64, singular: &'static str, plural: &'static str) -> &'static str {
    if count == 1 { singular } else { plural }
}

fn summarize_patch_input(patch: &str) -> Option<String> {
    let operations = patch
        .lines()
        .filter_map(parse_patch_header)
        .collect::<Vec<_>>();
    if operations.is_empty() {
        return None;
    }
    let files = compact_patch_file_list(operations.iter().map(|(_, path)| path.as_str()));
    Some(format!(
        "Input: patch {} {} · {files}",
        operations.len(),
        file_label(operations.len())
    ))
}

fn parse_patch_header(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let (operation, path) = trimmed
        .strip_prefix("*** Update File: ")
        .map(|path| ("update", path))
        .or_else(|| {
            trimmed
                .strip_prefix("*** Add File: ")
                .map(|path| ("add", path))
        })
        .or_else(|| {
            trimmed
                .strip_prefix("*** Delete File: ")
                .map(|path| ("delete", path))
        })?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some((operation.to_string(), compact_tool_path(path)))
}

fn summarize_patch_results(results: &[Value]) -> Option<String> {
    let files = results
        .iter()
        .filter_map(Value::as_str)
        .filter_map(parse_patch_result_file)
        .collect::<Vec<_>>();
    if files.is_empty() {
        return None;
    }
    Some(format!(
        "Output: patched {} {} · {}",
        files.len(),
        file_label(files.len()),
        compact_patch_file_list(files.iter().map(String::as_str))
    ))
}

fn parse_patch_result_file(result: &str) -> Option<String> {
    let (_, path) = result.split_once(':')?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some(compact_tool_path(path))
}

fn compact_patch_file_list<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let paths = paths.collect::<Vec<_>>();
    let mut listed = paths.iter().take(3).copied().collect::<Vec<_>>().join(", ");
    let remaining = paths.len().saturating_sub(3);
    if remaining > 0 {
        listed.push_str(&format!(" +{remaining} more"));
    }
    listed
}

fn file_label(count: usize) -> &'static str {
    if count == 1 { "file" } else { "files" }
}

fn compact_command(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn replacement_label(count: usize) -> &'static str {
    if count == 1 {
        "replacement"
    } else {
        "replacements"
    }
}

fn compact_tool_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string())
}

fn footer_status_line(state: &AppState) -> String {
    let base = {
        let Some(session) = state.current_session() else {
            if let Some(pending_session) = state.pending_session.as_ref() {
                return append_background_footer(pending_session.model_label(), state);
            }
            return append_background_footer(state.status.clone(), state);
        };

        let provider = session.provider.trim();
        let model = session.model.trim();
        match (provider, model.is_empty()) {
            (provider, false) if !provider.is_empty() => format!("{provider} · {model}"),
            (_, false) => model.to_string(),
            _ => state.status.clone(),
        }
    };
    append_background_footer(base, state)
}

fn append_background_footer(base: String, state: &AppState) -> String {
    if let Some(work) = state.background_work.footer_label() {
        if base.trim().is_empty() {
            return work;
        }
        return format!("{base} · {work}");
    }
    base
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

fn bottom_pad_lines(mut lines: Vec<Line<'static>>, height: usize) -> Vec<Line<'static>> {
    if height == 0 || lines.len() >= height {
        return lines;
    }
    let mut padding = vec![Line::from(""); height - lines.len()];
    padding.append(&mut lines);
    padding
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

fn preserve_first_cell_tail(lines: Vec<Line<'static>>, height: usize) -> Vec<Line<'static>> {
    if height == 0 || lines.len() <= height {
        return lines;
    }
    if height == 1 {
        return lines.into_iter().take(1).collect();
    }

    let first_cell_len = lines
        .iter()
        .position(line_is_empty)
        .map(|index| index.max(1))
        .unwrap_or(1)
        .min(height - 1);
    let tail_len = height.saturating_sub(first_cell_len);
    let mut tail_start = lines.len().saturating_sub(tail_len);

    let mut visible = lines[..first_cell_len].to_vec();
    if tail_start > first_cell_len
        && line_starts_with(&lines[tail_start], CONTINUATION_PREFIX)
        && let Some(cell_start) = previous_cell_start(&lines, tail_start)
        && cell_start >= first_cell_len
        && cell_start < tail_start
        && visible.len() < height
    {
        visible.push(lines[cell_start].clone());
        let remaining = height.saturating_sub(visible.len());
        tail_start = lines.len().saturating_sub(remaining);
        if tail_start > cell_start + 1 && visible.len() < height {
            visible.push(styled_line(CLIPPED_CELL_MARKER, muted_style()));
            let remaining = height.saturating_sub(visible.len());
            tail_start = lines.len().saturating_sub(remaining);
        }
    }
    visible.extend_from_slice(&lines[tail_start.max(first_cell_len)..]);
    visible.truncate(height);
    visible
}

fn previous_cell_start(lines: &[Line<'static>], before: usize) -> Option<usize> {
    let mut index = before.min(lines.len());
    while index > 0 {
        index -= 1;
        if line_is_empty(&lines[index]) {
            return Some(index + 1);
        }
    }
    Some(0)
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

fn line_starts_with(line: &Line<'_>, prefix: &str) -> bool {
    let mut remaining = prefix;
    for span in &line.spans {
        let content = span.content.as_ref();
        if content.is_empty() {
            continue;
        }
        if remaining.len() <= content.len() {
            return content.starts_with(remaining);
        }
        if !remaining.starts_with(content) {
            return false;
        }
        remaining = &remaining[content.len()..];
    }
    remaining.is_empty()
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
        CLIPPED_CELL_MARKER, ViewportSnapshot, bottom_anchor_lines, build_stable_history_cells,
        build_transient_lines, build_viewport_snapshot, cell_title_style, changed_row_indices,
        clamp_history_scroll, compact_session_preview, footer_status_line, format_title,
        is_cell_prefix, line_text, normalize_body_lines, preserve_active_cell_separator,
        preserve_first_cell_tail, preserve_first_line_tail, preserve_scrolled_offset,
        protected_append_top, queue_clear_visible, queue_purge_visible_and_scrollback,
        render_history_append_lines, session_message_count_label,
        should_force_live_viewport_redraw, summarize_tool_body, visible_history_fill_count,
        visible_history_tail_lines, write_styled_line,
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
        ProviderPickerItem, SkillPickerItem, TaskPickerItem,
    };
    use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};
    use restflow_core::models::{
        ChatMessage, ChatSession, ChatSessionSummary, ChatTurnEventKind, ExecutionThread,
        ExecutionTimeline, ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceSource,
        ExecutionTraceStats, LifecycleTrace, RunKind, RunSummary, Skill, SkillSource,
    };
    use restflow_traits::{StreamFrame, TaskStreamEvent};

    fn line_texts(lines: &[Line<'static>]) -> Vec<String> {
        lines.iter().map(line_text).collect()
    }

    #[test]
    fn tool_summary_compacts_and_truncates_multiline_content() {
        let summary = summarize_tool_body(" \n {\"ok\": true}\n\nsecond line");
        assert_eq!(summary, "{\"ok\": true} second line");
    }

    #[test]
    fn tool_summary_formats_edit_input_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}",
            serde_json::json!({
                "file_path": "README.md",
                "old_string": "status=pending",
                "new_string": "status=done"
            })
        ));

        assert_eq!(summary, "Input: edit README.md · 1 replacement");
    }

    #[test]
    fn tool_summary_formats_file_read_without_claiming_edit() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nOutput: {}",
            serde_json::json!({
                "action": "read",
                "path": "README.md"
            }),
            serde_json::json!({
                "path": "/private/tmp/work/README.md",
                "content": "# Title\n"
            })
        ));

        assert_eq!(summary, "Input: read README.md");
        assert!(!summary.contains("edited"));
    }

    #[test]
    fn tool_summary_formats_successful_bash_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nOutput: {}",
            serde_json::json!({
                "command": "cargo   test  -p restflow-tui"
            }),
            serde_json::json!({
                "exit_code": 0,
                "stdout": "ok\n",
                "stderr": "",
                "truncated": false,
                "duration_ms": 42
            })
        ));

        assert_eq!(
            summary,
            "Input: $ cargo test -p restflow-tui Output: exit 0 · 42ms · 1 stdout line"
        );
    }

    #[test]
    fn tool_summary_formats_task_create_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nOutput: {}",
            serde_json::json!({
                "operation": "create",
                "name": "BG_TASK_PANEL_REAL_20260506_DELETE_ME",
                "agent_id": "agent-1",
                "schedule": {
                    "type": "interval",
                    "interval_ms": 3600000,
                    "start_at": null
                }
            }),
            serde_json::json!({
                "status": "executed",
                "result": {
                    "id": "b244bf7c-b07a-4c9c-9415-c3b1a4d6d19f",
                    "name": "BG_TASK_PANEL_REAL_20260506_DELETE_ME",
                    "status": "active"
                }
            })
        ));

        assert_eq!(
            summary,
            "Input: create task BG_TASK_PANEL_REAL_20260506_DELETE_ME Output: task · active · b244bf7c"
        );
    }

    #[test]
    fn tool_summary_formats_failed_bash_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nError: {}",
            serde_json::json!({
                "command": "cargo test"
            }),
            serde_json::json!({
                "exit_code": 101,
                "stdout": "",
                "stderr": "failed\nsecond\n",
                "truncated": true,
                "duration_ms": 900
            })
        ));

        assert_eq!(
            summary,
            "Input: $ cargo test Error: exit 101 · 900ms · 2 stderr lines · truncated"
        );
    }

    #[test]
    fn tool_summary_formats_structured_reviewer_error_without_raw_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nError: {}",
            serde_json::json!({
                "command": "sleep 120"
            }),
            serde_json::json!({
                "error": "Operation denied by reviewer: long sleep is not allowed.",
                "reason": "duplicate internal explanation",
                "review_denied": true
            })
        ));

        assert_eq!(
            summary,
            "Input: $ sleep 120 Error: Operation denied by reviewer: long sleep is not allowed."
        );
        assert!(!summary.contains("review_denied"));
        assert!(!summary.contains("duplicate internal explanation"));
    }

    #[test]
    fn tool_summary_formats_multiedit_input_and_output_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nOutput: {}",
            serde_json::json!({
                "file_path": "README.md",
                "edits": [
                    {"old_string": "a", "new_string": "b"},
                    {"old_string": "c", "new_string": "d"}
                ]
            }),
            serde_json::json!({
                "message": "2 edits applied to README.md (0 lines changed)",
                "path": "README.md",
                "edits_applied": 2,
                "lines_changed": 0
            })
        ));

        assert_eq!(
            summary,
            "Input: edit README.md · 2 replacements Output: edited README.md · 2 replacements · 0 lines changed"
        );
    }

    #[test]
    fn tool_summary_formats_grep_content_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nOutput: {}",
            serde_json::json!({
                "pattern": "status=pending",
                "path": "README.md",
                "output_mode": "content"
            }),
            serde_json::json!({
                "output": "1:status=pending",
                "match_count": 1
            })
        ));

        assert_eq!(
            summary,
            "Input: grep README.md · status=pending Output: 1 match"
        );
    }

    #[test]
    fn tool_summary_formats_grep_files_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nOutput: {}",
            serde_json::json!({
                "pattern": "status=",
                "path": "/private/tmp/work",
                "output_mode": "files_with_matches"
            }),
            serde_json::json!({
                "files": ["/private/tmp/work/README.md", "/private/tmp/work/TODO.md"],
                "total": 2
            })
        ));

        assert_eq!(
            summary,
            "Input: grep files in work · status= Output: 2 matching files"
        );
    }

    #[test]
    fn tool_summary_formats_grep_count_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nOutput: {}",
            serde_json::json!({
                "pattern": "TODO",
                "output_mode": "count"
            }),
            serde_json::json!({
                "counts": [
                    {"file": "/private/tmp/work/README.md", "count": 3}
                ],
                "total": 1
            })
        ));

        assert_eq!(
            summary,
            "Input: grep count in workspace · TODO Output: 1 file with matches"
        );
    }

    #[test]
    fn tool_summary_formats_patch_input_and_output_json() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nOutput: {}",
            serde_json::json!({
                "patch": "*** Update File: README.md\n-old\n+new\n*** Add File: notes/TODO.md\n+todo"
            }),
            serde_json::json!({
                "results": [
                    "Updated: /private/tmp/work/README.md",
                    "Created: /private/tmp/work/notes/TODO.md"
                ]
            })
        ));

        assert_eq!(
            summary,
            "Input: patch 2 files · README.md, TODO.md Output: patched 2 files · README.md, TODO.md"
        );
    }

    #[test]
    fn tool_summary_formats_patch_plain_text_error() {
        let summary = summarize_tool_body(&format!(
            "Input: {}\nError: {}",
            serde_json::json!({
                "patch": "*** Update File: README.md\n-status=pending\n+status=patch_checked"
            }),
            "File README.md has not been read. Read it before patching."
        ));

        assert_eq!(
            summary,
            "Input: patch 1 file · README.md Error: File README.md has not been read. Read it before patching."
        );
    }

    #[test]
    fn tool_summary_formats_large_patch_without_raw_patch_text() {
        let summary = summarize_tool_body(&format!(
            "Input: {}",
            serde_json::json!({
                "patch": "*** Update File: a.txt\n-a\n+b\n*** Update File: b.txt\n-a\n+b\n*** Delete File: c.txt\n*** Add File: d.txt\n+d"
            })
        ));

        assert_eq!(
            summary,
            "Input: patch 4 files · a.txt, b.txt, c.txt +1 more"
        );
        assert!(!summary.contains("*** Update File"));
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
    fn first_turn_short_stable_history_stays_bottom_anchored() {
        let cells = vec![
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
                body: "OK".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
        ];

        let rendered = line_texts(&visible_history_tail_lines(&cells, 80, 8));

        assert_eq!(rendered.len(), 8);
        assert!(rendered[..2].iter().all(|line| line.is_empty()));
        assert_eq!(rendered[2], "You");
        assert!(rendered.iter().any(|line| line.contains("OK")));
        assert_eq!(rendered.last().map(String::as_str), Some(""));
    }

    #[test]
    fn finalized_first_turn_keeps_live_message_row_alignment() {
        let mut live_state = AppState::empty();
        live_state.push_local_user_message("first message stability check".to_string());
        live_state.start_assistant_typing();
        live_state.apply_stream_frame(StreamFrame::Ack {
            content: "FIRST_STABLE_OK".to_string(),
        });

        let live_viewport = build_viewport_snapshot(&live_state, (60, 18));
        let live_lines = line_texts(&live_viewport.lines);
        let live_user_row = live_viewport.top as usize
            + live_lines
                .iter()
                .position(|line| line == "You")
                .expect("live user row");

        let stable_cells = vec![
            TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "first message stability check".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
            TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Default Assistant".to_string(),
                subtitle: None,
                body: "FIRST_STABLE_OK".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
        ];
        let stable_viewport_top = build_viewport_snapshot(&AppState::empty(), (60, 18)).top;
        let stable_lines = line_texts(&visible_history_tail_lines(
            &stable_cells,
            60,
            stable_viewport_top as usize,
        ));
        let stable_user_row = stable_lines
            .iter()
            .position(|line| line == "You")
            .expect("stable user row");

        assert_eq!(stable_user_row, live_user_row);
    }

    #[test]
    fn first_turn_stable_history_keeps_user_cell_when_overflowing() {
        let cells = vec![
            TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "run lots of output".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
            TranscriptCell {
                kind: TranscriptCellKind::Tool,
                title: "Tool · bash".to_string(),
                subtitle: Some("#call-1".to_string()),
                body: format!(
                    "Input: {{\"command\":\"ls\"}}\nOutput: {}",
                    (1..=30)
                        .map(|index| format!("line {index}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                group: MessageGroup::ToolActivity,
                is_active: false,
            },
            TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Agent".to_string(),
                subtitle: None,
                body: "done".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            },
        ];

        let rendered = line_texts(&visible_history_tail_lines(&cells, 80, 12));

        assert!(rendered.iter().any(|line| line == "You"));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("run lots of output"))
        );
        assert!(rendered.iter().any(|line| line.contains("Tool · bash")));
        assert!(rendered.iter().any(|line| line.contains("done")));
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
        state.apply_stream_frame(StreamFrame::Ack {
            content: "first\nlatest visible message".to_string(),
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
        state.apply_stream_frame(StreamFrame::Ack {
            content: "live still visible".to_string(),
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
    fn stable_history_uses_persisted_conversation_order() {
        let mut state = AppState::empty();
        state.conversation_cells.push(TranscriptCell {
            kind: TranscriptCellKind::User,
            title: "You".to_string(),
            subtitle: None,
            body: "hello".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        });
        state.conversation_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: "Agent".to_string(),
            subtitle: None,
            body: "hi".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
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
    fn stable_history_keeps_non_streaming_team_runtime_summary_cells() {
        let mut state = AppState::empty();
        state.runtime_cells.push(AnchoredRuntimeCell {
            base_cell_index: 0,
            cell: TranscriptCell {
                kind: TranscriptCellKind::Subagent,
                title: "Subagent".to_string(),
                subtitle: Some("#call-team".to_string()),
                body: "Starting 1 subagent".to_string(),
                group: MessageGroup::ToolActivity,
                is_active: false,
            },
        });

        let cells = build_stable_history_cells(&state);

        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, TranscriptCellKind::Subagent);
        assert!(cells[0].body.contains("Starting 1 subagent"));
    }

    #[test]
    fn live_tool_cells_render_as_separate_chat_entries() {
        let mut state = AppState::empty();
        state.push_local_user_message("check disk".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-df".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command":"df -h | grep -i samsung"}),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-ls".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command":"ls"}),
        });

        let lines = line_texts(&super::build_message_lines(&state, 100, 12));
        let first_tool_index = lines
            .iter()
            .position(|line| line.contains("Tool · bash") && line.contains("#call-df"))
            .expect("first tool cell");
        let second_tool_index = lines
            .iter()
            .position(|line| line.contains("Tool · bash") && line.contains("#call-ls"))
            .expect("second tool cell");

        assert!(first_tool_index < second_tool_index);
        assert!(!lines.iter().any(|line| line.contains("Tool activity")));
        assert!(!lines[first_tool_index].contains("df -h"));
        assert!(lines.iter().any(|line| line.contains("df -h")));
        assert!(lines.iter().any(|line| line.contains("Input: $ ls")));
    }

    #[test]
    fn stable_history_keeps_runtime_cells_between_user_and_final_assistant() {
        let mut state = AppState::empty();
        state.conversation_cells.push(TranscriptCell {
            kind: TranscriptCellKind::User,
            title: "You".to_string(),
            subtitle: None,
            body: "run a tool".to_string(),
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
    fn stable_history_excludes_running_active_turn_from_session_projection() {
        let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
        session.record_turn_user_message("turn-1", "previous");
        session.complete_turn_with_assistant_message("turn-1", "done");
        session.record_turn_user_message("turn-2", "current");
        session.record_turn_event(
            "turn-2",
            ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "file".to_string(),
                arguments: "{}".to_string(),
            },
        );
        let mut state = AppState::empty();
        state.set_current_session(session);
        state.push_local_user_message("current".to_string());

        let cells = build_stable_history_cells(&state);

        assert_eq!(
            cells
                .iter()
                .map(|cell| cell.body.as_str())
                .collect::<Vec<_>>(),
            vec!["previous", "done"]
        );
    }

    #[test]
    fn stable_history_excludes_pending_legacy_user_when_active_turn_is_visible() {
        let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
        session.messages.push(ChatMessage::user("current"));
        let mut state = AppState::empty();
        state.set_current_session(session);
        state.push_local_user_message("current".to_string());
        state.start_assistant_typing();

        let cells = build_stable_history_cells(&state);

        assert!(cells.is_empty());
    }

    #[test]
    fn stable_history_keeps_previous_same_text_when_active_turn_is_not_persisted() {
        let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
        session.record_turn_user_message("turn-1", "repeat");
        session.complete_turn_with_assistant_message("turn-1", "done");
        let mut state = AppState::empty();
        state.set_current_session(session);
        state.push_local_user_message("repeat".to_string());

        let cells = build_stable_history_cells(&state);

        assert_eq!(
            cells
                .iter()
                .map(|cell| cell.body.as_str())
                .collect::<Vec<_>>(),
            vec!["repeat", "done"]
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
        state.apply_stream_frame(StreamFrame::Ack {
            content: "streaming".to_string(),
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
        assert!(rendered.iter().any(|line| line.contains("/skill")));
        assert!(rendered.iter().any(|line| line.contains("/task")));
        assert!(
            !rendered
                .iter()
                .any(|line| line.contains("/session open <session_id>"))
        );
        assert!(rendered.iter().any(|line| line.contains("/runs")));
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
    fn help_overlay_renders_as_transient_content() {
        let mut state = AppState::empty();
        state.open_help_overlay();

        let lines = build_transient_lines(&state, 100, 32);
        let rendered = line_texts(&lines);

        assert!(rendered[0].contains("Help"));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("RestFlow terminal shell"))
        );
        assert!(rendered.iter().any(|line| line.contains("/daemon")));
        assert!(rendered.iter().any(|line| line.contains("/skill")));
        assert!(!rendered.iter().any(|line| line.starts_with("Info")));
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
        assert!(rendered.iter().any(|line| line.contains("2 chat messages")));
        assert!(rendered.iter().any(|line| line.contains("0 chat messages")));
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
            latest_run_id: None,
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
    fn message_viewport_shows_background_tasks_and_subagent_runs() {
        let mut state = AppState::empty();
        state.push_local_user_message("coordinate live work".to_string());
        state.apply_stream_frame(StreamFrame::Start {
            stream_id: "run-1".to_string(),
        });
        state.apply_task_event(TaskStreamEvent::started(
            "task-1",
            "Daily digest",
            "agent-1",
            "api",
        ));
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-task".to_string(),
            name: "manage_tasks".to_string(),
            arguments: serde_json::json!({"operation":"promote_to_background"}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-task".to_string(),
            success: true,
            result: serde_json::json!({
                "status": "executed",
                "result": {
                    "task": {
                        "id": "task-1"
                    }
                }
            })
            .to_string(),
        });
        state.set_session_runs_and_child_runs(
            vec![RunSummary {
                id: "run-1".to_string(),
                kind: RunKind::WorkspaceRun,
                container_id: "session-1".to_string(),
                root_run_id: Some("run-1".to_string()),
                title: "Workspace run".to_string(),
                subtitle: None,
                status: "running".to_string(),
                updated_at: 1,
                started_at: Some(1),
                ended_at: None,
                session_id: Some("session-1".to_string()),
                run_id: Some("run-1".to_string()),
                task_id: None,
                parent_run_id: None,
                agent_id: Some("agent-1".to_string()),
                source_channel: None,
                source_conversation_id: None,
                effective_model: None,
                provider: None,
                event_count: 0,
            }],
            vec![RunSummary {
                id: "child-1".to_string(),
                kind: RunKind::SubagentRun,
                container_id: "session-1".to_string(),
                root_run_id: Some("run-1".to_string()),
                title: "Subagent run".to_string(),
                subtitle: None,
                status: "running".to_string(),
                updated_at: 2,
                started_at: Some(2),
                ended_at: None,
                session_id: Some("session-1".to_string()),
                run_id: Some("child-1".to_string()),
                task_id: None,
                parent_run_id: Some("run-1".to_string()),
                agent_id: Some("agent-2".to_string()),
                source_channel: None,
                source_conversation_id: None,
                effective_model: None,
                provider: None,
                event_count: 0,
            }],
        );

        let text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");

        assert!(!text.contains("Background work"));
        assert!(!text.contains("Daily digest"));
        assert!(text.contains("Subagents"));
        assert!(text.contains("Subagent run"));
        assert!(text.contains("child-1"));
        assert!(!text.contains("Workspace run"));
        assert!(!text.contains("Open a run with"));
    }

    #[test]
    fn message_viewport_hides_unrelated_background_task_after_manage_tasks_call() {
        let mut state = AppState::empty();
        state.push_local_user_message("create a background task".to_string());
        state.tasks = vec![
            TaskPickerItem {
                task_id: "task-1".to_string(),
                name: "Created task".to_string(),
                status: "Active".to_string(),
                next_run_at: None,
                latest_run_id: Some("run-task-1".to_string()),
            },
            TaskPickerItem {
                task_id: "task-2".to_string(),
                name: "Unrelated task".to_string(),
                status: "Active".to_string(),
                next_run_at: None,
                latest_run_id: Some("run-task-2".to_string()),
            },
        ];
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-task".to_string(),
            name: "manage_tasks".to_string(),
            arguments: serde_json::json!({"operation":"create","name":"Created task"}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-task".to_string(),
            success: true,
            result: serde_json::json!({
                "status": "executed",
                "result": {
                    "id": "task-1"
                }
            })
            .to_string(),
        });

        let text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");

        assert!(text.contains("create a background task"));
        assert!(text.contains("Tool · manage_tasks"));
        assert!(text.contains("task-1"));
        assert!(!text.contains("Unrelated task"));
    }

    #[test]
    fn message_viewport_hides_background_tasks_unrelated_to_current_turn() {
        let mut state = AppState::empty();
        state.push_local_user_message("edit a file".to_string());
        state.tasks = vec![TaskPickerItem {
            task_id: "task-1".to_string(),
            name: "Daily digest".to_string(),
            status: "Active".to_string(),
            next_run_at: None,
            latest_run_id: Some("run-task-1".to_string()),
        }];

        let text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");

        assert!(!text.contains("Current turn activity"));
        assert!(!text.contains("background task"));
        assert!(!text.contains("Daily digest"));
    }

    #[test]
    fn message_viewport_hides_unrelated_running_runs_during_active_turn() {
        let mut state = AppState::empty();
        state.push_local_user_message("edit a file".to_string());
        state.apply_stream_frame(StreamFrame::Start {
            stream_id: "run-current".to_string(),
        });
        state.thread.child_runs = vec![RunSummary {
            id: "child-1".to_string(),
            kind: RunKind::SubagentRun,
            container_id: "session-1".to_string(),
            root_run_id: Some("run-other".to_string()),
            title: "Other subagent run".to_string(),
            subtitle: None,
            status: "running".to_string(),
            updated_at: 2,
            started_at: Some(2),
            ended_at: None,
            session_id: Some("session-1".to_string()),
            run_id: Some("child-1".to_string()),
            task_id: None,
            parent_run_id: Some("run-other".to_string()),
            agent_id: Some("agent-2".to_string()),
            source_channel: None,
            source_conversation_id: None,
            effective_model: None,
            provider: None,
            event_count: 0,
        }];

        let text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");

        assert!(!text.contains("Current turn activity"));
        assert!(!text.contains("Other subagent run"));
        assert!(!text.contains("child-1"));
    }

    #[test]
    fn message_viewport_keeps_task_stream_activity_outside_message_panel() {
        let mut state = AppState::empty();
        state.push_local_user_message("run a background check".to_string());
        state.apply_task_event(TaskStreamEvent::progress(
            "task-1",
            "Compiling",
            Some(50),
            Some("main.rs".to_string()),
        ));

        let text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");
        let stable = super::build_stable_history_cells(&state);

        assert!(text.is_empty());
        assert!(footer_status_line(&state).contains("Work 1/1 running"));
        assert!(stable.is_empty());
        assert!(state.runtime_cells.is_empty());
    }

    #[test]
    fn message_viewport_hides_task_stream_activity_when_agent_is_idle() {
        let mut state = AppState::empty();
        state.apply_task_event(TaskStreamEvent::progress(
            "task-1",
            "Compiling",
            Some(50),
            Some("main.rs".to_string()),
        ));

        let text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");

        assert!(text.is_empty());
        assert!(state.activity.live_cells().is_empty());
        assert!(footer_status_line(&state).contains("Work 1/1 running"));
        assert!(state.runtime_cells.is_empty());
    }

    #[test]
    fn message_viewport_drops_task_stream_activity_when_task_finishes() {
        let mut state = AppState::empty();
        state.push_local_user_message("run build task".to_string());
        state.apply_task_event(TaskStreamEvent::started(
            "task-1", "Build", "agent-1", "api",
        ));
        assert!(footer_status_line(&state).contains("Work 1/1 running"));

        state.apply_task_event(TaskStreamEvent::completed("task-1", "Done", 1200));

        let text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");
        assert!(text.is_empty());
        assert!(state.activity.live_cells().is_empty());
        assert!(!footer_status_line(&state).contains("Work"));
        assert!(state.runtime_cells.is_empty());
        assert!(state.status.contains("completed"));
    }

    #[test]
    fn message_viewport_hides_work_notice_when_agent_is_idle() {
        let mut state = AppState::empty();
        state.tasks = vec![TaskPickerItem {
            task_id: "task-1".to_string(),
            name: "Daily digest".to_string(),
            status: "Active".to_string(),
            next_run_at: None,
            latest_run_id: None,
        }];
        state.set_session_runs_and_child_runs(
            Vec::new(),
            vec![RunSummary {
                id: "child-1".to_string(),
                kind: RunKind::SubagentRun,
                container_id: "session-1".to_string(),
                root_run_id: Some("run-1".to_string()),
                title: "Subagent run".to_string(),
                subtitle: None,
                status: "running".to_string(),
                updated_at: 2,
                started_at: Some(2),
                ended_at: None,
                session_id: Some("session-1".to_string()),
                run_id: Some("child-1".to_string()),
                task_id: None,
                parent_run_id: Some("run-1".to_string()),
                agent_id: Some("agent-2".to_string()),
                source_channel: None,
                source_conversation_id: None,
                effective_model: None,
                provider: None,
                event_count: 0,
            }],
        );

        let text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");

        assert!(!text.contains("Activity"));
        assert!(!text.contains("Daily digest"));
        assert!(!text.contains("subagent run"));
    }

    #[test]
    fn message_viewport_does_not_keep_open_run_focus_as_a_live_block() {
        let mut state = AppState::empty();
        let focus = RunSummary {
            id: "run-1".to_string(),
            kind: RunKind::WorkspaceRun,
            container_id: "session-1".to_string(),
            root_run_id: None,
            title: "Workspace run".to_string(),
            subtitle: Some("checking current migration".to_string()),
            status: "run_completed".to_string(),
            updated_at: 2,
            started_at: Some(1),
            ended_at: Some(2),
            session_id: Some("session-1".to_string()),
            run_id: Some("run-1".to_string()),
            task_id: None,
            parent_run_id: None,
            agent_id: Some("agent-1".to_string()),
            source_channel: None,
            source_conversation_id: None,
            effective_model: Some("deepseek-chat".to_string()),
            provider: Some("deepseek".to_string()),
            event_count: 1,
        };
        let thread = ExecutionThread {
            focus,
            timeline: ExecutionTimeline {
                events: vec![ExecutionTraceEvent {
                    id: "event-1".to_string(),
                    task_id: String::new(),
                    agent_id: "agent-1".to_string(),
                    category: ExecutionTraceCategory::Lifecycle,
                    source: ExecutionTraceSource::Runtime,
                    timestamp: 2,
                    subflow_path: Vec::new(),
                    run_id: Some("run-1".to_string()),
                    parent_run_id: None,
                    session_id: Some("session-1".to_string()),
                    turn_id: None,
                    requested_model: None,
                    effective_model: Some("deepseek-chat".to_string()),
                    provider: Some("deepseek".to_string()),
                    attempt: None,
                    llm_call: None,
                    tool_call: None,
                    model_switch: None,
                    lifecycle: Some(LifecycleTrace {
                        status: "run_completed".to_string(),
                        message: Some("done".to_string()),
                        error: None,
                        ai_duration_ms: Some(10),
                    }),
                    message: None,
                    metric_sample: None,
                    provider_health: None,
                    log_record: None,
                }],
                stats: ExecutionTraceStats {
                    total_events: 1,
                    lifecycle_count: 1,
                    ..ExecutionTraceStats::default()
                },
            },
        };
        state.thread.set_run_focus(
            "run-1".to_string(),
            thread,
            vec![RunSummary {
                id: "child-1".to_string(),
                kind: RunKind::SubagentRun,
                container_id: "session-1".to_string(),
                root_run_id: Some("run-1".to_string()),
                title: "Subagent run".to_string(),
                subtitle: None,
                status: "run_completed".to_string(),
                updated_at: 2,
                started_at: Some(1),
                ended_at: Some(2),
                session_id: Some("session-1".to_string()),
                run_id: Some("child-1".to_string()),
                task_id: None,
                parent_run_id: Some("run-1".to_string()),
                agent_id: Some("agent-2".to_string()),
                source_channel: None,
                source_conversation_id: None,
                effective_model: None,
                provider: None,
                event_count: 1,
            }],
        );

        let text = line_texts(&super::build_message_lines(&state, 100, 14)).join("\n");

        assert!(text.is_empty());
        assert!(!text.contains("Run"));
        assert!(!text.contains("Workspace run"));
        assert!(!text.contains("child-1"));
    }

    #[test]
    fn message_viewport_drops_activity_notice_when_turn_finishes() {
        let mut state = AppState::empty();
        state.push_local_user_message("coordinate live work".to_string());
        state.apply_stream_frame(StreamFrame::Start {
            stream_id: "run-1".to_string(),
        });
        state.set_session_runs_and_child_runs(
            Vec::new(),
            vec![RunSummary {
                id: "child-1".to_string(),
                kind: RunKind::SubagentRun,
                container_id: "session-1".to_string(),
                root_run_id: Some("run-1".to_string()),
                title: "Subagent run".to_string(),
                subtitle: None,
                status: "running".to_string(),
                updated_at: 2,
                started_at: Some(2),
                ended_at: None,
                session_id: Some("session-1".to_string()),
                run_id: Some("child-1".to_string()),
                task_id: None,
                parent_run_id: Some("run-1".to_string()),
                agent_id: Some("agent-2".to_string()),
                source_channel: None,
                source_conversation_id: None,
                effective_model: None,
                provider: None,
                event_count: 0,
            }],
        );

        let active_text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");
        assert!(active_text.contains("Subagents"));
        assert!(active_text.contains("Subagent run"));
        assert!(active_text.contains("child-1"));

        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        let completed_text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");
        assert!(completed_text.is_empty());
        assert!(!completed_text.contains("Current turn activity"));
        assert!(!completed_text.contains("child-1"));
    }

    #[test]
    fn message_viewport_hides_completed_tool_only_live_turn() {
        let mut state = AppState::empty();
        state.push_local_user_message("coordinate team".to_string());
        state.start_assistant_typing();
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-team".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({"specs":[{"task":"reply ok"}]}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-team".to_string(),
            success: true,
            result: serde_json::json!({"status":"completed"}).to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        let text = line_texts(&super::build_message_lines(&state, 100, 12)).join("\n");

        assert!(text.is_empty());
        assert!(!text.contains("Team"));
        assert!(!text.contains("call-team"));
    }

    #[test]
    fn skill_manager_lists_grouped_skills() {
        let mut state = AppState::empty();
        state.skills = vec![
            SkillPickerItem {
                id: "team".to_string(),
                name: "Team".to_string(),
                description: Some("Coordinate parallel subagents".to_string()),
                source: SkillSource::System,
                read_only: true,
            },
            SkillPickerItem {
                id: "regex-finder".to_string(),
                name: "Regex Finder".to_string(),
                description: Some("Find text with regex".to_string()),
                source: SkillSource::External,
                read_only: false,
            },
        ];
        state.open_skill_manager();

        let lines = build_transient_lines(&state, 100, 10);
        let text = line_texts(&lines).join("\n");

        assert!(text.contains("Skill Manager"));
        assert!(text.contains("System skills"));
        assert!(text.contains("External skills"));
        assert!(text.contains("Team · team · read-only"));
        assert!(text.contains("Regex Finder"));
    }

    #[test]
    fn skill_mention_picker_lists_matching_skills() {
        let mut state = AppState::empty();
        state.skills = vec![SkillPickerItem {
            id: "team".to_string(),
            name: "Team".to_string(),
            description: Some("Coordinate parallel subagents".to_string()),
            source: SkillSource::System,
            read_only: true,
        }];
        state.composer.replace("use @tea");
        state.open_skill_mention_picker();

        let lines = build_transient_lines(&state, 100, 10);
        let text = line_texts(&lines).join("\n");

        assert!(text.contains("Skill mentions"));
        assert!(text.contains("@team"));
    }

    #[test]
    fn skill_detail_renders_metadata_and_subagent_usage_hint() {
        let mut state = AppState::empty();
        let mut skill = Skill::new(
            "team".to_string(),
            "Team".to_string(),
            Some("Coordinate short-lived parallel subagents".to_string()),
            Some(vec!["system".to_string(), "team".to_string()]),
            "# Team".to_string(),
        );
        skill.source = SkillSource::External;
        skill.read_only = true;
        skill.source_ref = Some("skrun:team@0.1.0".to_string());
        skill.suggested_tools = vec!["spawn_subagent_batch".to_string()];
        state.open_skill_detail(skill);

        let lines = build_transient_lines(&state, 120, 10);
        let text = line_texts(&lines).join("\n");

        assert!(text.contains("Skill"));
        assert!(text.contains("Team · team · read-only"));
        assert!(text.contains("source: external"));
        assert!(text.contains("suggested_tools: spawn_subagent_batch"));
        assert!(text.contains("source_ref: skrun:team@0.1.0"));
        assert!(text.contains("Use this skill by asking for parallel/team/subagent work."));
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
    fn provider_picker_scrolls_to_selected_provider() {
        let mut state = AppState::empty();
        state.provider_items = (0..10)
            .map(|index| ProviderPickerItem {
                provider: format!("provider-{index}"),
                label: format!("provider-{index}"),
                category: ModelPickerCategory::Available,
                usage_count: 0,
                last_used_at: None,
                is_current: false,
            })
            .collect();
        state.open_provider_picker();
        for _ in 0..9 {
            state.move_overlay_selection(1);
        }

        let lines = build_transient_lines(&state, 80, 8);
        let text = line_texts(&lines).join("\n");
        assert!(text.contains("provider-9"));
        assert!(!text.contains("provider-0"));
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
    fn model_picker_scrolls_to_selected_model() {
        let mut state = AppState::empty();
        state.model_items = (0..8)
            .map(|index| ModelPickerItem {
                provider: "codex".to_string(),
                model: format!("model-{index}"),
                name: format!("Model {index}"),
                category: ModelPickerCategory::Available,
                usage_count: 0,
                last_used_at: None,
                is_current: false,
            })
            .collect();
        state.open_model_picker("codex");
        for _ in 0..7 {
            state.move_overlay_selection(1);
        }

        let lines = build_transient_lines(&state, 80, 8);
        let text = line_texts(&lines).join("\n");
        assert!(text.contains("Model 7"));
        assert!(text.contains("model: model-7"));
        assert!(!text.contains("Model 0"));
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
    fn session_message_count_label_names_chat_messages() {
        assert_eq!(session_message_count_label(0), " · 0 chat messages");
        assert_eq!(session_message_count_label(1), " · 1 chat message");
        assert_eq!(session_message_count_label(2), " · 2 chat messages");
    }

    #[test]
    fn live_viewport_forces_full_redraw_during_active_turn() {
        let mut state = AppState::empty();
        assert!(!should_force_live_viewport_redraw(&state));

        state.push_local_user_message("hello".to_string());

        assert!(should_force_live_viewport_redraw(&state));
    }

    #[test]
    fn transient_view_separates_active_assistant_from_prior_history() {
        let mut state = AppState::empty();
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "streaming".to_string(),
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
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "streaming".to_string(),
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
        state.apply_stream_frame(StreamFrame::Ack {
            content: "line one\nline two\nline three\nline four".to_string(),
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
    fn message_viewport_shows_pending_user_before_stream_start() {
        let mut state = AppState::empty();
        state.push_local_user_message("first message".to_string());
        state.start_assistant_typing();

        let lines = line_texts(&super::build_message_lines(&state, 80, 8));

        assert!(lines.iter().any(|line| line.contains("You")));
        assert!(lines.iter().any(|line| line.contains("first message")));
        assert!(lines.iter().any(|line| line.contains("Agent")));
        assert!(lines.iter().any(|line| line.contains("typing")));
    }

    #[test]
    fn first_live_message_keeps_row_when_assistant_text_starts() {
        let mut state = AppState::empty();
        state.push_local_user_message("first message stability check".to_string());
        state.start_assistant_typing();

        let before = line_texts(&build_viewport_snapshot(&state, (60, 18)).lines);
        state.apply_stream_frame(StreamFrame::Ack {
            content: "FIRST_STABLE_OK".to_string(),
        });
        let after = line_texts(&build_viewport_snapshot(&state, (60, 18)).lines);

        assert_eq!(
            before.iter().position(|line| line == "You"),
            after.iter().position(|line| line == "You")
        );
        assert!(after.iter().any(|line| line.contains("FIRST_STABLE_OK")));
    }

    #[test]
    fn live_turn_renders_tool_as_a_separate_cell() {
        let mut state = AppState::empty();
        state.apply_stream_frame(StreamFrame::Ack {
            content: "Checking first".to_string(),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "web_search".to_string(),
            arguments: serde_json::json!({"query": "离骚全文 屈原"}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-1".to_string(),
            result: "{\"ok\":true}".to_string(),
            success: true,
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "Final answer".to_string(),
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
    fn live_turn_renders_queued_updates_inside_message_panel() {
        let mut state = AppState::empty();
        state.begin_stream("turn-1".to_string());
        state.push_local_user_message("first".to_string());
        state.queue_active_turn_update("please use the shorter answer".to_string());

        let lines = build_transient_lines(&state, 100, 12);
        let rendered = line_texts(&lines);

        assert!(rendered.iter().any(|line| line == "You"));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("Queued update waiting"))
        );
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("1. please use the shorter answer"))
        );
    }

    #[test]
    fn live_tool_activity_expands_bash_stdout_lines() {
        let mut state = AppState::empty();
        state.push_local_user_message("run output command".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "printf 'LONG_TOOL_1\\nLONG_TOOL_2\\n'"}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-1".to_string(),
            result: serde_json::json!({
                "duration_ms": 3,
                "exit_code": 0,
                "stderr": "",
                "stdout": "LONG_TOOL_1\nLONG_TOOL_2\n",
                "truncated": false
            })
            .to_string(),
            success: true,
        });

        let rendered = line_texts(&super::build_message_lines(&state, 100, 30));

        assert!(rendered.iter().any(|line| line.contains("Input: $ printf")));
        assert!(rendered.iter().any(|line| line.contains("Output: exit 0")));
        assert!(rendered.iter().any(|line| line.trim() == "stdout:"));
        assert!(rendered.iter().any(|line| line.trim() == "LONG_TOOL_1"));
        assert!(rendered.iter().any(|line| line.trim() == "LONG_TOOL_2"));
        assert!(
            !rendered
                .iter()
                .any(|line| line.contains("Output:") && line.contains("\\nLONG_TOOL_2"))
        );
    }

    #[test]
    fn live_turn_can_fill_the_full_message_viewport() {
        let mut state = AppState::empty();
        state.apply_stream_frame(StreamFrame::Ack {
            content: (1..=30)
                .map(|index| format!("line {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
        });

        let viewport = build_viewport_snapshot(&state, (80, 24));
        let rendered = line_texts(&viewport.lines);

        assert_eq!(viewport.top, 0);
        assert_eq!(viewport.lines.len(), 24);
        assert!(rendered.iter().any(|line| line.contains("line 30")));
        assert!(rendered.iter().any(|line| line.starts_with('┌')));
    }

    #[test]
    fn bottom_live_viewport_keeps_user_cell_when_turn_overflows() {
        let mut state = AppState::empty();
        state.push_local_user_message("run lots of output".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: (1..=30)
                .map(|index| format!("line {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
        });

        let rendered = line_texts(&build_viewport_snapshot(&state, (80, 16)).lines);

        assert!(rendered.iter().any(|line| line == "You"));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("run lots of output"))
        );
        assert!(rendered.iter().any(|line| line.contains("line 30")));
    }

    #[test]
    fn overflowing_live_tool_cell_marks_clipped_body() {
        let mut lines = vec![
            Line::from("You"),
            Line::from("  run lots of output"),
            Line::from(""),
            Line::from("Tool · bash #call-1"),
        ];
        lines.extend((1..=20).map(|index| Line::from(format!("  LONG_TOOL_{index}"))));

        let rendered = line_texts(&preserve_first_cell_tail(lines, 8));

        assert_eq!(rendered[0], "You");
        assert_eq!(rendered[1], "  run lots of output");
        assert!(rendered.iter().any(|line| line == "Tool · bash #call-1"));
        assert!(rendered.iter().any(|line| line == CLIPPED_CELL_MARKER));
        assert!(rendered.iter().any(|line| line.contains("LONG_TOOL_20")));
        assert_eq!(rendered.len(), 8);
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
        state.apply_stream_frame(StreamFrame::Ack {
            content: (1..=20)
                .map(|index| format!("live {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
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
    fn append_lines_keep_bottom_spacer_when_history_exists() {
        let cells = vec![TranscriptCell {
            kind: TranscriptCellKind::User,
            title: "You".to_string(),
            subtitle: None,
            body: "hello".to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        }];
        let lines = render_history_append_lines(&cells, 40);
        let rendered = line_texts(&lines);
        assert_eq!(rendered[0], "You");
        assert_eq!(rendered.last().map(String::as_str), Some(""));
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
