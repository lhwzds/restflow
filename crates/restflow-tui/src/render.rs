use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

const COMPOSER_BORDER_HEIGHT: u16 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellBottomViewport {
    pub lines: Vec<String>,
    pub cursor_x: u16,
    pub cursor_y: u16,
}

pub(crate) fn render_shell_bottom_viewport(
    width: u16,
    transient_lines: Vec<String>,
    prompt_lines: &[String],
    prompt_cursor_column: u16,
    prompt_cursor_row: u16,
    footer: &str,
) -> ShellBottomViewport {
    let prompt_height = prompt_lines.len() as u16 + COMPOSER_BORDER_HEIGHT;
    let transient_height = transient_lines.len() as u16;
    let height = (transient_height + prompt_height).max(1);
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);

    if transient_height > 0 {
        let transient_area = Rect::new(0, 0, width, transient_height);
        let transient = transient_lines
            .into_iter()
            .map(Line::from)
            .collect::<Vec<_>>();
        Paragraph::new(transient)
            .wrap(Wrap { trim: false })
            .render(transient_area, &mut buffer);
    }

    let prompt_area = Rect::new(0, transient_height, width, prompt_height);
    let footer_title = truncate_display_width(footer, width.saturating_sub(4));
    let block = Block::default()
        .borders(Borders::ALL)
        .title_bottom(format!(" {footer_title} "));
    let prompt = prompt_lines
        .iter()
        .cloned()
        .map(Line::from)
        .collect::<Vec<_>>();
    Paragraph::new(prompt)
        .block(block)
        .wrap(Wrap { trim: false })
        .render(prompt_area, &mut buffer);

    ShellBottomViewport {
        lines: buffer_to_plain_lines(&buffer),
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

fn buffer_to_plain_lines(buffer: &Buffer) -> Vec<String> {
    let mut lines = Vec::with_capacity(buffer.area.height as usize);
    for y in 0..buffer.area.height {
        let mut line = String::new();
        let mut x = 0;
        while x < buffer.area.width {
            if let Some(cell) = buffer.cell((x, y))
                && !cell.skip
            {
                let symbol = cell.symbol();
                line.push_str(symbol);
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
                continue;
            }
            x += 1;
        }
        lines.push(line.trim_end_matches(' ').to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::render_shell_bottom_viewport;

    #[test]
    fn bottom_viewport_keeps_footer_and_borders() {
        let prompt_lines = vec![String::from("hello")];
        let rendered =
            render_shell_bottom_viewport(20, Vec::new(), &prompt_lines, 0, 0, "openai · gpt-5");

        assert_eq!(rendered.lines.len(), 3);
        assert!(rendered.lines[0].starts_with('┌'));
        assert!(rendered.lines[1].starts_with('│'));
        assert!(rendered.lines[2].starts_with('└'));
        assert!(rendered.lines[2].contains("openai"));
    }

    #[test]
    fn bottom_viewport_preserves_cjk_without_spacer_cells() {
        let prompt_lines = vec![String::from("帮我打开浏览器")];
        let rendered =
            render_shell_bottom_viewport(32, Vec::new(), &prompt_lines, 0, 0, "openai · gpt-5");

        assert!(rendered.lines[1].contains("帮我打开浏览器"));
        assert!(!rendered.lines[1].contains("帮 我"));
    }

    #[test]
    fn bottom_viewport_places_cursor_after_transient_lines() {
        let prompt_lines = vec![String::from("hello")];
        let rendered = render_shell_bottom_viewport(
            24,
            vec!["Agent · typing…".to_string()],
            &prompt_lines,
            2,
            0,
            "model",
        );

        assert_eq!(rendered.cursor_x, 3);
        assert_eq!(rendered.cursor_y, 2);
    }
}
