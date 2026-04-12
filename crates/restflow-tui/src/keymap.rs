use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    CloseOverlay,
    OpenSessions,
    OpenRuns,
    OpenApprovals,
    OpenTeam,
    OpenHelp,
    Redraw,
    NavUp,
    NavDown,
    MoveLeft,
    MoveRight,
    ScrollUp,
    ScrollDown,
    InputChar(char),
    InputBackspace,
    Newline,
    Submit,
    OverlaySelect,
    RejectSelected,
    Noop,
}

pub fn map_event(event: Event) -> Action {
    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
        Event::Key(KeyEvent {
            code: KeyCode::Esc, ..
        }) => Action::CloseOverlay,
        Event::Key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Action::OpenSessions,
        Event::Key(KeyEvent {
            code: KeyCode::Char('r'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Action::OpenRuns,
        Event::Key(KeyEvent {
            code: KeyCode::Char('a'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Action::OpenApprovals,
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
            code: KeyCode::Char('r'),
            modifiers,
            ..
        }) if modifiers.is_empty() => Action::RejectSelected,
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
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn maps_ctrl_c_to_quit() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(map_event(event), Action::Quit);
    }

    #[test]
    fn maps_ctrl_p_to_open_sessions() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert_eq!(map_event(event), Action::OpenSessions);
    }

    #[test]
    fn maps_ctrl_j_to_newline() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
        assert_eq!(map_event(event), Action::Newline);
    }
}
