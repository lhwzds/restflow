use crossterm::{
    cursor::{MoveTo, Show, position},
    execute,
    style::Print,
    terminal::size,
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use std::io::{self, Write};

/// Terminal with inline viewport (no AlternateScreen)
pub struct ViewportTerminal {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    viewport_area: Rect,
}

impl ViewportTerminal {
    pub fn new() -> io::Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        let (_, cursor_y) = position()?;

        Ok(Self {
            terminal,
            viewport_area: Rect::new(0, cursor_y, 0, 0),
        })
    }

    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }

    pub fn setup_viewport_from(&mut self, start_y: u16, height: u16) -> io::Result<()> {
        let (term_width, term_height) = size()?;
        self.viewport_area = Rect::new(0, start_y, term_width, height);

        // Scroll terminal if viewport overflows bottom
        if self.viewport_area.bottom() > term_height {
            let scroll_amount = self.viewport_area.bottom() - term_height;
            for _ in 0..scroll_amount {
                println!();
            }
            self.viewport_area.y = term_height.saturating_sub(height);
        }

        Ok(())
    }

    pub fn adjust_viewport_height(&mut self, new_height: u16) -> io::Result<()> {
        let (term_width, term_height) = size()?;

        let desired_height = new_height.min(term_height);

        self.viewport_area.width = term_width;
        self.viewport_area.height = desired_height;

        // Calculate where viewport bottom would be
        let current_bottom = self.viewport_area.y + desired_height;

        if current_bottom > term_height {
            // Viewport exceeds screen bottom - scroll down
            let scroll_amount = current_bottom - term_height;
            for _ in 0..scroll_amount {
                println!();
            }
            self.viewport_area.y = term_height.saturating_sub(desired_height);
        }
        // Otherwise: keep viewport.y unchanged (Codex pattern)

        Ok(())
    }

    /// Legacy method for backward compatibility
    #[allow(dead_code)]
    pub fn setup_viewport(&mut self, height: u16) -> io::Result<()> {
        let (_, cursor_y) = position()?;
        self.setup_viewport_from(cursor_y, height)
    }

    pub fn insert_history_line(&mut self, line: &str) -> io::Result<()> {
        let mut stdout = io::stdout();
        let current_cursor = position()?;

        // ANSI Scroll Region boundaries (1-based indexing)
        let scroll_region_top = 1u16;
        let mut scroll_region_bottom = self.viewport_area.y.max(1);

        // Viewport at top edge: scroll terminal to make space
        if scroll_region_bottom <= 1 {
            execute!(stdout, Print("\n"))?;
            self.viewport_area.y = self.viewport_area.y.saturating_add(1);
            scroll_region_bottom = self.viewport_area.y.max(1);
        }

        // Insufficient space: fallback to direct print
        if scroll_region_bottom <= 1 {
            execute!(stdout, Print(line))?;
            execute!(stdout, Print("\r\n"))?;
            execute!(stdout, MoveTo(current_cursor.0, current_cursor.1))?;
            return stdout.flush();
        }

        // Set Scroll Region to protect viewport (ANSI: CSI {top};{bottom} r)
        write!(
            stdout,
            "\x1b[{};{}r",
            scroll_region_top, scroll_region_bottom
        )?;

        // Move to bottom of region
        execute!(stdout, MoveTo(0, scroll_region_bottom - 1))?;

        // Insert new line (scrolls content up within region)
        execute!(stdout, Print("\r\n"))?;
        execute!(stdout, Print(line))?;

        // Reset Scroll Region to full screen (ANSI: CSI r)
        write!(stdout, "\x1b[r")?;

        // Restore cursor
        execute!(stdout, MoveTo(current_cursor.0, current_cursor.1))?;

        stdout.flush()
    }

    pub fn show_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), Show)
    }

    pub fn viewport_start_y(&self) -> u16 {
        self.viewport_area.y
    }
}
