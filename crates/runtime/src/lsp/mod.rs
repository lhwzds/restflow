//! Language Server Protocol integration.

mod client;
mod manager;
mod protocol;
mod watcher;

pub use client::{LspClient, LspClientConfig};
pub use manager::{LanguageId, LspManager};
pub use watcher::LspWatcher;
