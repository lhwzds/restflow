use crate::tui::app::{ChatTui, TuiMode};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

pub async fn handle_key(app: &mut ChatTui, key: KeyEvent) -> Result<bool> {
    match app.mode {
        TuiMode::Chat => handle_chat_key(app, key).await,
        TuiMode::AgentSelect => Ok(handle_agent_select_key(app, key)),
        TuiMode::ModelSelect => Ok(handle_model_select_key(app, key)),
        TuiMode::SessionSelect => Ok(handle_session_select_key(app, key)),
        TuiMode::Help => Ok(handle_help_key(app, key)),
    }
}

async fn handle_chat_key(app: &mut ChatTui, key: KeyEvent) -> Result<bool> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            let now = Instant::now();
            if now.duration_since(app.last_ctrl_c) < Duration::from_secs(1) {
                return Ok(true);
            }
            app.last_ctrl_c = now;
            app.activity_status = "Press Ctrl+C again to exit".to_string();
        }
        (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
            app.mode = TuiMode::AgentSelect;
        }
        (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
            app.mode = TuiMode::ModelSelect;
        }
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
            app.mode = TuiMode::SessionSelect;
        }
        (KeyCode::Char('o'), KeyModifiers::CONTROL) => {
            app.show_tools = !app.show_tools;
        }
        (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
            app.show_thinking = !app.show_thinking;
        }
        (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            app.new_session();
        }
        (KeyCode::Esc, _) => {
            if app.is_streaming {
                app.cancel_streaming();
            }
        }
        (KeyCode::Enter, KeyModifiers::SHIFT) => {
            app.enter_char('\n');
        }
        (KeyCode::Enter, _) => {
            app.submit().await?;
        }
        (KeyCode::Backspace, _) => {
            app.delete_char();
        }
        (KeyCode::Delete, _) => {
            app.move_cursor_right();
            app.delete_char();
        }
        (KeyCode::Left, _) => {
            app.move_cursor_left();
        }
        (KeyCode::Right, _) => {
            app.move_cursor_right();
        }
        (KeyCode::Up, _) => {
            app.history_up();
        }
        (KeyCode::Down, _) => {
            app.history_down();
        }
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            app.enter_char(c);
        }
        _ => {}
    }

    Ok(false)
}

fn handle_agent_select_key(app: &mut ChatTui, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = TuiMode::Chat;
        }
        KeyCode::Up => {
            app.agent_index = app.agent_index.saturating_sub(1);
        }
        KeyCode::Down => {
            if app.agent_index + 1 < app.agents.len() {
                app.agent_index += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(agent) = app.agents.get(app.agent_index) {
                let id = agent.id.clone();
                app.switch_agent(&id);
            }
            app.mode = TuiMode::Chat;
        }
        _ => {}
    }

    false
}

fn handle_model_select_key(app: &mut ChatTui, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = TuiMode::Chat;
        }
        KeyCode::Up => {
            app.model_index = app.model_index.saturating_sub(1);
        }
        KeyCode::Down => {
            if app.model_index + 1 < app.models.len() {
                app.model_index += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(model) = app.models.get(app.model_index) {
                let name = model.clone();
                app.switch_model(&name);
            }
            app.mode = TuiMode::Chat;
        }
        _ => {}
    }

    false
}

fn handle_session_select_key(app: &mut ChatTui, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = TuiMode::Chat;
        }
        KeyCode::Up => {
            app.session_index = app.session_index.saturating_sub(1);
        }
        KeyCode::Down => {
            if app.session_index + 1 < app.sessions.len() {
                app.session_index += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(session) = app.sessions.get(app.session_index) {
                let id = session.id.clone();
                app.load_session(&id);
            }
            app.mode = TuiMode::Chat;
        }
        _ => {}
    }

    false
}

fn handle_help_key(app: &mut ChatTui, key: KeyEvent) -> bool {
    if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
        app.mode = TuiMode::Chat;
    }
    false
}
