//! Model Failover System
//!
//! This module provides automatic failover between AI models when the primary
//! model fails or becomes unavailable. It tracks model health and routes
//! requests to healthy fallback models.
//!
//! # Features
//!
//! - Primary/fallback model configuration
//! - Automatic health tracking per model
//! - Cooldown periods after failures
//! - Circuit breaker pattern for unhealthy models
//! - Configurable failure thresholds
//!
//! # Example
//!
//! ```ignore
//! use restflow_tauri::agent_task::failover::{FailoverConfig, FailoverManager};
//! use crate::AIModel;
//!
//! let config = FailoverConfig {
//!     primary: AIModel::ClaudeSonnet4_5,
//!     fallbacks: vec![AIModel::Gpt5, AIModel::DeepseekChat],
//!     cooldown_secs: 300,
//!     failure_threshold: 3,
//! };
//!
//! let manager = FailoverManager::new(config);
//!
//! // Get the best available model
//! if let Some(model) = manager.get_available_model().await {
//!     // Use this model for the request
//! }
//!
//! // Record failure/success
//! manager.record_failure(AIModel::ClaudeSonnet4_5).await;
//! manager.record_success(AIModel::Gpt5).await;
//! ```

use crate::AIModel;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration for the model failover system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverConfig {
    /// Primary model to use when healthy
    pub primary: AIModel,
    /// Fallback models in order of preference
    pub fallbacks: Vec<AIModel>,
    /// Cooldown period in seconds after a model failure
    pub cooldown_secs: u64,
    /// Number of consecutive failures before putting model in cooldown
    pub failure_threshold: u32,
    /// Whether to automatically recover models after cooldown expires
    pub auto_recover: bool,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            primary: AIModel::ClaudeSonnet4_5,
            fallbacks: vec![AIModel::Gpt5, AIModel::DeepseekChat],
            cooldown_secs: 300,   // 5 minutes
            failure_threshold: 3, // 3 consecutive failures
            auto_recover: true,
        }
    }
}

impl FailoverConfig {
    /// Create a new failover config with the specified primary model.
    ///
    /// CLI-based models (Codex CLI, OpenCode, Gemini CLI) disable fallbacks
    /// because they manage their own authentication and cannot fall back
    /// to API-based models that require different credentials.
    pub fn with_primary(primary: AIModel) -> Self {
        let fallbacks = if primary.is_cli_model() {
            vec![]
        } else {
            Self::default().fallbacks
        };
        Self {
            primary,
            fallbacks,
            ..Default::default()
        }
    }

    /// Create a config with custom fallbacks
    pub fn with_fallbacks(primary: AIModel, fallbacks: Vec<AIModel>) -> Self {
        Self {
            primary,
            fallbacks,
            ..Default::default()
        }
    }

    /// Get all models in priority order (primary first, then fallbacks)
    pub fn all_models(&self) -> Vec<AIModel> {
        let mut models = vec![self.primary];
        models.extend(self.fallbacks.iter().copied());
        models
    }

    /// Check if a model is in the failover chain
    pub fn contains(&self, model: AIModel) -> bool {
        self.primary == model || self.fallbacks.contains(&model)
    }
}

/// Health state for a single model
#[derive(Debug, Clone, Default)]
struct ModelHealth {
    /// Number of consecutive failures
    consecutive_failures: u32,
    /// Total failures since last reset
    total_failures: u32,
    /// Total successes since last reset
    total_successes: u32,
    /// Timestamp when cooldown expires (None = healthy)
    cooldown_until: Option<i64>,
    /// Last failure error message
    last_error: Option<String>,
    /// Timestamp of last failure
    last_failure_at: Option<i64>,
    /// Timestamp of last success
    last_success_at: Option<i64>,
}

impl ModelHealth {
    fn new() -> Self {
        Self::default()
    }

    /// Check if the model is currently in cooldown
    fn is_in_cooldown(&self, now: i64) -> bool {
        self.cooldown_until
            .map(|until| now < until)
            .unwrap_or(false)
    }

