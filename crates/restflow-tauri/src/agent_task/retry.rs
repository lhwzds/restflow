//! Retry Manager for Failed Tasks
//!
//! This module provides a retry mechanism for agent tasks that fail due to
//! transient errors (e.g., network timeouts, rate limits, temporary service
//! unavailability).
//!
//! # Features
//!
//! - Configurable maximum retry attempts
//! - Exponential backoff with jitter
//! - Transient error detection
//! - Per-task retry state tracking
//!
//! # Example
//!
//! ```ignore
//! use restflow_tauri::agent_task::retry::{RetryConfig, RetryState};
//!
//! let config = RetryConfig::default();
//! let mut state = RetryState::new();
//!
//! // After a failure
//! if state.should_retry(&config, "Connection timeout") {
//!     state.record_failure("Connection timeout", &config);
//!     // Wait for state.next_retry_at before retrying
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for the retry mechanism
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (0 = no retries)
    pub max_retries: u32,
    /// Initial delay between retries in seconds
    pub initial_delay_secs: u64,
    /// Maximum delay between retries in seconds (caps exponential growth)
    pub max_delay_secs: u64,
    /// Multiplier for exponential backoff (e.g., 2.0 = double each time)
    pub backoff_multiplier: f64,
    /// Whether to add random jitter to delays (recommended for distributed systems)
    pub jitter_enabled: bool,
    /// Maximum jitter as a fraction of delay (e.g., 0.25 = up to 25% variation)
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_secs: 60,  // 1 minute
            max_delay_secs: 3600,    // 1 hour
            backoff_multiplier: 2.0, // Double each time
            jitter_enabled: true,
            jitter_factor: 0.25, // Up to 25% variation
        }
    }
}

impl RetryConfig {
    /// Create a new configuration with custom settings
    pub fn new(max_retries: u32, initial_delay_secs: u64) -> Self {
        Self {
            max_retries,
            initial_delay_secs,
            ..Default::default()
        }
    }

    /// Create a configuration with no retries
    pub fn no_retries() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Create an aggressive retry configuration (for critical tasks)
    pub fn aggressive() -> Self {
        Self {
            max_retries: 5,
            initial_delay_secs: 30,
            max_delay_secs: 1800, // 30 minutes
            backoff_multiplier: 1.5,
            jitter_enabled: true,
            jitter_factor: 0.2,
        }
    }

    /// Create a conservative retry configuration (for less critical tasks)
    pub fn conservative() -> Self {
        Self {
            max_retries: 2,
            initial_delay_secs: 120, // 2 minutes
            max_delay_secs: 7200,    // 2 hours
            backoff_multiplier: 3.0,
            jitter_enabled: true,
            jitter_factor: 0.3,
        }
    }
}

/// State for tracking retry attempts for a task
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetryState {
    /// Current retry attempt number (0 = initial attempt, 1 = first retry, etc.)
    pub attempt: u32,
    /// Error message from the last failure
    pub last_error: Option<String>,
    /// Timestamp (milliseconds since epoch) for when the next retry should occur
    pub next_retry_at: Option<i64>,
    /// Timestamp of the last failure
    pub last_failure_at: Option<i64>,
    /// Total number of failures (including non-retryable ones)
    pub total_failures: u32,
}

impl RetryState {
    /// Create a new retry state
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if we should retry based on the config and error
    ///
    /// Returns true if:
    /// - We haven't exceeded max_retries
    /// - The error is transient (retryable)
    pub fn should_retry(&self, config: &RetryConfig, error: &str) -> bool {
        if self.attempt >= config.max_retries {
            return false;
        }
        is_transient_error(error)
    }

    /// Calculate the delay before the next retry attempt
    ///
    /// Uses exponential backoff with optional jitter
    pub fn calculate_delay(&self, config: &RetryConfig) -> Duration {
        // Base delay with exponential backoff
        let base_delay =
            config.initial_delay_secs as f64 * config.backoff_multiplier.powi(self.attempt as i32);

        // Cap at maximum delay
        let capped_delay = base_delay.min(config.max_delay_secs as f64);

        // Add jitter if enabled
        let final_delay = if config.jitter_enabled {
            let jitter_range = capped_delay * config.jitter_factor;
            // Simple deterministic jitter based on attempt number
            // In production, you might want to use actual random jitter
            let jitter = jitter_range * ((self.attempt as f64 * 0.37).sin().abs());
            capped_delay + jitter
        } else {
            capped_delay
        };

        Duration::from_secs(final_delay as u64)
    }

