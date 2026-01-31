//! Webhook Configuration and Types
//!
//! This module provides types for configuring webhook triggers on agent tasks.
//! Webhooks allow external systems to trigger task executions via HTTP.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Webhook configuration for a task
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct WebhookConfig {
    /// Whether webhook trigger is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Unique webhook token for authentication
    pub token: String,
    /// Optional rate limit (requests per minute)
    #[serde(default)]
    pub rate_limit: Option<u32>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: uuid::Uuid::new_v4().to_string(),
            rate_limit: Some(60),
        }
    }
}

impl WebhookConfig {
    /// Create a new disabled webhook config with a fresh token
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new enabled webhook config
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            token: uuid::Uuid::new_v4().to_string(),
            rate_limit: Some(60),
        }
    }

    /// Create a webhook config with a specific token
    pub fn with_token(token: String) -> Self {
        Self {
            enabled: true,
            token,
            rate_limit: Some(60),
        }
    }

    /// Regenerate the webhook token
    pub fn regenerate_token(&mut self) {
        self.token = uuid::Uuid::new_v4().to_string();
    }

    /// Get the full webhook URL for a task
    pub fn webhook_url(&self, base_url: &str, task_id: &str) -> String {
        format!(
            "{}/hooks/trigger/{}",
            base_url.trim_end_matches('/'),
            task_id
        )
    }
}

/// Incoming webhook request payload
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebhookRequest {
    /// Optional input to pass to the task (overrides task's default input)
    #[serde(default)]
    pub input: Option<String>,
    /// Source identifier for tracking where the webhook was called from
    #[serde(default)]
    pub source: Option<String>,
    /// Optional metadata for logging/debugging
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Webhook trigger response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WebhookResponse {
    /// Whether the webhook was accepted
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
    /// Timestamp when the request was processed
    #[serde(default)]
    #[ts(type = "number | null")]
    pub timestamp: Option<i64>,
}

impl WebhookResponse {
    /// Create a success response
    pub fn success(task_id: String, run_id: String) -> Self {
        Self {
            accepted: true,
            run_id: Some(run_id),
            task_id: Some(task_id),
            error: None,
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
        }
    }

    /// Create an error response
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            accepted: false,
            run_id: None,
            task_id: None,
            error: Some(message.into()),
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
        }
    }
}

/// Rate limiter state for webhook endpoints
#[derive(Debug, Clone, Default)]
pub struct WebhookRateLimiter {
    /// Request counts per task ID (task_id -> (count, window_start_ms))
    requests: std::collections::HashMap<String, (u32, i64)>,
}

impl WebhookRateLimiter {
    /// Create a new rate limiter
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a request is allowed and record it
    pub fn check_and_record(&mut self, task_id: &str, limit: u32) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        let window_ms: i64 = 60_000; // 1 minute window

        let entry = self.requests.entry(task_id.to_string()).or_insert((0, now));

        // Reset window if expired
        if now - entry.1 >= window_ms {
            entry.0 = 0;
            entry.1 = now;
        }

        // Check limit
        if entry.0 >= limit {
            return false;
        }

        // Record request
        entry.0 += 1;
        true
    }

    /// Clean up expired entries
    pub fn cleanup(&mut self) {
        let now = chrono::Utc::now().timestamp_millis();
        let window_ms: i64 = 60_000;

        self.requests.retain(|_, (_, window_start)| {
            now - *window_start < window_ms * 2 // Keep for 2 windows
        });
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
    }

    #[test]
    fn test_webhook_config_enabled() {
        let config = WebhookConfig::enabled();
        assert!(config.enabled);
        assert!(!config.token.is_empty());
    }

    #[test]
    fn test_webhook_url() {
        let config = WebhookConfig::enabled();
        let url = config.webhook_url("http://localhost:8080", "task-123");
        assert_eq!(url, "http://localhost:8080/hooks/trigger/task-123");

        // Test with trailing slash
        let url = config.webhook_url("http://localhost:8080/", "task-456");
        assert_eq!(url, "http://localhost:8080/hooks/trigger/task-456");
    }

    #[test]
    fn test_regenerate_token() {
        let mut config = WebhookConfig::default();
        let original_token = config.token.clone();
        config.regenerate_token();
        assert_ne!(config.token, original_token);
    }

    #[test]
    fn test_webhook_response_success() {
        let response = WebhookResponse::success("task-1".to_string(), "run-1".to_string());
        assert!(response.accepted);
        assert_eq!(response.task_id, Some("task-1".to_string()));
        assert_eq!(response.run_id, Some("run-1".to_string()));
        assert!(response.error.is_none());
    }

    #[test]
    fn test_webhook_response_error() {
        let response = WebhookResponse::error("Not found");
        assert!(!response.accepted);
        assert!(response.task_id.is_none());
        assert!(response.run_id.is_none());
        assert_eq!(response.error, Some("Not found".to_string()));
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = WebhookRateLimiter::new();
        let task_id = "task-1";
        let limit = 3;

        // First 3 requests should succeed
        assert!(limiter.check_and_record(task_id, limit));
        assert!(limiter.check_and_record(task_id, limit));
        assert!(limiter.check_and_record(task_id, limit));

        // 4th request should be denied
        assert!(!limiter.check_and_record(task_id, limit));
    }

    #[test]
    fn test_rate_limiter_different_tasks() {
        let mut limiter = WebhookRateLimiter::new();

        // Different tasks should have independent limits
        assert!(limiter.check_and_record("task-1", 1));
        assert!(!limiter.check_and_record("task-1", 1));

        assert!(limiter.check_and_record("task-2", 1));
        assert!(!limiter.check_and_record("task-2", 1));
    }

    #[test]
    fn test_webhook_request_default() {
        let request = WebhookRequest::default();
        assert!(request.input.is_none());
        assert!(request.source.is_none());
        assert!(request.metadata.is_none());
    }
}
