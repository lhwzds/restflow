//! Tauri command handlers
//!
//! This module contains all Tauri IPC command handlers that bridge
//! the frontend with the RestFlow backend.

pub mod agents;
pub mod config;
pub mod secrets;
pub mod skills;

// Re-export all commands for easy registration
pub use agents::*;
pub use config::*;
pub use secrets::*;
pub use skills::*;