    /// Record a failure and update retry state
    ///
    /// Increments the attempt counter and calculates the next retry time
    pub fn record_failure(&mut self, error: &str, config: &RetryConfig) {
        let now = chrono::Utc::now().timestamp_millis();

        self.attempt += 1;
        self.total_failures += 1;
        self.last_error = Some(error.to_string());
        self.last_failure_at = Some(now);

        // Calculate next retry time if we haven't exceeded max retries
        if self.attempt < config.max_retries && is_transient_error(error) {
            let delay = self.calculate_delay(config);
            self.next_retry_at = Some(now + delay.as_millis() as i64);
        } else {
            self.next_retry_at = None;
        }
    }

    /// Check if a retry is due (current time >= next_retry_at)
    pub fn is_retry_due(&self) -> bool {
        match self.next_retry_at {
            Some(retry_at) => {
                let now = chrono::Utc::now().timestamp_millis();
                now >= retry_at
            }
            None => false,
        }
    }

    /// Get the remaining time until the next retry in milliseconds
    pub fn time_until_retry(&self) -> Option<i64> {
        self.next_retry_at.map(|retry_at| {
            let now = chrono::Utc::now().timestamp_millis();
            (retry_at - now).max(0)
        })
    }

    /// Reset the retry state (e.g., after a successful execution)
    pub fn reset(&mut self) {
        self.attempt = 0;
        self.last_error = None;
        self.next_retry_at = None;
        self.last_failure_at = None;
        // Note: total_failures is preserved for historical tracking
    }

    /// Check if we've exhausted all retry attempts
    pub fn is_exhausted(&self, config: &RetryConfig) -> bool {
        self.attempt >= config.max_retries
    }

    /// Get a human-readable status string
    pub fn status_string(&self, config: &RetryConfig) -> String {
        if self.attempt == 0 {
            return "Not retried".to_string();
        }

        if self.is_exhausted(config) {
            return format!(
                "Exhausted ({}/{} retries, {} total failures)",
                self.attempt, config.max_retries, self.total_failures
            );
        }

        match self.time_until_retry() {
            Some(ms) if ms > 0 => {
                let secs = ms / 1000;
                format!(
                    "Retry {}/{} in {}s",
                    self.attempt + 1,
                    config.max_retries,
                    secs
                )
            }
            _ => format!("Retry {}/{} ready", self.attempt + 1, config.max_retries),
        }
    }
}

/// Determine if an error is transient and worth retrying
///
/// Transient errors are temporary failures that might succeed on retry:
/// - Network timeouts
/// - Connection errors
/// - Rate limiting (429, 503)
/// - Temporary service unavailability
///
/// Non-transient errors should not be retried:
/// - Authentication failures (401, 403)
/// - Bad requests (400)
/// - Not found (404)
/// - Configuration errors
///
/// Prefer using `AiError::is_retryable()` when the original error type is available.
/// This string-based check is a fallback for contexts where only the error message is available.
pub fn is_transient_error(error: &str) -> bool {
    let error_lower = error.to_lowercase();

    // Transient error patterns
    let transient_patterns = [
        "timeout",
        "timed out",
        "connection refused",
        "connection reset",
        "connection closed",
        "network error",
        "network unreachable",
        "temporary failure",
        "temporarily unavailable",
        "service unavailable",
        "rate limit",
        "rate-limit",
        "too many requests",
        "429",
        "502",
        "503",
        "504",
        "gateway timeout",
        "bad gateway",
        "overloaded",
        "capacity",
        "retry after",
        "retry-after",
        "please try again",
        "internal server error",
        "500",
    ];

    // Non-transient error patterns (explicitly not retryable)
    let non_transient_patterns = [
        "unauthorized",
        "authentication",
        "auth failed",
        "invalid api key",
        "invalid token",
        "forbidden",
        "access denied",
        "permission denied",
        "401",
        "403",
        "not found",
        "404",
        "bad request",
        "invalid request",
        "validation error",
        "400",
        "invalid model",
        "model not found",
        "configuration error",
    ];

    // First check if it's explicitly non-transient
    for pattern in non_transient_patterns {
        if error_lower.contains(pattern) {
            return false;
        }
    }

    // Then check if it matches transient patterns
    for pattern in transient_patterns {
        if error_lower.contains(pattern) {
            return true;
        }
    }

    // Default: unknown errors are not retried
    false
}

