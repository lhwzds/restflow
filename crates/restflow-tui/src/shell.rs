use std::fmt;
use std::io::{Result as IoResult, Stdout, Write};

use crossterm::Command;
use crossterm::cursor::{MoveTo, MoveToColumn};
use crossterm::queue;
use crossterm::style::Print;
use crossterm::terminal::{self, Clear, ClearType};

use crate::state::AppState;
use crate::transcript::{TranscriptCell, TranscriptCellKind};

const CONTINUATION_PREFIX: &str = "  ";
const TOOL_SUMMARY_LIMIT: usize = 120;
const PROMPT_MIN_VISIBLE_ROWS: u16 = 1;
const PROMPT_MAX_VISIBLE_ROWS: u16 = 6;
const MAX_TRANSIENT_ROWS: u16 = 8;

pub struct ShellRenderer {
    stdout: Stdout,
    committed_cells: Vec<TranscriptCell>,
    history_lines: Vec<String>,
    pending_history_lines: Vec<String>,
    last_viewport: Option<ViewportSnapshot>,
    last_terminal_size: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ViewportSnapshot {
    top: u16,
    lines: Vec<String>,
    cursor_x: u16,
    cursor_y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptSnapshot {
    lines: Vec<String>,
    cursor_column: u16,
    cursor_row: u16,
}

impl ShellRenderer {
    pub fn new() -> Self {
        Self {
            stdout: std::io::stdout(),
            committed_cells: Vec::new(),
            history_lines: Vec::new(),
            pending_history_lines: Vec::new(),
            last_viewport: None,
            last_terminal_size: None,
        }
    }

    pub fn clear_screen(&mut self) -> IoResult<()> {
        queue_clear_visible(&mut self.stdout)?;
        self.last_viewport = None;
        self.last_terminal_size = None;
        self.stdout.flush()
    }

    pub fn sync(&mut self, state: &AppState) -> IoResult<()> {
        let size = normalize_terminal_size(terminal::size().unwrap_or((80, 24)));
        let viewport = build_viewport_snapshot(state, size);
        let stable_cells = build_stable_history_cells(state);

        let mut force_full_redraw = false;
        if !is_cell_prefix(&self.committed_cells, &stable_cells) {
            self.committed_cells.clear();
            self.history_lines.clear();
            self.pending_history_lines.clear();
            self.last_viewport = None;
            force_full_redraw = true;
        }

        let mut new_history_lines = render_history_append_lines(
            &stable_cells[self.committed_cells.len()..],
            size.0,
            !self.history_lines.is_empty() || !self.pending_history_lines.is_empty(),
        );
        if !self.pending_history_lines.is_empty() {
            let mut pending = std::mem::take(&mut self.pending_history_lines);
            pending.append(&mut new_history_lines);
            new_history_lines = pending;
        }

        let viewport_shape_changed = self.last_terminal_size != Some(size)
            || self.last_viewport.as_ref().is_none_or(|previous| {
                previous.top != viewport.top || previous.lines.len() != viewport.lines.len()
            });

        if force_full_redraw || viewport_shape_changed {
            queue_clear_visible(&mut self.stdout)?;
            if !new_history_lines.is_empty() && viewport.top > 0 {
                self.append_history_lines(viewport.top, size.0, &new_history_lines)?;
                self.history_lines.extend(new_history_lines);
            } else if !new_history_lines.is_empty() {
                self.pending_history_lines.extend(new_history_lines);
            }
            self.redraw_visible_history(viewport.top, size.0)?;
            self.redraw_viewport_full(&viewport, size.0)?;
        } else {
            if !new_history_lines.is_empty() {
                if viewport.top > 0 {
                    self.append_history_lines(viewport.top, size.0, &new_history_lines)?;
                    self.history_lines.extend(new_history_lines);
                } else {
                    self.pending_history_lines.extend(new_history_lines);
                }
            }

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

        self.committed_cells = stable_cells;
        self.last_viewport = Some(viewport);
        self.last_terminal_size = Some(size);
        self.stdout.flush()
    }

    fn append_history_lines(
        &mut self,
        viewport_top: u16,
        width: u16,
        lines: &[String],
    ) -> IoResult<()> {
        if lines.is_empty() || viewport_top == 0 {
            return Ok(());
        }

        queue!(
            self.stdout,
            SetScrollRegion(1..viewport_top),
            MoveTo(0, viewport_top.saturating_sub(1))
        )?;
        for line in lines {
            queue!(
                self.stdout,
                Print("\r\n"),
                MoveToColumn(0),
                Clear(ClearType::CurrentLine),
                Print(truncate_to_width(line, width))
            )?;
        }
        queue!(self.stdout, ResetScrollRegion)?;
        Ok(())
    }

    fn redraw_visible_history(&mut self, viewport_top: u16, width: u16) -> IoResult<()> {
        let visible = bottom_anchor_lines(self.history_lines.clone(), viewport_top as usize, 0);
        for row in 0..viewport_top {
            let line = visible.get(row as usize).map(String::as_str).unwrap_or("");
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

    fn write_row(&mut self, row: u16, line: &str, width: u16) -> IoResult<()> {
        queue!(
            self.stdout,
            MoveTo(0, row),
            Clear(ClearType::CurrentLine),
            Print(truncate_to_width(line, width))
        )
    }
}

fn build_viewport_snapshot(state: &AppState, size: (u16, u16)) -> ViewportSnapshot {
    let (width, height) = size;
    let prompt = build_prompt_snapshot(state, width, height);
    let prompt_lines = format_prompt_box(width, &prompt.lines, &footer_status_line(state));
    let transient_capacity = height
        .saturating_sub(prompt_lines.len() as u16)
        .min(MAX_TRANSIENT_ROWS);
    let transient_lines = build_transient_lines(state, width, transient_capacity);
    let top = height.saturating_sub(transient_lines.len() as u16 + prompt_lines.len() as u16);

    let mut lines = transient_lines;
    let prompt_offset = lines.len() as u16;
    lines.extend(prompt_lines);

    ViewportSnapshot {
        top,
        lines,
        cursor_x: (1 + prompt.cursor_column).min(width.saturating_sub(2)),
        cursor_y: (top + prompt_offset + 1 + prompt.cursor_row).min(height.saturating_sub(1)),
    }
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
        state.composer.visible_lines(content_width, visible_rows)
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

fn build_stable_history_cells(state: &AppState) -> Vec<TranscriptCell> {
    let mut cells = Vec::with_capacity(
        state.conversation_cells.len() + state.pending_user_cells.len() + state.runtime_cells.len(),
    );

    let mut pending = state.pending_user_cells.iter().peekable();
    for (index, cell) in state.conversation_cells.iter().enumerate() {
        while let Some(entry) = pending.peek() {
            if entry.base_cell_index <= index {
                cells.push(entry.cell.clone());
                pending.next();
            } else {
                break;
            }
        }
        cells.push(cell.clone());
    }
    for entry in pending {
        cells.push(entry.cell.clone());
    }
    cells.extend(state.runtime_cells.clone());
    cells
}

fn build_transient_lines(state: &AppState, width: u16, max_rows: u16) -> Vec<String> {
    if max_rows == 0 {
        return Vec::new();
    }

    let cells = state.active_cell.clone().into_iter().collect::<Vec<_>>();

    let lines = build_cell_lines(&cells, width);
    tail_lines(lines, max_rows as usize)
}

fn render_history_append_lines(
    cells: &[TranscriptCell],
    width: u16,
    prepend_separator: bool,
) -> Vec<String> {
    let mut lines = build_cell_lines(cells, width);
    if prepend_separator && !lines.is_empty() {
        lines.insert(0, String::new());
    }
    lines
}

fn build_cell_lines(cells: &[TranscriptCell], width: u16) -> Vec<String> {
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
                    title
                } else {
                    format!("{title} {summary}")
                };
                lines.push(truncate_to_width(&line, width));
            }
            _ => {
                lines.push(truncate_to_width(&format_title(cell), width));
                for line in normalize_body_lines(cell.body.as_str()) {
                    lines.push(truncate_to_width(
                        &format!("{CONTINUATION_PREFIX}{line}"),
                        width,
                    ));
                }
            }
        }

        lines.push(String::new());
    }

    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

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

fn queue_clear_visible(writer: &mut impl Write) -> IoResult<()> {
    queue!(writer, Clear(ClearType::All), MoveTo(0, 0))
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

fn format_prompt_box(total_width: u16, draft_lines: &[String], footer: &str) -> Vec<String> {
    let inner_width = prompt_content_width(total_width);
    let mut lines = Vec::with_capacity(draft_lines.len() + 2);
    lines.push(format!("┌{}┐", "─".repeat(inner_width as usize)));
    for line in draft_lines {
        lines.push(format!("│{}│", pad_to_width(line, inner_width)));
    }
    let footer_text = truncate_to_width(footer, inner_width.saturating_sub(2));
    let mut bottom = String::from("└ ");
    bottom.push_str(&footer_text);
    let fill = inner_width
        .saturating_sub(2)
        .saturating_sub(display_width(&footer_text));
    bottom.push_str(&"─".repeat(fill as usize));
    bottom.push('┘');
    lines.push(bottom);
    lines
}

fn placeholder_line(inner_width: u16) -> String {
    truncate_to_width("Type your message or use /help", inner_width)
}

fn tail_lines(lines: Vec<String>, max_rows: usize) -> Vec<String> {
    if max_rows == 0 || lines.is_empty() {
        return Vec::new();
    }
    let start = lines.len().saturating_sub(max_rows);
    lines[start..].to_vec()
}

fn bottom_anchor_lines(
    lines: Vec<String>,
    height: usize,
    scroll_from_bottom: usize,
) -> Vec<String> {
    if height == 0 {
        return Vec::new();
    }
    let total = lines.len();
    let end = total.saturating_sub(scroll_from_bottom);
    let start = end.saturating_sub(height);
    let mut visible = lines[start..end].to_vec();
    if visible.len() < height {
        let mut padding = vec![String::new(); height - visible.len()];
        padding.append(&mut visible);
        return padding;
    }
    visible
}

fn changed_row_indices(previous: &[String], current: &[String]) -> Vec<usize> {
    let max_len = previous.len().max(current.len());
    let mut rows = Vec::new();
    for index in 0..max_len {
        if previous.get(index) != current.get(index) {
            rows.push(index);
        }
    }
    rows
}

fn pad_to_width(value: &str, width: u16) -> String {
    let truncated = truncate_to_width(value, width);
    let padding = width.saturating_sub(display_width(&truncated));
    let mut out = truncated;
    out.push_str(&" ".repeat(padding as usize));
    out
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SetScrollRegion(std::ops::Range<u16>);

impl Command for SetScrollRegion {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1b[{};{}r", self.0.start, self.0.end)
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        panic!("SetScrollRegion requires ANSI");
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResetScrollRegion;

impl Command for ResetScrollRegion {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1b[r")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        panic!("ResetScrollRegion requires ANSI");
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bottom_anchor_lines, build_stable_history_cells, build_transient_lines,
        build_viewport_snapshot, changed_row_indices, format_prompt_box, format_title,
        is_cell_prefix, normalize_body_lines, queue_clear_visible, render_history_append_lines,
        summarize_tool_body,
    };
    use crate::state::{AppState, PendingUserCell};
    use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};

    #[test]
    fn tool_summary_compacts_and_truncates_multiline_content() {
        let summary = summarize_tool_body(" \n {\"ok\": true}\n\nsecond line");
        assert_eq!(summary, "{\"ok\": true} second line");
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
    fn prompt_box_keeps_footer_and_borders() {
        let lines = format_prompt_box(20, &[String::from("hello")], "openai · gpt-5");
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with('┌'));
        assert!(lines[1].starts_with('│'));
        assert!(lines[2].starts_with("└ "));
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
    fn viewport_stays_anchored_to_bottom() {
        let state = AppState::empty();
        let viewport = build_viewport_snapshot(&state, (40, 10));
        assert_eq!(viewport.top, 7);
        assert_eq!(viewport.cursor_y, 8);
        assert_eq!(viewport.lines.len(), 3);
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
        state.runtime_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Tool,
            title: "Tool · switch_model".to_string(),
            subtitle: None,
            body: "{\"ok\":true}".to_string(),
            group: MessageGroup::ToolActivity,
            is_active: false,
        });
        state.runtime_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Notice,
            title: "Info".to_string(),
            subtitle: None,
            body: "Listed sessions".to_string(),
            group: MessageGroup::RuntimeNotice,
            is_active: false,
        });

        let cells = build_stable_history_cells(&state);

        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].kind, TranscriptCellKind::Tool);
        assert_eq!(cells[1].kind, TranscriptCellKind::Notice);
    }

    #[test]
    fn transient_view_only_contains_active_assistant() {
        let mut state = AppState::empty();
        state.runtime_cells.push(TranscriptCell {
            kind: TranscriptCellKind::Notice,
            title: "Info".to_string(),
            subtitle: None,
            body: "This should be committed history".to_string(),
            group: MessageGroup::RuntimeNotice,
            is_active: false,
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

        assert!(lines.iter().any(|line| line.contains("typing")));
        assert!(!lines.iter().any(|line| line.contains("committed history")));
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
        assert_eq!(lines[0], "You");
        assert_eq!(lines[1], "  hello");
        assert_eq!(lines.len(), 2);
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
        assert_eq!(lines[0], "");
        assert_eq!(lines[1], "You");
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
        let previous = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let current = vec!["a".to_string(), "x".to_string(), "c".to_string()];
        assert_eq!(changed_row_indices(&previous, &current), vec![1]);
    }

    #[test]
    fn bottom_anchor_lines_pads_from_top() {
        let visible = bottom_anchor_lines(vec!["one".to_string(), "two".to_string()], 4, 0);
        assert_eq!(visible, vec!["", "", "one", "two"]);
    }
}
