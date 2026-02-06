use super::ipc_client;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::PathBuf;

pub struct HealthChecker {
    ipc_socket: PathBuf,
    http_url: Option<String>,
}

impl HealthChecker {
    pub fn new(ipc_socket: PathBuf, http_url: Option<String>) -> Self {
        Self {
            ipc_socket,
            http_url,
        }
    }

    pub async fn check(&self) -> HealthStatus {
        let ipc_ok = self.check_ipc().await;
        let http_ok = self.check_http().await;
        HealthStatus {
            healthy: ipc_ok && http_ok.unwrap_or(true),
            ipc: ipc_ok,
            http: http_ok,
            timestamp: Utc::now(),
        }
    }

    async fn check_ipc(&self) -> bool {
        ipc_client::is_daemon_available(&self.ipc_socket).await
    }

    async fn check_http(&self) -> Option<bool> {
        let url = self.http_url.as_ref()?;
        let client = reqwest::Client::new();
        let response = client.get(format!("{}/health", url)).send().await;
        Some(
            response
                .map(|resp| resp.status().is_success())
                .unwrap_or(false),
        )
    }
}

#[derive(Serialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub ipc: bool,
    pub http: Option<bool>,
    pub timestamp: DateTime<Utc>,
}

pub async fn check_health(ipc_socket: PathBuf, http_url: Option<String>) -> Result<HealthStatus> {
    let checker = HealthChecker::new(ipc_socket, http_url);
    Ok(checker.check().await)
}
