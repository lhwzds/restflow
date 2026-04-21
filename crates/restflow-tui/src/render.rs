use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

const COMPOSER_BORDER_HEIGHT: u16 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellBottomViewport {
    pub lines: Vec<Line<'static>>,
    pub cursor_x: u16,
    pub cursor_y: u16,
}

pub(crate) fn render_shell_bottom_viewport(
    width: u16,
    transient_lines: Vec<Line<'static>>,
    prompt_lines: &[Line<'static>],
    prompt_cursor_column: u16,
    prompt_cursor_row: u16,
    footer: &str,
) -> ShellBottomViewport {
    let prompt_height = prompt_lines.len() as u16 + COMPOSER_BORDER_HEIGHT;
    let transient_height = transient_lines.len() as u16;
    let height = (transient_height + prompt_height).max(1);
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);

    for (row, line) in transient_lines.into_iter().enumerate() {
        if row as u16 >= transient_height {
            break;
        }
        line.render(Rect::new(0, row as u16, width, 1), &mut buffer);
    }

    let prompt_area = Rect::new(0, transient_height, width, prompt_height);
    let footer_title = truncate_display_width(footer, width.saturating_sub(4));
    let block = Block::default()
        .borders(Borders::ALL)
        .title_bottom(format!(" {footer_title} "));
    Paragraph::new(prompt_lines.to_vec())
        .block(block)
        .wrap(Wrap { trim: false })
        .render(prompt_area, &mut buffer);

    ShellBottomViewport {
        lines: buffer_to_styled_lines(&buffer),
        cursor_x: (1 + prompt_cursor_column).min(width.saturating_sub(2)),
        cursor_y: transient_height + 1 + prompt_cursor_row,
    }
}

fn truncate_display_width(value: &str, width: u16) -> String {
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

fn buffer_to_styled_lines(buffer: &Buffer) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(buffer.area.height as usize);
    for y in 0..buffer.area.height {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut x = 0;
        let last_content_x = last_content_cell_x(buffer, y);
        if last_content_x.is_none() {
            lines.push(Line::from(""));
            continue;
        }
        while x < buffer.area.width {
            if let Some(cell) = buffer.cell((x, y))
                && !cell.skip
            {
                if last_content_x.is_some_and(|last| x > last) {
                    break;
                }
                let symbol = cell.symbol();
                push_symbol(&mut spans, symbol, cell.style());
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
                continue;
            }
            x += 1;
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn last_content_cell_x(buffer: &Buffer, y: u16) -> Option<u16> {
    let mut last = None;
    for x in 0..buffer.area.width {
        if let Some(cell) = buffer.cell((x, y))
            && !cell.skip
            && !cell.symbol().trim_end_matches(' ').is_empty()
        {
            last = Some(x);
        }
    }
    last
}

fn push_symbol(spans: &mut Vec<Span<'static>>, symbol: &str, style: Style) {
    if let Some(last) = spans.last_mut()
        && last.style == style
    {
        last.content.to_mut().push_str(symbol);
        return;
    }
    spans.push(Span::styled(symbol.to_string(), style));
}

#[cfg(test)]
fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Style};
    use ratatui::text::Line;

    use super::line_text;
    use super::render_shell_bottom_viewport;

    #[test]
    fn bottom_viewport_keeps_footer_and_borders() {
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
    fn bottom_viewport_preserves_cjk_without_spacer_cells() {
        let prompt_lines = vec![Line::from("帮我打开浏览器")];
        let rendered =
            render_shell_bottom_viewport(32, Vec::new(), &prompt_lines, 0, 0, "openai · gpt-5");

        assert!(line_text(&rendered.lines[1]).contains("帮我打开浏览器"));
        assert!(!line_text(&rendered.lines[1]).contains("帮 我"));
    }

    #[test]
    fn bottom_viewport_places_cursor_after_transient_lines() {
        let prompt_lines = vec![Line::from("hello")];
        let rendered = render_shell_bottom_viewport(
            24,
            vec![Line::from("Agent · typing…")],
            &prompt_lines,
            2,
            0,
            "model",
        );

        assert_eq!(rendered.cursor_x, 3);
        assert_eq!(rendered.cursor_y, 2);
    }

    #[test]
    fn bottom_viewport_preserves_line_style() {
        let prompt_lines = vec![Line::from("hello")];
        let rendered = render_shell_bottom_viewport(
            24,
            vec![Line::from("Agent").style(Style::default().fg(Color::Yellow))],
            &prompt_lines,
            0,
            0,
            "model",
        );

        assert_eq!(rendered.lines[0].spans[0].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn bottom_viewport_does_not_emit_full_width_blank_rows() {
        let prompt_lines = vec![Line::from("hello")];
        let rendered = render_shell_bottom_viewport(
            24,
            vec![Line::from(""), Line::from("message")],
            &prompt_lines,
            0,
            0,
            "model",
        );

        assert_eq!(line_text(&rendered.lines[0]), "");
        assert_eq!(line_text(&rendered.lines[1]), "message");
    }
}
