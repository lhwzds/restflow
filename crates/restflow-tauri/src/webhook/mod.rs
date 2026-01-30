//! Webhook Module
//!
//! This module provides HTTP webhook functionality for triggering agent tasks
//! from external systems.
//!
//! # Architecture
//!
//! The webhook system consists of:
//! - **Handler**: Processes incoming webhook requests (authentication, rate limiting, task triggering)
//! - **Server**: Embedded HTTP server that can be started/stopped dynamically
//!
//! # Usage
//!
//! ```rust,ignore
//! use restflow_tauri_lib::webhook::{WebhookServerBuilder, WebhookServerConfig};
//!
//! // Start the webhook server
//! let handle = WebhookServerBuilder::new()
//!     .port(8787)
//!     .storage(storage)
//!     .trigger_callback(|task_id, input| {
//!         // Trigger task execution and return run ID
//!         runner.trigger_task(&task_id, input)
//!     })
//!     .start()
//!     .await?;
//!
//! // Stop when done
//! handle.stop().await;
//! ```
//!
//! # Webhook Request
//!
//! External systems can trigger tasks via POST request:
//!
//! ```text
//! POST /hooks/trigger/{task_id}
//! Authorization: Bearer {webhook_token}
//! Content-Type: application/json
//!
//! {
//!     "input": "Optional override input",
//!     "source": "my-system",
//!     "metadata": {"key": "value"}
//! }
//! ```
//!
//! # Security
//!
//! - Each task has a unique webhook token for authentication
//! - Rate limiting prevents abuse (configurable per task)
//! - By default, the server only binds to localhost (127.0.0.1)
//! - For external access, use a reverse proxy with proper security measures

pub mod handler;
pub mod server;

pub use handler::{webhook_router, WebhookState};
pub use server::{
    start_webhook_server, WebhookServerBuilder, WebhookServerConfig, WebhookServerError,
    WebhookServerHandle,
};
