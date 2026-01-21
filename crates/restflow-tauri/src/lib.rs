//! RestFlow Tauri Desktop Application
//!
//! This crate provides the Tauri desktop application wrapper for RestFlow,
//! exposing the workflow engine functionality through Tauri commands.

pub mod commands;
pub mod error;
pub mod state;

pub use error::TauriError;
pub use state::AppState;
