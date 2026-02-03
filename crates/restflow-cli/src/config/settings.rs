use restflow_core::paths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CliConfig {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub default: DefaultConfig,
    #[serde(default)]
    pub sandbox: SandboxConfig,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            version: 1,
            default: DefaultConfig::default(),
            sandbox: SandboxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DefaultConfig {
    pub agent: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SandboxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub env: EnvSandboxConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct EnvSandboxConfig {
    #[serde(default)]
    pub isolate: bool,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub block: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LimitsConfig {
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_output")]
    pub max_output_bytes: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_timeout(),
            max_output_bytes: default_max_output(),
        }
    }
}

impl CliConfig {
    pub fn load() -> Self {
        let json_path = Self::config_path();
        if json_path.exists() {
            match std::fs::read_to_string(&json_path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(config) => return config,
                    Err(err) => {
                        eprintln!("Warning: Failed to parse config.json: {err}");
                    }
                },
                Err(err) => {
                    eprintln!("Warning: Failed to read config.json: {err}");
                }
            }
        }

        if let Some(config) = Self::migrate_from_toml() {
            let _ = config.save();
            return config;
        }

        Self::default()
    }

    #[allow(dead_code)]
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn config_path() -> PathBuf {
        paths::config_path().unwrap_or_else(|_| PathBuf::from("config.json"))
    }

    fn migrate_from_toml() -> Option<Self> {
        let old_path = dirs::config_dir()?.join("restflow").join("config.toml");
        if !old_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&old_path).ok()?;

        #[derive(Deserialize)]
        struct OldConfig {
            default: Option<OldDefaultConfig>,
            api_keys: Option<OldApiKeys>,
        }

        #[allow(dead_code)]
        #[derive(Deserialize)]
        struct OldDefaultConfig {
            agent: Option<String>,
            model: Option<String>,
            db_path: Option<String>,
        }

        #[allow(dead_code)]
        #[derive(Deserialize)]
        struct OldApiKeys {
            anthropic: Option<String>,
            openai: Option<String>,
            deepseek: Option<String>,
        }

        let old: OldConfig = toml::from_str(&content).ok()?;

        if old.api_keys.is_some() {
            eprintln!(
                "Warning: API keys in config.toml should be migrated to `restflow secret set`"
            );
        }

        Some(Self {
            version: 1,
            default: DefaultConfig {
                agent: old.default.as_ref().and_then(|d| d.agent.clone()),
                model: old.default.as_ref().and_then(|d| d.model.clone()),
            },
            sandbox: SandboxConfig::default(),
        })
    }
}

fn default_timeout() -> u64 {
    120
}

fn default_max_output() -> usize {
    1_048_576
}