/// Categorize an error for logging and metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Temporary failure, worth retrying
    Transient,
    /// Authentication/authorization failure
    AuthError,
    /// Client-side error (bad request, validation)
    ClientError,
    /// Resource not found
    NotFound,
    /// Unknown error category
    Unknown,
}

impl ErrorCategory {
    /// Categorize an error message
    pub fn from_error(error: &str) -> Self {
        let error_lower = error.to_lowercase();

        if error_lower.contains("401")
            || error_lower.contains("403")
            || error_lower.contains("unauthorized")
            || error_lower.contains("forbidden")
            || error_lower.contains("authentication")
            || error_lower.contains("api key")
        {
            return Self::AuthError;
        }

        if error_lower.contains("404") || error_lower.contains("not found") {
            return Self::NotFound;
        }

        if error_lower.contains("400")
            || error_lower.contains("bad request")
            || error_lower.contains("validation")
            || error_lower.contains("invalid")
        {
            return Self::ClientError;
        }

        if is_transient_error(error) {
            return Self::Transient;
        }

        Self::Unknown
    }

    /// Whether this error category should be retried
    pub fn should_retry(&self) -> bool {
        matches!(self, Self::Transient)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay_secs, 60);
        assert_eq!(config.max_delay_secs, 3600);
        assert_eq!(config.backoff_multiplier, 2.0);
        assert!(config.jitter_enabled);
    }

    #[test]
    fn test_retry_config_no_retries() {
        let config = RetryConfig::no_retries();
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn test_retry_config_aggressive() {
        let config = RetryConfig::aggressive();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay_secs, 30);
    }

    #[test]
    fn test_retry_config_conservative() {
        let config = RetryConfig::conservative();
        assert_eq!(config.max_retries, 2);
        assert_eq!(config.initial_delay_secs, 120);
    }

    #[test]
    fn test_retry_state_new() {
        let state = RetryState::new();
        assert_eq!(state.attempt, 0);
        assert!(state.last_error.is_none());
        assert!(state.next_retry_at.is_none());
    }

    #[test]
    fn test_should_retry_transient_error() {
        let config = RetryConfig::default();
        let state = RetryState::new();

        // Transient errors should be retried
        assert!(state.should_retry(&config, "Connection timeout"));
        assert!(state.should_retry(&config, "Rate limit exceeded"));
        assert!(state.should_retry(&config, "503 Service Unavailable"));
    }

    #[test]
    fn test_should_not_retry_auth_error() {
        let config = RetryConfig::default();
        let state = RetryState::new();

        // Auth errors should not be retried
        assert!(!state.should_retry(&config, "401 Unauthorized"));
        assert!(!state.should_retry(&config, "Invalid API key"));
        assert!(!state.should_retry(&config, "403 Forbidden"));
    }

    #[test]
    fn test_should_not_retry_when_exhausted() {
        let config = RetryConfig::default();
        let mut state = RetryState::new();
        state.attempt = config.max_retries;

        // Even transient errors should not retry when exhausted
        assert!(!state.should_retry(&config, "Connection timeout"));
    }

    #[test]
    fn test_calculate_delay_exponential() {
        let config = RetryConfig {
            max_retries: 5,
            initial_delay_secs: 10,
            max_delay_secs: 1000,
            backoff_multiplier: 2.0,
            jitter_enabled: false,
            jitter_factor: 0.0,
        };

        let mut state = RetryState::new();

        // First retry: 10 * 2^0 = 10
        assert_eq!(state.calculate_delay(&config).as_secs(), 10);

        state.attempt = 1;
        // Second retry: 10 * 2^1 = 20
        assert_eq!(state.calculate_delay(&config).as_secs(), 20);

        state.attempt = 2;
        // Third retry: 10 * 2^2 = 40
        assert_eq!(state.calculate_delay(&config).as_secs(), 40);
    }

    #[test]
    fn test_calculate_delay_capped() {
        let config = RetryConfig {
            max_retries: 10,
            initial_delay_secs: 100,
            max_delay_secs: 500,
            backoff_multiplier: 2.0,
            jitter_enabled: false,
            jitter_factor: 0.0,
        };

        let mut state = RetryState::new();
        state.attempt = 5;

        // Should be capped at max_delay_secs
        assert_eq!(state.calculate_delay(&config).as_secs(), 500);
    }

    #[test]
    fn test_record_failure() {
        let config = RetryConfig::default();
        let mut state = RetryState::new();

        state.record_failure("Connection timeout", &config);

        assert_eq!(state.attempt, 1);
        assert_eq!(state.total_failures, 1);
        assert_eq!(state.last_error, Some("Connection timeout".to_string()));
        assert!(state.next_retry_at.is_some());
        assert!(state.last_failure_at.is_some());
    }

    #[test]
    fn test_record_failure_non_transient() {
        let config = RetryConfig::default();
        let mut state = RetryState::new();

        state.record_failure("401 Unauthorized", &config);

        assert_eq!(state.attempt, 1);
        // Non-transient error: no next_retry_at
        assert!(state.next_retry_at.is_none());
    }

    #[test]
    fn test_reset() {
        let config = RetryConfig::default();
        let mut state = RetryState::new();

        state.record_failure("Connection timeout", &config);
        state.record_failure("Connection timeout", &config);

        state.reset();

        assert_eq!(state.attempt, 0);
        assert!(state.last_error.is_none());
        assert!(state.next_retry_at.is_none());
        // total_failures is preserved
        assert_eq!(state.total_failures, 2);
    }

    #[test]
    fn test_is_exhausted() {
        let config = RetryConfig::default();
        let mut state = RetryState::new();

        assert!(!state.is_exhausted(&config));

        state.attempt = config.max_retries;
        assert!(state.is_exhausted(&config));
    }

    #[test]
    fn test_is_transient_error() {
        // Transient errors
        assert!(is_transient_error("Connection timeout"));
        assert!(is_transient_error("Rate limit exceeded"));
        assert!(is_transient_error("503 Service Unavailable"));
        assert!(is_transient_error("504 Gateway Timeout"));
        assert!(is_transient_error("429 Too Many Requests"));
        assert!(is_transient_error("Network error"));
        assert!(is_transient_error("Connection reset by peer"));

        // Non-transient errors
        assert!(!is_transient_error("401 Unauthorized"));
        assert!(!is_transient_error("Invalid API key"));
        assert!(!is_transient_error("403 Forbidden"));
        assert!(!is_transient_error("404 Not Found"));
        assert!(!is_transient_error("400 Bad Request"));
        assert!(!is_transient_error("Invalid model specified"));
    }

    #[test]
    fn test_error_category() {
        assert_eq!(
            ErrorCategory::from_error("Connection timeout"),
            ErrorCategory::Transient
        );
        assert_eq!(
            ErrorCategory::from_error("401 Unauthorized"),
            ErrorCategory::AuthError
        );
        assert_eq!(
            ErrorCategory::from_error("404 Not Found"),
            ErrorCategory::NotFound
        );
        assert_eq!(
            ErrorCategory::from_error("400 Bad Request"),
            ErrorCategory::ClientError
        );
        assert_eq!(
            ErrorCategory::from_error("Some unknown error"),
            ErrorCategory::Unknown
        );
    }

    #[test]
    fn test_error_category_should_retry() {
        assert!(ErrorCategory::Transient.should_retry());
        assert!(!ErrorCategory::AuthError.should_retry());
        assert!(!ErrorCategory::ClientError.should_retry());
        assert!(!ErrorCategory::NotFound.should_retry());
        assert!(!ErrorCategory::Unknown.should_retry());
    }

    #[test]
    fn test_status_string() {
        let config = RetryConfig::default();
        let mut state = RetryState::new();

        assert_eq!(state.status_string(&config), "Not retried");

        state.record_failure("Connection timeout", &config);
        assert!(state.status_string(&config).contains("Retry 2/3"));

        state.attempt = config.max_retries;
        assert!(state.status_string(&config).contains("Exhausted"));
    }
}
