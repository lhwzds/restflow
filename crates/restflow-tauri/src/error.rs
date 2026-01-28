//! Error types for Tauri commands

use serde::Serialize;
use thiserror::Error;

/// Tauri command error type
#[derive(Debug, Error)]
pub enum TauriError {
    #[error("Storage error: {0}")]
    Storage(#[from] anyhow::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Execution error: {0}")]
    Execution(String),
}

/// Serializable error for Tauri frontend
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<TauriError> for CommandError {
    fn from(err: TauriError) -> Self {
        let (code, message) = match &err {
            TauriError::Storage(_) => ("STORAGE_ERROR", err.to_string()),
            TauriError::NotFound(_) => ("NOT_FOUND", err.to_string()),
            TauriError::InvalidInput(_) => ("INVALID_INPUT", err.to_string()),
            TauriError::Execution(_) => ("EXECUTION_ERROR", err.to_string()),
        };
        Self {
            code: code.to_string(),
            message,
        }
    }
}

// Implement IntoResponse for Tauri commands
impl From<TauriError> for String {
    fn from(err: TauriError) -> Self {
        err.to_string()
    }
}