    /// Check if the model is available (not in cooldown)
    fn is_available(&self, now: i64) -> bool {
        !self.is_in_cooldown(now)
    }

    /// Get remaining cooldown time in milliseconds
    fn remaining_cooldown_ms(&self, now: i64) -> Option<i64> {
        self.cooldown_until.and_then(|until| {
            let remaining = until - now;
            if remaining > 0 { Some(remaining) } else { None }
        })
    }

    /// Calculate success rate (0.0 to 1.0)
    fn success_rate(&self) -> f64 {
        let total = self.total_successes + self.total_failures;
        if total == 0 {
            1.0 // Assume healthy if no data
        } else {
            self.total_successes as f64 / total as f64
        }
    }
}

/// Model status information for external use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatus {
    pub model: AIModel,
    pub available: bool,
    pub consecutive_failures: u32,
    pub success_rate: f64,
    pub cooldown_remaining_secs: Option<u64>,
    pub last_error: Option<String>,
}

/// The failover manager that tracks model health and selects available models
pub struct FailoverManager {
    config: FailoverConfig,
    health: Arc<RwLock<HashMap<AIModel, ModelHealth>>>,
}

impl FailoverManager {
    /// Create a new failover manager with the given configuration
    pub fn new(config: FailoverConfig) -> Self {
        Self {
            config,
            health: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a manager with default configuration
    pub fn with_defaults() -> Self {
        Self::new(FailoverConfig::default())
    }

    /// Get the best available model
    ///
    /// Returns the primary model if healthy, otherwise the first healthy fallback.
    /// Returns None if all models are in cooldown.
    pub async fn get_available_model(&self) -> Option<AIModel> {
        let health = self.health.read().await;
        let now = chrono::Utc::now().timestamp_millis();

        // Try primary first
        if self.is_model_available(&health, self.config.primary, now) {
            return Some(self.config.primary);
        }

        debug!(
            "Primary model {:?} unavailable, checking fallbacks",
            self.config.primary
        );

        // Try fallbacks in order
        for &model in &self.config.fallbacks {
            if self.is_model_available(&health, model, now) {
                info!("Failing over to model {:?}", model);
                return Some(model);
            }
        }

        warn!("All models are in cooldown or unavailable");
        None
    }

    /// Get a specific model if available, or the best alternative
    pub async fn get_model_or_fallback(&self, preferred: AIModel) -> Option<AIModel> {
        let health = self.health.read().await;
        let now = chrono::Utc::now().timestamp_millis();

        // Try preferred model first
        if self.is_model_available(&health, preferred, now) {
            return Some(preferred);
        }

        // Fall back to normal priority order
        drop(health);
        self.get_available_model().await
    }

    /// Check if a specific model is available
    fn is_model_available(
        &self,
        health: &HashMap<AIModel, ModelHealth>,
        model: AIModel,
        now: i64,
    ) -> bool {
        match health.get(&model) {
            Some(h) => h.is_available(now),
            None => true, // No health data = assume healthy
        }
    }

    /// Record a successful request to a model
    pub async fn record_success(&self, model: AIModel) {
        let mut health = self.health.write().await;
        let now = chrono::Utc::now().timestamp_millis();

        let entry = health.entry(model).or_insert_with(ModelHealth::new);
        entry.consecutive_failures = 0;
        entry.total_successes += 1;
        entry.last_success_at = Some(now);

        // Clear cooldown on success (if auto_recover is enabled)
        if self.config.auto_recover {
            entry.cooldown_until = None;
        }

        debug!(
            "Model {:?} success recorded (total: {}, rate: {:.1}%)",
            model,
            entry.total_successes,
            entry.success_rate() * 100.0
        );
    }

    /// Record a failed request to a model
    pub async fn record_failure(&self, model: AIModel) {
        self.record_failure_with_error(model, None).await
    }

    /// Record a failed request with error details
    pub async fn record_failure_with_error(&self, model: AIModel, error: Option<&str>) {
        let mut health = self.health.write().await;
        let now = chrono::Utc::now().timestamp_millis();

        let entry = health.entry(model).or_insert_with(ModelHealth::new);
        entry.consecutive_failures += 1;
        entry.total_failures += 1;
        entry.last_failure_at = Some(now);

        if let Some(err) = error {
            entry.last_error = Some(err.to_string());
        }

        // Check if we should put the model in cooldown
        if entry.consecutive_failures >= self.config.failure_threshold {
            let cooldown_until = now + (self.config.cooldown_secs * 1000) as i64;
            entry.cooldown_until = Some(cooldown_until);

            warn!(
                "Model {:?} placed in cooldown for {}s after {} consecutive failures",
                model, self.config.cooldown_secs, entry.consecutive_failures
            );
        } else {
            debug!(
                "Model {:?} failure {}/{} before cooldown",
                model, entry.consecutive_failures, self.config.failure_threshold
            );
        }
    }

    /// Manually clear cooldown for a model
    pub async fn clear_cooldown(&self, model: AIModel) {
        let mut health = self.health.write().await;
        if let Some(entry) = health.get_mut(&model) {
            entry.cooldown_until = None;
            entry.consecutive_failures = 0;
            info!("Manually cleared cooldown for model {:?}", model);
        }
    }

    /// Manually put a model in cooldown
    pub async fn force_cooldown(&self, model: AIModel) {
        let mut health = self.health.write().await;
        let now = chrono::Utc::now().timestamp_millis();
        let cooldown_until = now + (self.config.cooldown_secs * 1000) as i64;

        let entry = health.entry(model).or_insert_with(ModelHealth::new);
        entry.cooldown_until = Some(cooldown_until);

        info!(
            "Manually placed model {:?} in cooldown for {}s",
            model, self.config.cooldown_secs
        );
    }

    /// Get the status of all configured models
    pub async fn get_all_status(&self) -> Vec<ModelStatus> {
        let health = self.health.read().await;
        let now = chrono::Utc::now().timestamp_millis();

        self.config
            .all_models()
            .into_iter()
            .map(|model| self.model_status(&health, model, now))
            .collect()
    }

    /// Get the status of a specific model
    pub async fn get_status(&self, model: AIModel) -> ModelStatus {
        let health = self.health.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        self.model_status(&health, model, now)
    }

    fn model_status(
        &self,
        health: &HashMap<AIModel, ModelHealth>,
        model: AIModel,
        now: i64,
    ) -> ModelStatus {
        match health.get(&model) {
            Some(h) => ModelStatus {
                model,
                available: h.is_available(now),
                consecutive_failures: h.consecutive_failures,
                success_rate: h.success_rate(),
                cooldown_remaining_secs: h.remaining_cooldown_ms(now).map(|ms| (ms / 1000) as u64),
                last_error: h.last_error.clone(),
            },
            None => ModelStatus {
                model,
                available: true,
                consecutive_failures: 0,
                success_rate: 1.0,
                cooldown_remaining_secs: None,
                last_error: None,
            },
        }
    }

    /// Reset all health tracking data
    pub async fn reset(&self) {
        let mut health = self.health.write().await;
        health.clear();
        info!("Failover manager reset - all models marked healthy");
    }

    /// Get the current configuration
    pub fn config(&self) -> &FailoverConfig {
        &self.config
    }

    /// Check if any model is available
    pub async fn any_available(&self) -> bool {
        self.get_available_model().await.is_some()
    }

    /// Get count of available models
    pub async fn available_count(&self) -> usize {
        let health = self.health.read().await;
        let now = chrono::Utc::now().timestamp_millis();

        self.config
            .all_models()
            .iter()
            .filter(|&&model| self.is_model_available(&health, model, now))
            .count()
    }
}

/// Execute a task with automatic failover
///
/// Tries the primary model first, then falls back to alternates on failure.
/// Returns the result from the first successful model or the last error.
pub async fn execute_with_failover<F, Fut, T>(
    manager: &FailoverManager,
    mut execute_fn: F,
) -> Result<(T, AIModel)>
where
    F: FnMut(AIModel) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let models = manager.config().all_models();
    let mut last_error = None;

    for model in models {
        // Check if this model is available
        let status = manager.get_status(model).await;
        if !status.available {
            debug!("Skipping model {:?} (in cooldown)", model);
            continue;
        }

        debug!("Attempting execution with model {:?}", model);

        match execute_fn(model).await {
            Ok(result) => {
                manager.record_success(model).await;
                return Ok((result, model));
            }
            Err(e) => {
                let error_str = e.to_string();
                warn!("Model {:?} failed: {}", model, error_str);
                manager
                    .record_failure_with_error(model, Some(&error_str))
                    .await;
                last_error = Some(e);
            }
        }
    }

    // All models failed
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No models available")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> FailoverConfig {
        FailoverConfig {
            primary: AIModel::ClaudeSonnet4_5,
            fallbacks: vec![AIModel::Gpt5, AIModel::DeepseekChat],
            cooldown_secs: 60,
            failure_threshold: 2,
            auto_recover: true,
        }
    }

