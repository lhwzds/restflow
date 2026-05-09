use std::fmt;
use std::io::{ErrorKind, Result as IoResult, Write};

use crossterm::Command;
use crossterm::cursor::{MoveTo, MoveToColumn};
use crossterm::queue;
use crossterm::style::{
    Attribute, Color as CrosstermColor, Colors, Print, SetAttribute, SetBackgroundColor, SetColors,
    SetForegroundColor,
};
use crossterm::terminal::{Clear, ClearType};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::transcript::TranscriptCell;

#[derive(Default)]
pub struct ScrollbackWriter {
    committed_cells: Vec<TranscriptCell>,
    pending_lines: Vec<Line<'static>>,
}

impl ScrollbackWriter {
    pub fn reset(&mut self) {
        self.committed_cells.clear();
        self.pending_lines.clear();
    }

    pub fn is_prefix_of(&self, cells: &[TranscriptCell]) -> bool {
        is_cell_prefix(&self.committed_cells, cells)
    }

    pub fn sync_history<F>(&mut self, cells: &[TranscriptCell], width: u16, mut render_lines: F)
    where
        F: FnMut(&[TranscriptCell], u16) -> Vec<Line<'static>>,
    {
        if !self.is_prefix_of(cells) {
            self.reset();
        } else {
            // Clear stale pending lines when content changed but prefix still matches.
            // This prevents ghost lines accumulating when runtime_cells are updated
            // (e.g. tool results replaced, model switched) between sync cycles.
            self.pending_lines.clear();
        }

        let new_cells = &cells[self.committed_cells.len()..];
        let lines = render_lines(new_cells, width);
        self.pending_lines = lines;
        self.committed_cells = cells.to_vec();
    }

