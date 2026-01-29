//! Webhook Configuration and Types
//!
//! This module defines the types for webhook triggers, allowing external systems
//! (GitHub, CI/CD, monitoring tools) to trigger agent tasks via HTTP.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Webhook configuration for a task
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WebhookConfig {
    /// Whether webhook trigger is enabled
    pub enabled: bool,
    /// Unique webhook token for authentication
    pub token: String,
    /// Optional allowed IP addresses/ranges
    #[serde(default)]
    pub allowed_ips: Option<Vec<String>>,
    /// Optional rate limit (requests per minute)
    #[serde(default)]
    pub rate_limit: Option<u32>,
    /// Whether to require signature verification
    #[serde(default)]
    pub require_signature: bool,
    /// Secret for signature verification (HMAC-SHA256)
    #[serde(default)]
    pub signature_secret: Option<String>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: uuid::Uuid::new_v4().to_string(),
            allowed_ips: None,
            rate_limit: Some(60), // 60 requests per minute
            require_signature: false,
            signature_secret: None,
        }
    }
}

impl WebhookConfig {
    /// Create a new enabled webhook config with a fresh token
    pub fn new_enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// Create a new webhook config with signature verification
    pub fn with_signature(secret: String) -> Self {
        Self {
            enabled: true,
            require_signature: true,
            signature_secret: Some(secret),
            ..Default::default()
        }
    }

    /// Generate the webhook URL path for this config
    pub fn webhook_path(&self, task_id: &str) -> String {
        format!("/api/hooks/tasks/{}/trigger", task_id)
    }

    /// Validate an incoming request token
    pub fn validate_token(&self, token: &str) -> bool {
        self.token == token
    }

    /// Check if an IP is allowed (if allowlist is configured)
    pub fn is_ip_allowed(&self, ip: &str) -> bool {
        match &self.allowed_ips {
            Some(allowed) => allowed.iter().any(|allowed_ip| {
                // Simple exact match for now
                // TODO: Add CIDR range support
                allowed_ip == ip || allowed_ip == "*"
            }),
            None => true, // No allowlist = all IPs allowed
        }
    }
}

/// Incoming webhook request payload
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WebhookRequest {
    /// Task ID to trigger (optional if using task-specific endpoint)
    #[serde(default)]
    pub task_id: Option<String>,
    /// Input to pass to the task (overrides default input)
    #[serde(default)]
    pub input: Option<String>,
    /// Optional metadata from the webhook source
    #[serde(default)]
    #[ts(type = "Record<string, unknown> | undefined")]
    pub metadata: Option<serde_json::Value>,
    /// Source identifier (e.g., "github", "gitlab", "custom")
    #[serde(default)]
    pub source: Option<String>,
}

impl WebhookRequest {
    /// Create a simple webhook request with just an input
    pub fn with_input(input: String) -> Self {
        Self {
            task_id: None,
            input: Some(input),
            metadata: None,
            source: None,
        }
    }

    /// Create a webhook request for a specific task
    pub fn for_task(task_id: String, input: Option<String>) -> Self {
        Self {
            task_id: Some(task_id),
            input,
            metadata: None,
            source: None,
        }
    }
}

/// Webhook trigger response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WebhookResponse {
    /// Whether the trigger was accepted
    pub accepted: bool,
    /// Run ID if task was triggered
    #[serde(default)]
    pub run_id: Option<String>,
    /// Task ID that was triggered
    #[serde(default)]
    pub task_id: Option<String>,
    /// Error message if not accepted
    #[serde(default)]
    pub error: Option<String>,
    /// Timestamp of the response
    pub timestamp: i64,
}

