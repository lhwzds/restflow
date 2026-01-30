//! Tauri command handlers
//!
//! This module contains all Tauri IPC command handlers that bridge
//! the frontend with the RestFlow backend.

pub mod agent_task;
pub mod agents;
pub mod config;
pub mod pty;
pub mod secrets;
pub mod security;
pub mod shell;
pub mod skills;
pub mod terminal_sessions;

// Re-export all commands for easy registration
pub use agent_task::*;
pub use agents::*;
pub use config::*;
pub use pty::*;
pub use secrets::*;
pub use security::*;
pub use shell::*;
pub use skills::*;
pub use terminal_sessions::*;