    pub fn insert_pending(
        &mut self,
        writer: &mut impl Write,
        viewport_top: u16,
        width: u16,
    ) -> IoResult<bool> {
        if self.pending_lines.is_empty() || viewport_top == 0 {
            return Ok(false);
        }
        let lines = std::mem::take(&mut self.pending_lines);
        match insert_history_lines(writer, viewport_top, width, &lines) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::Unsupported => {
                self.reset();
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
}

pub fn insert_history_lines(
    writer: &mut impl Write,
    viewport_top: u16,
    width: u16,
    lines: &[Line<'static>],
) -> IoResult<()> {
    if lines.is_empty() || viewport_top == 0 {
        return Ok(());
    }

    queue!(
        writer,
        SetScrollRegion(1..viewport_top),
        MoveTo(0, viewport_top.saturating_sub(1))
    )?;
    for line in lines {
        queue!(
            writer,
            Print("\r\n"),
            MoveToColumn(0),
            SetForegroundColor(CrosstermColor::Reset),
            SetBackgroundColor(CrosstermColor::Reset),
            SetAttribute(Attribute::Reset),
            Clear(ClearType::CurrentLine),
        )?;
        write_styled_line(writer, &truncate_line_to_width(line, width))?;
    }
    queue!(writer, ResetScrollRegion)?;
    Ok(())
}

fn is_cell_prefix(previous: &[TranscriptCell], current: &[TranscriptCell]) -> bool {
    previous.len() <= current.len()
        && previous
            .iter()
            .zip(current.iter())
            .all(|(left, right)| left == right)
}

fn truncate_line_to_width(line: &Line<'static>, width: u16) -> Line<'static> {
    let width = width as usize;
    let mut spans = Vec::new();
    let mut current_width = 0usize;

    for span in &line.spans {
        for ch in span.content.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
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
        let style = if line.style == ratatui::style::Style::default() {
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

fn line_from_spans(
    spans: Vec<Span<'static>>,
    style: ratatui::style::Style,
    alignment: Option<ratatui::layout::Alignment>,
) -> Line<'static> {
    let mut line = Line::from(spans);
    line.style = style;
    line.alignment = alignment;
    line
}

fn push_char_span(spans: &mut Vec<Span<'static>>, ch: char, style: ratatui::style::Style) {
    if let Some(last) = spans.last_mut()
        && last.style == style
    {
        last.content.to_mut().push(ch);
        return;
    }
    spans.push(Span::styled(ch.to_string(), style));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SetScrollRegion(std::ops::Range<u16>);

impl Command for SetScrollRegion {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1b[{};{}r", self.0.start, self.0.end)
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "SetScrollRegion requires ANSI support",
        ))
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
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "ResetScrollRegion requires ANSI support",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{ScrollbackWriter, insert_history_lines};
    use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};
    use ratatui::text::Line;
    use std::io::{Error, ErrorKind, Result as IoResult, Write};

    fn user_cell(body: &str) -> TranscriptCell {
        TranscriptCell {
            kind: TranscriptCellKind::User,
            title: "You".to_string(),
            subtitle: None,
            body: body.to_string(),
            group: MessageGroup::Conversation,
            is_active: false,
        }
    }

    #[test]
    fn sync_history_appends_only_new_cells() {
        let mut scrollback = ScrollbackWriter::default();
        let cells = vec![user_cell("one")];

        scrollback.sync_history(&cells, 80, |new_cells, _| {
            new_cells
                .iter()
                .map(|cell| Line::from(cell.body.clone()))
                .collect()
        });
        assert_eq!(scrollback.pending_lines.len(), 1);

        scrollback.sync_history(&cells, 80, |new_cells, _| {
            new_cells
                .iter()
                .map(|cell| Line::from(cell.body.clone()))
                .collect()
        });
        // Re-syncing the same cells produces no new pending lines since
        // the committed cells already cover the full content.
        assert_eq!(scrollback.pending_lines.len(), 0);

        let cells = vec![user_cell("one"), user_cell("two")];
        scrollback.sync_history(&cells, 80, |new_cells, _| {
            new_cells
                .iter()
                .map(|cell| Line::from(cell.body.clone()))
                .collect()
        });
        // Only the new cell ("two") produces pending lines since "one" is already committed.
        assert_eq!(scrollback.pending_lines.len(), 1);
    }

    #[test]
    fn sync_history_resets_when_prefix_changes() {
        let mut scrollback = ScrollbackWriter::default();
        scrollback.sync_history(&[user_cell("one")], 80, |new_cells, _| {
            new_cells
                .iter()
                .map(|cell| Line::from(cell.body.clone()))
                .collect()
        });

        scrollback.sync_history(&[user_cell("other")], 80, |new_cells, _| {
            new_cells
                .iter()
                .map(|cell| Line::from(cell.body.clone()))
                .collect()
        });

        let rendered = scrollback
            .pending_lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect::<String>();
        assert!(rendered.contains("other"));
    }

    #[test]
    fn insert_history_uses_region_above_retained_viewport() {
        let mut output = Vec::new();
        insert_history_lines(&mut output, 5, 40, &[Line::from("history line")])
            .expect("history insertion should render");
        let ansi = String::from_utf8(output).expect("history insertion should be utf8");

        assert!(ansi.contains("\u{1b}[1;5r"));
        assert!(ansi.contains("history line"));
        assert!(ansi.contains("\u{1b}[r"));
    }

    #[test]
    fn insert_pending_reports_whether_lines_were_inserted() {
        let mut scrollback = ScrollbackWriter::default();
        let mut output = Vec::new();

        let inserted = scrollback
            .insert_pending(&mut output, 5, 40)
            .expect("empty insert should be valid");
        assert!(!inserted);
        assert!(output.is_empty());

        scrollback.sync_history(&[user_cell("one")], 80, |new_cells, _| {
            new_cells
                .iter()
                .map(|cell| Line::from(cell.body.clone()))
                .collect()
        });
        let inserted = scrollback
            .insert_pending(&mut output, 5, 40)
            .expect("pending insert should be valid");

        assert!(inserted);
        assert!(scrollback.pending_lines.is_empty());
        assert!(String::from_utf8(output).unwrap().contains("one"));
    }

    #[test]
    fn insert_pending_falls_back_when_terminal_insert_is_unsupported() {
        struct UnsupportedWriter;

        impl Write for UnsupportedWriter {
            fn write(&mut self, _buf: &[u8]) -> IoResult<usize> {
                Err(Error::new(ErrorKind::Unsupported, "unsupported terminal"))
            }

            fn flush(&mut self) -> IoResult<()> {
                Ok(())
            }
        }

        let mut scrollback = ScrollbackWriter::default();
        scrollback.sync_history(&[user_cell("one")], 80, |new_cells, _| {
            new_cells
                .iter()
                .map(|cell| Line::from(cell.body.clone()))
                .collect()
        });

        let inserted = scrollback
            .insert_pending(&mut UnsupportedWriter, 5, 40)
            .expect("unsupported terminal should fall back to full redraw");

        assert!(!inserted);
        assert!(scrollback.pending_lines.is_empty());
        assert!(scrollback.committed_cells.is_empty());
    }
}
