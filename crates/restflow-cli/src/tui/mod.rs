mod app;
mod daemon_client;
mod event_loop;
mod keymap;
mod render;
mod state;

pub use app::run_chat_tui;

#[derive(Debug, Clone, Default)]
pub struct ChatLaunchOptions {
    pub agent: Option<String>,
    pub session: Option<String>,
    pub message: Option<String>,
}
