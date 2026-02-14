//! Model routing helpers for choosing a model tier based on task complexity.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Task complexity tier for model routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskTier {
    /// Simple operations: file reads, status checks, formatting.
    Routine,
    /// Moderate complexity: code generation, summaries, translations.
    Moderate,
    /// High complexity: debugging, architecture, multi-file refactoring.
    Complex,
}

/// Model routing configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelRoutingConfig {
    /// Enable automatic model routing.
    pub enabled: bool,
    /// Model for routine tasks (cheapest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine_model: Option<String>,
    /// Model for moderate tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderate_model: Option<String>,
    /// Model for complex tasks (most capable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complex_model: Option<String>,
    /// Auto-escalate to complex tier when previous iteration failed.
    pub escalate_on_failure: bool,
}

impl Default for ModelRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            routine_model: None,
            moderate_model: None,
            complex_model: None,
            escalate_on_failure: true,
        }
    }
}

/// Runtime abstraction used by the executor to switch the active model.
#[async_trait]
pub trait ModelSwitcher: Send + Sync {
    /// Return the currently active model identifier.
    fn current_model(&self) -> String;

    /// Switch to the target model identifier.
    async fn switch_model(&self, target_model: &str) -> Result<()>;
}

/// Classify the complexity of a pending agent action.
pub fn classify_task(
    tool_names: &[&str],
    message_content: &str,
    iteration: usize,
    previous_failure: bool,
) -> TaskTier {
    if previous_failure {
        return TaskTier::Complex;
    }

    let complex_signals = [
        "debug",
        "fix",
        "refactor",
        "architect",
        "design",
        "security",
        "vulnerability",
        "migration",
        "breaking change",
        "performance",
        "optimize",
        "concurrent",
        "deadlock",
    ];

    let routine_signals = [
        "list", "read", "status", "get", "fetch", "search", "format", "lint", "check", "version",
        "help",
    ];

    let content_lower = message_content.to_lowercase();
    let tool_str = tool_names.join(" ").to_lowercase();
    let combined = format!("{} {}", content_lower, tool_str);

    let complex_score = complex_signals
        .iter()
        .filter(|signal| combined.contains(**signal))
        .count();
    let routine_score = routine_signals
        .iter()
        .filter(|signal| combined.contains(**signal))
        .count();

    let iteration_bonus = if iteration > 10 {
        2
    } else if iteration > 5 {
        1
    } else {
        0
    };

    let total_complex = complex_score + iteration_bonus;
    if total_complex >= 2 {
        TaskTier::Complex
    } else if routine_score >= 2 && total_complex == 0 {
        TaskTier::Routine
    } else {
        TaskTier::Moderate
    }
}

/// Select the model for a given tier based on routing config.
pub fn select_model(config: &ModelRoutingConfig, tier: TaskTier, default_model: &str) -> String {
    match tier {
        TaskTier::Routine => config
            .routine_model
            .clone()
            .unwrap_or_else(|| default_model.to_string()),
        TaskTier::Moderate => config
            .moderate_model
            .clone()
            .unwrap_or_else(|| default_model.to_string()),
        TaskTier::Complex => config
            .complex_model
            .clone()
            .unwrap_or_else(|| default_model.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelRoutingConfig, TaskTier, classify_task, select_model};

    #[test]
    fn classify_routine_task() {
        let tier = classify_task(&["file"], "list all files and check status", 1, false);
        assert_eq!(tier, TaskTier::Routine);
    }

    #[test]
    fn classify_complex_task() {
        let tier = classify_task(
            &["bash", "file"],
            "debug the authentication deadlock",
            1,
            false,
        );
        assert_eq!(tier, TaskTier::Complex);
    }

    #[test]
    fn escalation_on_failure_forces_complex() {
        let tier = classify_task(&["file"], "read config", 1, true);
        assert_eq!(tier, TaskTier::Complex);
    }

    #[test]
    fn late_iteration_adds_complexity_bonus() {
        let tier = classify_task(&["bash"], "run tests", 12, false);
        assert_eq!(tier, TaskTier::Complex);
    }

    #[test]
    fn select_model_falls_back_to_default() {
        let config = ModelRoutingConfig {
            enabled: true,
            routine_model: Some("gpt-5-nano".to_string()),
            moderate_model: None,
            complex_model: Some("gpt-5".to_string()),
            escalate_on_failure: true,
        };
        assert_eq!(
            select_model(&config, TaskTier::Routine, "claude-sonnet-4-5"),
            "gpt-5-nano"
        );
        assert_eq!(
            select_model(&config, TaskTier::Moderate, "claude-sonnet-4-5"),
            "claude-sonnet-4-5"
        );
        assert_eq!(
            select_model(&config, TaskTier::Complex, "claude-sonnet-4-5"),
            "gpt-5"
        );
    }
}
