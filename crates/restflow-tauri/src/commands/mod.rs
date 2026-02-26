//! Tauri command handlers
//!
//! This module contains all Tauri IPC command handlers that bridge
//! the frontend with the RestFlow backend.

pub mod agents;
pub mod auth;
pub mod background_agent;
pub mod chat_sessions;
pub mod config;
pub mod hooks;
pub mod marketplace;
pub mod memory;
pub mod pty;
pub mod secrets;
pub mod skills;
pub mod terminal_sessions;
pub mod voice;

// Re-export all commands for easy registration
pub use agents::*;
pub use auth::*;
pub use background_agent::*;
pub use chat_sessions::*;
pub use config::*;
pub use hooks::*;
pub use marketplace::*;
pub use memory::*;
pub use pty::*;
pub use secrets::*;
pub use skills::*;
pub use terminal_sessions::*;
pub use voice::*;
