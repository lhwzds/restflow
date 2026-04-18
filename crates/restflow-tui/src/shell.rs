use std::io::{Result as IoResult, Stdout, Write};

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::Print;
use crossterm::terminal::{self, Clear, ClearType};

use crate::state::AppState;
use crate::transcript::{TranscriptCell, TranscriptCellKind};

const CONTINUATION_PREFIX: &str = "  ";
const TOOL_SUMMARY_LIMIT: usize = 120;
const PROMPT_MIN_VISIBLE_ROWS: u16 = 1;
const PROMPT_MAX_VISIBLE_ROWS: u16 = 6;
const PROMPT_BORDER_ROWS: u16 = 2;

pub struct ShellRenderer {
    stdout: Stdout,
    last_snapshot: Option<FrameSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameSnapshot {
    width: u16,
    height: u16,
    prompt_top_row: u16,
    screen_lines: Vec<String>,
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
            last_snapshot: None,
        }
    }

    pub fn clear_screen(&mut self) -> IoResult<()> {
        queue_full_clear(&mut self.stdout)?;
        self.last_snapshot = None;
        self.stdout.flush()
    }

    pub fn sync(&mut self, state: &AppState) -> IoResult<()> {
        let snapshot = build_frame_snapshot(state, terminal::size().unwrap_or((80, 24)));
        if self.last_snapshot.as_ref() == Some(&snapshot) {
            return Ok(());
        }

        match self.last_snapshot.clone() {
            Some(previous) if previous.width == snapshot.width && previous.height == snapshot.height => {
                self.redraw_diff(&previous, &snapshot)?;
            }
            _ => self.redraw_full(&snapshot)?,
        }
        self.last_snapshot = Some(snapshot);
        Ok(())
    }

    fn redraw_full(&mut self, snapshot: &FrameSnapshot) -> IoResult<()> {
        queue_full_clear(&mut self.stdout)?;
        for (row, line) in snapshot.screen_lines.iter().enumerate() {
            self.write_row(row as u16, line, snapshot.width)?;
        }
        queue!(self.stdout, MoveTo(snapshot.cursor_x, snapshot.cursor_y))?;
        self.stdout.flush()
    }

    fn redraw_diff(&mut self, previous: &FrameSnapshot, snapshot: &FrameSnapshot) -> IoResult<()> {
        for row in changed_row_indices(&previous.screen_lines, &snapshot.screen_lines) {
            self.write_row(row as u16, &snapshot.screen_lines[row], snapshot.width)?;
        }
        queue!(self.stdout, MoveTo(snapshot.cursor_x, snapshot.cursor_y))?;
        self.stdout.flush()
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

fn build_frame_snapshot(state: &AppState, size: (u16, u16)) -> FrameSnapshot {
    let (width, height) = normalize_terminal_size(size);
    let prompt = build_prompt_snapshot(state, width, height);
    let prompt_lines = format_prompt_box(width, &prompt.lines, &footer_status_line(state));
    let prompt_height = prompt_lines.len() as u16;
    let prompt_top_row = height.saturating_sub(prompt_height);
    let transcript_height = prompt_top_row;
    let transcript_lines = bottom_anchor_lines(
        build_transcript_lines(&state.transcript_cells_for_render(), width),
        transcript_height as usize,
        state.transcript_scroll as usize,
    );
    let mut screen_lines = vec![String::new(); height as usize];
    for (row, line) in transcript_lines.into_iter().enumerate() {
        if row < screen_lines.len() {
            screen_lines[row] = line;
        }
    }
    for (offset, line) in prompt_lines.iter().enumerate() {
        let row = prompt_top_row as usize + offset;
        if row < screen_lines.len() {
            screen_lines[row] = line.clone();
        }
    }

    FrameSnapshot {
        width,
        height,
        prompt_top_row,
        screen_lines,
        cursor_x: (1 + prompt.cursor_column).min(width.saturating_sub(2)),
        cursor_y: (prompt_top_row + 1 + prompt.cursor_row).min(height.saturating_sub(1)),
    }
}

fn build_prompt_snapshot(state: &AppState, width: u16, height: u16) -> PromptSnapshot {
    let content_width = prompt_content_width(width);
    let max_visible_rows = height
        .saturating_sub(PROMPT_BORDER_ROWS)
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

fn build_transcript_lines(cells: &[TranscriptCell], width: u16) -> Vec<String> {
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
                    let content = format!("{CONTINUATION_PREFIX}{line}");
                    lines.push(truncate_to_width(&content, width));
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

fn should_render_cell(cell: &TranscriptCell) -> bool {
    match cell.kind {
        TranscriptCellKind::Tool => !summarize_tool_body(cell.body.as_str()).is_empty(),
        TranscriptCellKind::Assistant => cell.is_active || !cell.body.trim().is_empty(),
        TranscriptCellKind::User
        | TranscriptCellKind::System
        | TranscriptCellKind::Notice => !cell.body.trim().is_empty(),
    }
}

fn queue_full_clear(writer: &mut impl Write) -> IoResult<()> {
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

fn bottom_anchor_lines(lines: Vec<String>, height: usize, scroll_from_bottom: usize) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::{
        build_frame_snapshot, bottom_anchor_lines, changed_row_indices, format_prompt_box,
        format_title, normalize_body_lines, queue_full_clear, summarize_tool_body,
    };
    use crate::state::AppState;
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
    fn full_clear_homes_cursor_before_redraw() {
        let mut output = Vec::new();
        queue_full_clear(&mut output).expect("clear sequence should render");
        let ansi = String::from_utf8(output).expect("clear sequence should be utf8");
        assert!(ansi.contains("\u{1b}[2J"));
        assert!(ansi.contains("\u{1b}[1;1H") || ansi.contains("\u{1b}[H"));
    }

    #[test]
    fn prompt_snapshot_stays_anchored_to_bottom() {
        let state = AppState::empty();
        let snapshot = build_frame_snapshot(&state, (40, 10));
        assert_eq!(snapshot.prompt_top_row, 7);
        assert_eq!(snapshot.cursor_y, 8);
        assert_eq!(snapshot.screen_lines.len(), 10);
        assert!(snapshot.screen_lines[7].starts_with('┌'));
    }

    #[test]
    fn transcript_view_filters_empty_user_cells() {
        let lines = super::build_transcript_lines(
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
    fn bottom_anchor_lines_pads_from_top() {
        let visible = bottom_anchor_lines(
            vec!["one".to_string(), "two".to_string()],
            4,
            0,
        );
        assert_eq!(visible, vec!["", "", "one", "two"]);
    }

    #[test]
    fn normalize_body_lines_trims_edges_and_compacts_blank_runs() {
        let lines = normalize_body_lines("\n\nhello\n\n\nworld\n\n");
        assert_eq!(lines, vec!["hello", "", "world"]);
    }

    #[test]
    fn changed_row_indices_only_returns_modified_rows() {
        let previous = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let current = vec!["a".to_string(), "x".to_string(), "c".to_string()];
        assert_eq!(changed_row_indices(&previous, &current), vec![1]);
    }
}
