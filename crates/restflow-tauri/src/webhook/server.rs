//! Webhook HTTP Server
//!
//! This module provides an embedded HTTP server for handling webhook requests.
//! The server can be started and stopped dynamically as needed.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{RwLock, oneshot};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info};

use restflow_core::models::WebhookRateLimiter;
use restflow_core::storage::AgentTaskStorage;

/// Type alias for the task trigger callback function
pub type TriggerCallback = Arc<dyn Fn(String, Option<String>) -> String + Send + Sync>;

use super::handler::{WebhookState, webhook_router};

/// Configuration for the webhook server
#[derive(Debug, Clone)]
pub struct WebhookServerConfig {
    /// Port to listen on
    pub port: u16,
    /// Host to bind to (default: 127.0.0.1 for local only)
    pub host: String,
    /// Enable CORS for cross-origin requests
    pub enable_cors: bool,
}

impl Default for WebhookServerConfig {
    fn default() -> Self {
        Self {
            port: 8787,
            host: "127.0.0.1".to_string(),
            enable_cors: false,
        }
    }
}

impl WebhookServerConfig {
    /// Create a config for local-only access
    pub fn local(port: u16) -> Self {
        Self {
            port,
            host: "127.0.0.1".to_string(),
            enable_cors: false,
        }
    }

    /// Create a config for network access (use with caution)
    pub fn network(port: u16) -> Self {
        Self {
            port,
            host: "0.0.0.0".to_string(),
            enable_cors: true,
        }
    }

    /// Get the socket address to bind to
    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("Invalid socket address")
    }
}

/// Handle for controlling the webhook server
pub struct WebhookServerHandle {
    /// Sender to signal shutdown
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Server address
    pub addr: SocketAddr,
    /// Running state
    running: Arc<RwLock<bool>>,
}

impl WebhookServerHandle {
    /// Check if the server is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Stop the webhook server
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            info!("Stopping webhook server");
            let _ = tx.send(());
            *self.running.write().await = false;
        }
    }
}

/// Start the webhook server
pub async fn start_webhook_server(
    config: WebhookServerConfig,
    state: WebhookState,
) -> Result<WebhookServerHandle, std::io::Error> {
    let addr = config.socket_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let running = Arc::new(RwLock::new(true));

    // Build the router
    let mut app = webhook_router(state);

    // Add middleware
    app = app.layer(TraceLayer::new_for_http());

    if config.enable_cors {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    }

    // Start the server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;

    info!(addr = %local_addr, "Starting webhook server");

    let running_clone = Arc::clone(&running);
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
                info!("Webhook server shutting down");
            })
            .await
            .map_err(|e| error!(error = %e, "Webhook server error"))
            .ok();

        *running_clone.write().await = false;
    });

    Ok(WebhookServerHandle {
        shutdown_tx: Some(shutdown_tx),
        addr: local_addr,
        running,
    })
}

/// Builder for creating and configuring a webhook server
pub struct WebhookServerBuilder {
    config: WebhookServerConfig,
    storage: Option<Arc<AgentTaskStorage>>,
    trigger_callback: Option<TriggerCallback>,
}

impl WebhookServerBuilder {
    /// Create a new builder with default config
    pub fn new() -> Self {
        Self {
            config: WebhookServerConfig::default(),
            storage: None,
            trigger_callback: None,
        }
    }

    /// Set the port
    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    /// Set the host
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.config.host = host.into();
        self
    }

    /// Enable CORS
    pub fn enable_cors(mut self) -> Self {
        self.config.enable_cors = true;
        self
    }

    /// Set the storage
    pub fn storage(mut self, storage: Arc<AgentTaskStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Set the trigger callback
    pub fn trigger_callback(
        mut self,
        callback: impl Fn(String, Option<String>) -> String + Send + Sync + 'static,
    ) -> Self {
        self.trigger_callback = Some(Arc::new(callback));
        self
    }

    /// Build and start the server
    pub async fn start(self) -> Result<WebhookServerHandle, WebhookServerError> {
        let storage = self.storage.ok_or(WebhookServerError::MissingStorage)?;
        let trigger_callback = self
            .trigger_callback
            .ok_or(WebhookServerError::MissingCallback)?;

        let state = WebhookState {
            storage,
            rate_limiter: Arc::new(RwLock::new(WebhookRateLimiter::new())),
            trigger_callback,
        };

        start_webhook_server(self.config, state)
            .await
            .map_err(WebhookServerError::IoError)
    }
}

impl Default for WebhookServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur when starting the webhook server
#[derive(Debug, thiserror::Error)]
pub enum WebhookServerError {
    #[error("Storage not configured")]
    MissingStorage,
    #[error("Trigger callback not configured")]
    MissingCallback,
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_storage() -> (Arc<AgentTaskStorage>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = std::sync::Arc::new(redb::Database::create(db_path).unwrap());
        (Arc::new(AgentTaskStorage::new(db).unwrap()), temp_dir)
    }

    #[test]
    fn test_config_default() {
        let config = WebhookServerConfig::default();
        assert_eq!(config.port, 8787);
        assert_eq!(config.host, "127.0.0.1");
        assert!(!config.enable_cors);
    }

    #[test]
    fn test_config_local() {
        let config = WebhookServerConfig::local(9999);
        assert_eq!(config.port, 9999);
        assert_eq!(config.host, "127.0.0.1");
    }

    #[test]
    fn test_config_network() {
        let config = WebhookServerConfig::network(8080);
        assert_eq!(config.port, 8080);
        assert_eq!(config.host, "0.0.0.0");
        assert!(config.enable_cors);
    }

    #[tokio::test]
    async fn test_server_start_stop() {
        let (storage, _tmp) = create_test_storage();
        let state = WebhookState::new(storage, |task_id, _| format!("run-{}", task_id));

        // Use port 0 to get a random available port
        let config = WebhookServerConfig {
            port: 0,
            host: "127.0.0.1".to_string(),
            enable_cors: false,
        };

        let mut handle = start_webhook_server(config, state).await.unwrap();
        assert!(handle.is_running().await);

        handle.stop().await;
        // Give the server time to shut down
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert!(!handle.is_running().await);
    }

    #[tokio::test]
    async fn test_builder() {
        let (storage, _tmp) = create_test_storage();

        let mut handle = WebhookServerBuilder::new()
            .port(0) // Random port
            .storage(storage)
            .trigger_callback(|task_id, _| format!("run-{}", task_id))
            .start()
            .await
            .unwrap();

        assert!(handle.is_running().await);
        handle.stop().await;
    }

    #[tokio::test]
    async fn test_builder_missing_storage() {
        let result = WebhookServerBuilder::new()
            .trigger_callback(|task_id, _| format!("run-{}", task_id))
            .start()
            .await;

        assert!(matches!(result, Err(WebhookServerError::MissingStorage)));
    }

    #[tokio::test]
    async fn test_builder_missing_callback() {
        let (storage, _tmp) = create_test_storage();

        let result = WebhookServerBuilder::new().storage(storage).start().await;

        assert!(matches!(result, Err(WebhookServerError::MissingCallback)));
    }
}
