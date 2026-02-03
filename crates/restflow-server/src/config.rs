use serde::Deserialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub daemon_url: String,
    pub rate_limit_per_minute: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    server: ServerSection,
    #[serde(default)]
    daemon: DaemonSection,
    #[serde(default)]
    rate_limit: RateLimitSection,
}

#[derive(Debug, Deserialize)]
struct ServerSection {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DaemonSection {
    #[serde(default = "default_daemon_url")]
    url: String,
}

impl Default for DaemonSection {
    fn default() -> Self {
        Self {
            url: default_daemon_url(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RateLimitSection {
    #[serde(default)]
    requests_per_minute: Option<u64>,
}

impl Default for RateLimitSection {
    fn default() -> Self {
        Self {
            requests_per_minute: None,
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_daemon_url() -> String {
    "http://127.0.0.1:3000".to_string()
}

impl ServerConfig {
    pub fn load() -> anyhow::Result<Self> {
        if let Some(file_config) = load_from_file()? {
            return Ok(Self {
                host: file_config.server.host,
                port: file_config.server.port,
                daemon_url: file_config.daemon.url,
                rate_limit_per_minute: file_config.rate_limit.requests_per_minute,
            });
        }

        Ok(Self::from_env())
    }

    fn from_env() -> Self {
        let host = env::var("RESTFLOW_SERVER_HOST").unwrap_or_else(|_| default_host());
        let port = env::var("RESTFLOW_SERVER_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(default_port);
        let daemon_url = env::var("RESTFLOW_DAEMON_URL").unwrap_or_else(|_| default_daemon_url());
        let rate_limit_per_minute = env::var("RESTFLOW_RATE_LIMIT_RPM")
            .ok()
            .and_then(|value| value.parse::<u64>().ok());

        Self {
            host,
            port,
            daemon_url,
            rate_limit_per_minute,
        }
    }
}

fn load_from_file() -> anyhow::Result<Option<FileConfig>> {
    let config_path = env::var("RESTFLOW_SERVER_CONFIG").ok();
    let path = if let Some(path) = config_path {
        Some(path)
    } else if Path::new("server.toml").exists() {
        Some("server.toml".to_string())
    } else {
        None
    };

    let Some(path) = path else {
        return Ok(None);
    };

    let contents = fs::read_to_string(&path)
        .map_err(|err| anyhow::anyhow!("Failed to read config {}: {}", path, err))?;
    let parsed: FileConfig = toml::from_str(&contents)
        .map_err(|err| anyhow::anyhow!("Failed to parse config {}: {}", path, err))?;
    Ok(Some(parsed))
}