    #[tokio::test]
    async fn test_get_available_model_primary_healthy() {
        let manager = FailoverManager::new(test_config());

        let model = manager.get_available_model().await;
        assert_eq!(model, Some(AIModel::ClaudeSonnet4_5));
    }

    #[tokio::test]
    async fn test_get_available_model_primary_in_cooldown() {
        let manager = FailoverManager::new(test_config());

        // Put primary in cooldown
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;

        let model = manager.get_available_model().await;
        // Should fall back to first fallback
        assert_eq!(model, Some(AIModel::Gpt5));
    }

    #[tokio::test]
    async fn test_get_available_model_all_in_cooldown() {
        let config = FailoverConfig {
            primary: AIModel::ClaudeSonnet4_5,
            fallbacks: vec![AIModel::Gpt5],
            cooldown_secs: 60,
            failure_threshold: 1, // Single failure triggers cooldown
            auto_recover: true,
        };
        let manager = FailoverManager::new(config);

        // Put all models in cooldown
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;
        manager.record_failure(AIModel::Gpt5).await;

        let model = manager.get_available_model().await;
        assert_eq!(model, None);
    }

    #[tokio::test]
    async fn test_record_success_clears_cooldown() {
        let manager = FailoverManager::new(test_config());

        // Put in cooldown
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;

        // Verify in cooldown
        let status = manager.get_status(AIModel::ClaudeSonnet4_5).await;
        assert!(!status.available);

        // Record success
        manager.record_success(AIModel::ClaudeSonnet4_5).await;

        // Should be available again
        let status = manager.get_status(AIModel::ClaudeSonnet4_5).await;
        assert!(status.available);
    }

