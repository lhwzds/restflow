use crate::AppCore;
use anyhow::Result;
use axum::Router;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

use super::router;

/// HTTP server configuration
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Host to bind to (default: 127.0.0.1)
    pub host: String,
    /// Port to listen on (default: 3000)
    pub port: u16,
    /// CORS allowed origins
    pub cors_origins: Vec<String>,
    /// Enable API key authentication
    pub auth_enabled: bool,
    /// API key for authentication (if enabled)
    pub api_key: Option<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        let host = std::env::var("RESTFLOW_HTTP_HOST")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "127.0.0.1".to_string());

        Self {
            host,
            port: 3000,
            cors_origins: vec![
                "http://localhost:5173".to_string(),
                "tauri://localhost".to_string(),
            ],
            auth_enabled: false,
            api_key: None,
        }
    }
}

/// HTTP server for the daemon
pub struct HttpServer {
    config: HttpConfig,
    core: Arc<AppCore>,
}

impl HttpServer {
    pub fn new(config: HttpConfig, core: Arc<AppCore>) -> Self {
        Self { config, core }
    }

    /// Build the router with all API routes
    fn build_router(&self) -> Router {
        router::build_router(self.core.clone(), &self.config)
    }

    /// Run the HTTP server
    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        let app = self.build_router();
        let addr = format!("{}:{}", self.config.host, self.config.port);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        info!("HTTP server listening on {}", addr);

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown.recv().await;
                info!("HTTP server shutting down");
            })
            .await?;

        Ok(())
    }
}