impl WebhookResponse {
    /// Create a successful response
    pub fn success(run_id: String, task_id: String) -> Self {
        Self {
            accepted: true,
            run_id: Some(run_id),
            task_id: Some(task_id),
            error: None,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Create an error response
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            accepted: false,
            run_id: None,
            task_id: None,
            error: Some(message.into()),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// Webhook event log entry for auditing
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WebhookEvent {
    /// Unique event ID
    pub id: String,
    /// Task ID that was triggered
    pub task_id: String,
    /// Timestamp of the event
    pub timestamp: i64,
    /// Source IP address of the request
    pub source_ip: String,
    /// Source identifier from request
    #[serde(default)]
    pub source: Option<String>,
    /// Whether the trigger was accepted
    pub accepted: bool,
    /// Run ID if task was triggered
    #[serde(default)]
    pub run_id: Option<String>,
    /// Error message if not accepted
    #[serde(default)]
    pub error: Option<String>,
    /// HTTP headers (for debugging)
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

impl WebhookEvent {
    /// Create a new webhook event
    pub fn new(task_id: String, source_ip: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            timestamp: chrono::Utc::now().timestamp(),
            source_ip,
            source: None,
            accepted: false,
            run_id: None,
            error: None,
            headers: None,
        }
    }

    /// Mark the event as successful
    pub fn with_success(mut self, run_id: String) -> Self {
        self.accepted = true;
        self.run_id = Some(run_id);
        self
    }

    /// Mark the event as failed
    pub fn with_error(mut self, error: String) -> Self {
        self.accepted = false;
        self.error = Some(error);
        self
    }

    /// Add source information
    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_config_default() {
        let config = WebhookConfig::default();
        assert!(!config.enabled);
        assert!(!config.token.is_empty());
        assert_eq!(config.rate_limit, Some(60));
        assert!(!config.require_signature);
    }

    #[test]
    fn test_webhook_config_new_enabled() {
        let config = WebhookConfig::new_enabled();
        assert!(config.enabled);
        assert!(!config.token.is_empty());
    }

    #[test]
    fn test_webhook_config_with_signature() {
        let secret = "my-secret".to_string();
        let config = WebhookConfig::with_signature(secret.clone());
        assert!(config.enabled);
        assert!(config.require_signature);
        assert_eq!(config.signature_secret, Some(secret));
    }

    #[test]
    fn test_webhook_path() {
        let config = WebhookConfig::default();
        let path = config.webhook_path("task-123");
        assert_eq!(path, "/api/hooks/tasks/task-123/trigger");
    }

    #[test]
    fn test_validate_token() {
        let config = WebhookConfig::default();
        assert!(config.validate_token(&config.token));
        assert!(!config.validate_token("wrong-token"));
    }

    #[test]
    fn test_ip_allowlist() {
        let mut config = WebhookConfig::default();
        
        // No allowlist = all IPs allowed
        assert!(config.is_ip_allowed("192.168.1.1"));
        
        // With allowlist
        config.allowed_ips = Some(vec!["192.168.1.1".to_string(), "10.0.0.1".to_string()]);
        assert!(config.is_ip_allowed("192.168.1.1"));
        assert!(config.is_ip_allowed("10.0.0.1"));
        assert!(!config.is_ip_allowed("8.8.8.8"));
        
        // Wildcard
        config.allowed_ips = Some(vec!["*".to_string()]);
        assert!(config.is_ip_allowed("any.ip.here"));
    }

    #[test]
    fn test_webhook_request_with_input() {
        let request = WebhookRequest::with_input("test input".to_string());
        assert_eq!(request.input, Some("test input".to_string()));
        assert!(request.task_id.is_none());
    }

    #[test]
    fn test_webhook_request_for_task() {
        let request = WebhookRequest::for_task("task-123".to_string(), Some("input".to_string()));
        assert_eq!(request.task_id, Some("task-123".to_string()));
        assert_eq!(request.input, Some("input".to_string()));
    }

    #[test]
    fn test_webhook_response_success() {
        let response = WebhookResponse::success("run-123".to_string(), "task-456".to_string());
        assert!(response.accepted);
        assert_eq!(response.run_id, Some("run-123".to_string()));
        assert_eq!(response.task_id, Some("task-456".to_string()));
        assert!(response.error.is_none());
    }

    #[test]
    fn test_webhook_response_error() {
        let response = WebhookResponse::error("Invalid token");
        assert!(!response.accepted);
        assert!(response.run_id.is_none());
        assert_eq!(response.error, Some("Invalid token".to_string()));
    }

    #[test]
    fn test_webhook_event_lifecycle() {
        let event = WebhookEvent::new("task-123".to_string(), "192.168.1.1".to_string())
            .with_source("github".to_string())
            .with_success("run-456".to_string());
        
        assert_eq!(event.task_id, "task-123");
        assert_eq!(event.source_ip, "192.168.1.1");
        assert_eq!(event.source, Some("github".to_string()));
        assert!(event.accepted);
        assert_eq!(event.run_id, Some("run-456".to_string()));
    }

    #[test]
    fn test_webhook_event_error() {
        let event = WebhookEvent::new("task-123".to_string(), "192.168.1.1".to_string())
            .with_error("Rate limit exceeded".to_string());
        
        assert!(!event.accepted);
        assert_eq!(event.error, Some("Rate limit exceeded".to_string()));
    }
}