    #[tokio::test]
    async fn test_failure_threshold() {
        let config = FailoverConfig {
            primary: AIModel::ClaudeSonnet4_5,
            fallbacks: vec![],
            cooldown_secs: 60,
            failure_threshold: 3,
            auto_recover: true,
        };
        let manager = FailoverManager::new(config);

        // First two failures: still available
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;
        let status = manager.get_status(AIModel::ClaudeSonnet4_5).await;
        assert!(status.available);
        assert_eq!(status.consecutive_failures, 1);

        manager.record_failure(AIModel::ClaudeSonnet4_5).await;
        let status = manager.get_status(AIModel::ClaudeSonnet4_5).await;
        assert!(status.available);
        assert_eq!(status.consecutive_failures, 2);

        // Third failure: should trigger cooldown
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;
        let status = manager.get_status(AIModel::ClaudeSonnet4_5).await;
        assert!(!status.available);
        assert_eq!(status.consecutive_failures, 3);
    }

    #[tokio::test]
    async fn test_clear_cooldown() {
        let manager = FailoverManager::new(test_config());

        // Put in cooldown
        manager.force_cooldown(AIModel::ClaudeSonnet4_5).await;

        // Verify in cooldown
        let status = manager.get_status(AIModel::ClaudeSonnet4_5).await;
        assert!(!status.available);

        // Clear cooldown
        manager.clear_cooldown(AIModel::ClaudeSonnet4_5).await;

        // Should be available
        let status = manager.get_status(AIModel::ClaudeSonnet4_5).await;
        assert!(status.available);
    }

