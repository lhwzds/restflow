use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CliConfig {
    #[serde(default)]
    pub default: DefaultConfig,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub api_keys: ApiKeysConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DefaultConfig {
    pub agent: Option<String>,
    pub model: Option<String>,
    pub db_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TuiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_true")]
    pub show_tokens: bool,
    #[serde(default = "default_true")]
    pub show_tools: bool,
    #[serde(default = "default_true")]
    pub syntax_highlight: bool,
    #[serde(default = "default_history_size")]
    pub history_size: usize,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            show_tokens: default_true(),
            show_tools: default_true(),
            syntax_highlight: default_true(),
            history_size: default_history_size(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ApiKeysConfig {
    pub anthropic: Option<String>,
    pub openai: Option<String>,
    pub deepseek: Option<String>,
}

impl CliConfig {
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => config,
                Err(err) => {
                    eprintln!("Warning: Failed to parse config: {err}");
                    Self::default()
                }
            },
            Err(err) => {
                eprintln!("Warning: Failed to read config: {err}");
                Self::default()
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("restflow")
            .join("config.toml")
    }

    pub fn apply_api_key_env(&self) {
        set_env_if_missing("ANTHROPIC_API_KEY", &self.api_keys.anthropic);
        set_env_if_missing("OPENAI_API_KEY", &self.api_keys.openai);
        set_env_if_missing("DEEPSEEK_API_KEY", &self.api_keys.deepseek);
    }
}

fn set_env_if_missing(key: &str, value: &Option<String>) {
    if std::env::var_os(key).is_none() {
        if let Some(val) = value.as_ref().map(|v| v.trim()).filter(|v| !v.is_empty()) {
            // SAFETY: We are setting environment variables during program initialization,
            // before spawning any threads, so this is safe.
            unsafe {
                std::env::set_var(key, val);
            }
        }
    }
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_true() -> bool {
    true
}

fn default_history_size() -> usize {
    1000
}
