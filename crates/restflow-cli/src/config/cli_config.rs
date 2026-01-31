//! CLI configuration file support
//!
//! Loads configuration from ~/.config/restflow/config.toml

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// CLI configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CliConfig {
    /// Default settings
    #[serde(default)]
    pub default: DefaultConfig,
    /// API key settings
    #[serde(default)]
    pub api_keys: ApiKeysConfig,
}

/// Default configuration values
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DefaultConfig {
    /// Default database path
    pub db_path: Option<String>,
    /// Default model
    pub model: Option<String>,
    /// Default agent ID
    pub agent_id: Option<String>,
}

/// API key configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiKeysConfig {
    /// Anthropic API key
    pub anthropic: Option<String>,
    /// OpenAI API key
    pub openai: Option<String>,
}

impl CliConfig {
    /// Load configuration from default path
    pub fn load() -> Self {
        Self::load_from_path(Self::default_path())
    }

    /// Load configuration from a specific path
    pub fn load_from_path(path: Option<PathBuf>) -> Self {
        let Some(path) = path else {
            return Self::default();
        };

        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Get the default configuration file path
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("restflow").join("config.toml"))
    }

    /// Apply API keys to environment variables
    ///
    /// # Safety
    /// This modifies environment variables which can cause issues in multi-threaded contexts.
    /// Should only be called early in main() before spawning threads.
    pub fn apply_api_key_env(&self) {
        if let Some(key) = &self.api_keys.anthropic {
            if std::env::var("ANTHROPIC_API_KEY").is_err() {
                // SAFETY: Called early in main() before spawning threads
                unsafe { std::env::set_var("ANTHROPIC_API_KEY", key) };
            }
        }
        if let Some(key) = &self.api_keys.openai {
            if std::env::var("OPENAI_API_KEY").is_err() {
                // SAFETY: Called early in main() before spawning threads
                unsafe { std::env::set_var("OPENAI_API_KEY", key) };
            }
        }
    }
}
