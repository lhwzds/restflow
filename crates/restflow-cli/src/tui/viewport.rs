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

        // Capture current cursor as the viewport origin
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

        // Set viewport area starting from the specified position
        self.viewport_area = Rect::new(0, start_y, term_width, height);

        // If viewport would overflow bottom of screen, scroll entire terminal
        if self.viewport_area.bottom() > term_height {
            let scroll_amount = self.viewport_area.bottom() - term_height;
            // Use println to scroll the terminal
            for _ in 0..scroll_amount {
                println!();
            }
            // Adjust viewport position after scrolling
            self.viewport_area.y = term_height.saturating_sub(height);
        }

        Ok(())
    }

    /// Resize viewport height without changing Y position
    pub fn adjust_viewport_height(&mut self, new_height: u16) -> io::Result<()> {
        let (term_width, term_height) = size()?;

        let desired_height = new_height.min(term_height);

        self.viewport_area.width = term_width;
        self.viewport_area.height = desired_height;

        if self.viewport_area.bottom() > term_height {
            let scroll_amount = self.viewport_area.bottom() - term_height;
            for _ in 0..scroll_amount {
                println!();
            }
            self.viewport_area.y = term_height.saturating_sub(desired_height);
        }

        Ok(())
    }

    /// Legacy method for backward compatibility
    #[allow(dead_code)]
    pub fn setup_viewport(&mut self, height: u16) -> io::Result<()> {
        let (_, cursor_y) = position()?;
        self.setup_viewport_from(cursor_y, height)
    }

    /// Insert line above viewport using ANSI Scroll Region
    pub fn insert_history_line(&mut self, line: &str) -> io::Result<()> {
        let mut stdout = io::stdout();
        let current_cursor = position()?;

        // Calculate Scroll Region boundaries (1-based indexing for ANSI)
        let scroll_region_top = 1u16;
        let mut scroll_region_bottom = self.viewport_area.y.max(1);

        // If the viewport is at the very top, scroll the entire terminal once
        if scroll_region_bottom <= 1 {
            execute!(stdout, Print("\n"))?;
            self.viewport_area.y = self.viewport_area.y.saturating_add(1);
            scroll_region_bottom = self.viewport_area.y.max(1);
        }

        // If we still do not have space for a scroll region, fall back to direct printing
        if scroll_region_bottom <= 1 {
            execute!(stdout, Print(line))?;
            execute!(stdout, Print("\r\n"))?;
            execute!(stdout, MoveTo(current_cursor.0, current_cursor.1))?;
            return stdout.flush();
        }

        // Set Scroll Region to protect the viewport
        // ANSI escape code: CSI {top};{bottom} r
        write!(
            stdout,
            "\x1b[{};{}r",
            scroll_region_top, scroll_region_bottom
        )?;

        // Move to the bottom of the Scroll Region
        execute!(stdout, MoveTo(0, scroll_region_bottom - 1))?;

        // Insert new line (this will scroll up within the region)
        execute!(stdout, Print("\r\n"))?;
        execute!(stdout, Print(line))?;

        // Reset Scroll Region to full screen
        // ANSI escape code: CSI r (no parameters)
        write!(stdout, "\x1b[r")?;

        // Restore cursor to viewport
        execute!(stdout, MoveTo(current_cursor.0, current_cursor.1))?;

        stdout.flush()
    }

    pub fn show_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), Show)
    }

    /// Get viewport start y coordinate (terminal absolute)
    pub fn viewport_start_y(&self) -> u16 {
        self.viewport_area.y
    }
}
