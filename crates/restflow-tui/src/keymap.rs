use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    CloseOverlay,
    OpenSessions,
    OpenRuns,
    OpenTeam,
    OpenHelp,
    Redraw,
    Resize,
    NavUp,
    NavDown,
    MoveLeft,
    MoveRight,
    ScrollUp,
    ScrollDown,
    WheelUp,
    WheelDown,
    InputChar(char),
    Paste(String),
    InputBackspace,
    Newline,
    Submit,
    OverlaySelect,
    DeleteSelected,
    Noop,
}

pub fn map_event(event: Event) -> Action {
    match event {
        Event::Paste(text) => Action::Paste(text),
        Event::Resize(_, _) => Action::Resize,
        Event::Mouse(event) => match event.kind {
            MouseEventKind::ScrollUp => Action::WheelUp,
            MouseEventKind::ScrollDown => Action::WheelDown,
            _ => Action::Noop,
        },
        Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL)
            || modifiers.contains(KeyModifiers::SUPER) =>
        {
            Action::Quit
        }
        Event::Key(KeyEvent {
            code: KeyCode::Esc, ..
        }) => Action::CloseOverlay,
        Event::Key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Action::OpenSessions,
        Event::Key(KeyEvent {
            code: KeyCode::Char('d'),
            modifiers,
            ..
        }) if modifiers.is_empty() => Action::DeleteSelected,
        Event::Key(KeyEvent {
            code: KeyCode::Char('r'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Action::OpenRuns,
        Event::Key(KeyEvent {
            code: KeyCode::Char('g'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Action::OpenTeam,
        Event::Key(KeyEvent {
            code: KeyCode::Char('l'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Action::Redraw,
        Event::Key(KeyEvent {
            code: KeyCode::Char('j'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Action::Newline,
        Event::Key(KeyEvent {
            code: KeyCode::Char('?'),
            modifiers,
            ..
        }) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => Action::OpenHelp,
        Event::Key(KeyEvent {
            code: KeyCode::Up, ..
        }) => Action::NavUp,
        Event::Key(KeyEvent {
            code: KeyCode::Down,
            ..
        }) => Action::NavDown,
        Event::Key(KeyEvent {
            code: KeyCode::Left,
            ..
        }) => Action::MoveLeft,
        Event::Key(KeyEvent {
            code: KeyCode::Right,
            ..
        }) => Action::MoveRight,
        Event::Key(KeyEvent {
            code: KeyCode::PageUp,
            ..
        }) => Action::ScrollUp,
        Event::Key(KeyEvent {
            code: KeyCode::PageDown,
            ..
        }) => Action::ScrollDown,
        Event::Key(KeyEvent {
            code: KeyCode::Backspace,
            ..
        }) => Action::InputBackspace,
        Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::ALT) => Action::OverlaySelect,
        Event::Key(KeyEvent {
            code: KeyCode::Enter,
            ..
        }) => Action::Submit,
        Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers,
            ..
        }) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => Action::InputChar(ch),
        _ => Action::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

    #[test]
    fn maps_ctrl_c_to_quit() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(map_event(event), Action::Quit);
    }

    #[test]
    fn maps_command_c_to_quit() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::SUPER));
        assert_eq!(map_event(event), Action::Quit);
    }

    #[test]
    fn maps_esc_to_close_overlay_without_quit() {
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(map_event(event), Action::CloseOverlay);
    }

    #[test]
    fn maps_ctrl_p_to_open_sessions() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert_eq!(map_event(event), Action::OpenSessions);
    }

    #[test]
    fn maps_d_to_delete_selected() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(map_event(event), Action::DeleteSelected);
    }

    #[test]
    fn maps_ctrl_j_to_newline() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
        assert_eq!(map_event(event), Action::Newline);
    }

    #[test]
    fn maps_paste_event_to_paste_action() {
        assert_eq!(
            map_event(Event::Paste("hello\nworld".to_string())),
            Action::Paste("hello\nworld".to_string())
        );
    }

    #[test]
    fn maps_resize_event_to_resize_action() {
        assert_eq!(map_event(Event::Resize(120, 40)), Action::Resize);
    }

    #[test]
    fn maps_mouse_wheel_to_scroll_actions() {
        let up = Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let down = Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(map_event(up), Action::WheelUp);
        assert_eq!(map_event(down), Action::WheelDown);
    }
}
