use ratatui::style::{Color, Style};

pub const HEADER_BG: Color = Color::Blue;
pub const HEADER_FG: Color = Color::White;
pub const STATUS_FG: Color = Color::DarkGray;
pub const INPUT_FG: Color = Color::Yellow;
pub const BORDER_FG: Color = Color::DarkGray;

pub fn header_style() -> Style {
    Style::default().bg(HEADER_BG).fg(HEADER_FG)
}

pub fn status_style() -> Style {
    Style::default().fg(STATUS_FG)
}

pub fn input_style() -> Style {
    Style::default().fg(INPUT_FG)
}

pub fn border_style() -> Style {
    Style::default().fg(BORDER_FG)
}