    #[tokio::test]
    async fn test_get_all_status() {
        let manager = FailoverManager::new(test_config());

        let statuses = manager.get_all_status().await;
        assert_eq!(statuses.len(), 3); // primary + 2 fallbacks

        // All should be available initially
        for status in &statuses {
            assert!(status.available);
            assert_eq!(status.consecutive_failures, 0);
        }
    }

    #[tokio::test]
    async fn test_success_rate() {
        let manager = FailoverManager::new(test_config());

        // 3 successes, 1 failure = 75% success rate
        manager.record_success(AIModel::ClaudeSonnet4_5).await;
        manager.record_success(AIModel::ClaudeSonnet4_5).await;
        manager.record_success(AIModel::ClaudeSonnet4_5).await;
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;

        let status = manager.get_status(AIModel::ClaudeSonnet4_5).await;
        assert!((status.success_rate - 0.75).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_reset() {
        let manager = FailoverManager::new(test_config());

        // Add some state
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;
        manager.record_failure(AIModel::ClaudeSonnet4_5).await;
        manager.record_failure(AIModel::Gpt5).await;

        // Reset
        manager.reset().await;

        // All models should be healthy
        let statuses = manager.get_all_status().await;
        for status in statuses {
            assert!(status.available);
            assert_eq!(status.consecutive_failures, 0);
            assert_eq!(status.success_rate, 1.0);
        }
    }

    #[tokio::test]
    async fn test_available_count() {
        let manager = FailoverManager::new(test_config());

        // Initially all available
        assert_eq!(manager.available_count().await, 3);

        // Put one in cooldown
        manager.force_cooldown(AIModel::ClaudeSonnet4_5).await;
        assert_eq!(manager.available_count().await, 2);

        // Put another in cooldown
        manager.force_cooldown(AIModel::Gpt5).await;
        assert_eq!(manager.available_count().await, 1);
    }

    #[tokio::test]
    async fn test_config_all_models() {
        let config = test_config();
        let models = config.all_models();

        assert_eq!(models.len(), 3);
        assert_eq!(models[0], AIModel::ClaudeSonnet4_5);
        assert_eq!(models[1], AIModel::Gpt5);
        assert_eq!(models[2], AIModel::DeepseekChat);
    }

    #[tokio::test]
    async fn test_config_contains() {
        let config = test_config();

        assert!(config.contains(AIModel::ClaudeSonnet4_5));
        assert!(config.contains(AIModel::Gpt5));
        assert!(config.contains(AIModel::DeepseekChat));
        assert!(!config.contains(AIModel::Gemini25Pro));
    }

    #[tokio::test]
    async fn test_execute_with_failover_success() {
        let manager = FailoverManager::new(test_config());

        let result = execute_with_failover(&manager, |model| async move {
            if model == AIModel::ClaudeSonnet4_5 {
                Ok("success")
            } else {
                Err(anyhow::anyhow!("wrong model"))
            }
        })
        .await;

        assert!(result.is_ok());
        let (value, model) = result.unwrap();
        assert_eq!(value, "success");
        assert_eq!(model, AIModel::ClaudeSonnet4_5);
    }

    #[tokio::test]
    async fn test_execute_with_failover_fallback() {
        let manager = FailoverManager::new(test_config());

        // Primary fails, fallback succeeds
        let result = execute_with_failover(&manager, |model| async move {
            if model == AIModel::Gpt5 {
                Ok("fallback success")
            } else {
                Err(anyhow::anyhow!("primary failed"))
            }
        })
        .await;

        assert!(result.is_ok());
        let (value, model) = result.unwrap();
        assert_eq!(value, "fallback success");
        assert_eq!(model, AIModel::Gpt5);
    }

    #[tokio::test]
    async fn test_execute_with_failover_all_fail() {
        let manager = FailoverManager::new(test_config());

        let result: Result<(String, AIModel)> =
            execute_with_failover(&manager, |_model| async move {
                Err(anyhow::anyhow!("all models fail"))
            })
            .await;

        assert!(result.is_err());
    }
}
