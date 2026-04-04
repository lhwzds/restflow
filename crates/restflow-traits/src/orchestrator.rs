//! Shared orchestration contracts for agent execution surfaces.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ToolError;
use crate::subagent::InlineRunConfig;

/// Lifecycle mode for one agent execution plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Interactive,
    Subagent,
    Background,
}

/// Shared execution plan consumed by orchestrators.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionPlan {
    /// Lifecycle mode that should handle this execution.
    pub mode: Option<ExecutionMode>,
    /// Optional stored agent identifier.
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Optional inline temporary child-run configuration.
    #[serde(default)]
    pub inline_subagent: Option<InlineRunConfig>,
    /// Runtime input for the execution.
    #[serde(default)]
    pub input: Option<String>,
    /// Optional chat session ID for interactive mode.
    #[serde(default)]
    pub chat_session_id: Option<String>,
    /// Optional background task ID for background mode.
    #[serde(default)]
    pub background_task_id: Option<String>,
    /// Optional timeout override in seconds.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Optional model override.
    #[serde(default)]
    pub model: Option<String>,
    /// Optional provider paired with model override.
    #[serde(default)]
    pub provider: Option<String>,
    /// Optional max iterations override.
    #[serde(default)]
    pub max_iterations: Option<u32>,
    /// Optional parent run ID.
    ///
    /// The serialized field name is canonicalized to `parent_run_id` while
    /// still accepting legacy `parent_execution_id` input for compatibility.
    #[serde(default, rename = "parent_run_id", alias = "parent_execution_id")]
    pub parent_execution_id: Option<String>,
    /// Optional trace session ID.
    #[serde(default)]
    pub trace_session_id: Option<String>,
    /// Optional trace scope ID.
    #[serde(default)]
    pub trace_scope_id: Option<String>,
    /// Optional authoritative run ID.
    ///
    /// For sub-agent executions this identifies the canonical child run. When
    /// supplied by a caller that already owns lifecycle emission, executors
    /// must reuse this run ID without emitting a second top-level lifecycle.
    #[serde(default)]
    pub run_id: Option<String>,
    /// Mode-specific metadata payload.
    #[serde(default)]
    pub metadata: Option<Value>,
}

impl ExecutionPlan {
    /// Returns the canonical parent run identifier for this execution plan.
    pub fn parent_run_id(&self) -> Option<&str> {
        self.parent_execution_id.as_deref()
    }

    /// Sets the canonical parent run identifier while preserving legacy storage.
    pub fn set_parent_run_id(&mut self, parent_run_id: Option<String>) {
        self.parent_execution_id = parent_run_id;
    }

    /// Validate that the plan contains the minimum fields required for its mode.
    pub fn validate(&self) -> Result<(), ToolError> {
        let mode = self
            .mode
            .as_ref()
            .ok_or_else(|| ToolError::Tool("Execution plan requires 'mode'.".to_string()))?;

        let has_model = self
            .model
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        let has_provider = self
            .provider
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if has_model != has_provider {
            return Err(ToolError::Tool(
                "Execution plan requires both 'model' and 'provider' when either is set."
                    .to_string(),
            ));
        }

        match mode {
            ExecutionMode::Interactive => {
                if self
                    .chat_session_id
                    .as_ref()
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
                {
                    return Err(ToolError::Tool(
                        "Interactive execution requires 'chat_session_id'.".to_string(),
                    ));
                }
                if self
                    .input
                    .as_ref()
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
                {
                    return Err(ToolError::Tool(
                        "Interactive execution requires non-empty 'input'.".to_string(),
                    ));
                }
            }
            ExecutionMode::Subagent => {
                let has_selector = self
                    .agent_id
                    .as_ref()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false)
                    || self.inline_subagent.is_some()
                    || (has_model && has_provider);
                if !has_selector {
                    return Err(ToolError::Tool(
                        "Subagent execution requires 'agent_id', 'inline_subagent', or paired 'model' and 'provider'.".to_string(),
                    ));
                }
                if self
                    .input
                    .as_ref()
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
                {
                    return Err(ToolError::Tool(
                        "Subagent execution requires non-empty 'input'.".to_string(),
                    ));
                }
            }
            ExecutionMode::Background => {
                if self
                    .agent_id
                    .as_ref()
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
                {
                    return Err(ToolError::Tool(
                        "Background execution requires 'agent_id'.".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Normalized outcome returned by an orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionOutcome {
    /// Whether execution succeeded.
    pub success: bool,
    /// Main textual output.
    #[serde(default)]
    pub text: Option<String>,
    /// Optional structured result payload.
    #[serde(default)]
    pub result: Option<Value>,
    /// Optional metadata payload.
    #[serde(default)]
    pub metadata: Option<Value>,
    /// Optional error message.
    #[serde(default)]
    pub error: Option<String>,
    /// Optional iteration count.
    #[serde(default)]
    pub iterations: Option<u32>,
    /// Optional resolved model identifier.
    #[serde(default)]
    pub model: Option<String>,
    /// Optional duration in milliseconds.
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// Shared orchestrator abstraction used by higher-level lifecycle adapters.
#[async_trait]
pub trait AgentOrchestrator: Send + Sync {
    async fn run(&self, plan: ExecutionPlan) -> Result<ExecutionOutcome, ToolError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_plan_requires_paired_model_and_provider() {
        let plan = ExecutionPlan {
            mode: Some(ExecutionMode::Subagent),
            input: Some("task".to_string()),
            inline_subagent: Some(InlineRunConfig::default()),
            model: Some("gpt-5.3-codex".to_string()),
            ..ExecutionPlan::default()
        };

        let error = plan.validate().unwrap_err();
        assert!(error.to_string().contains("both 'model' and 'provider'"));
    }

    #[test]
    fn test_execution_plan_validates_subagent_mode() {
        let invalid = ExecutionPlan {
            mode: Some(ExecutionMode::Subagent),
            input: Some("task".to_string()),
            ..ExecutionPlan::default()
        };
        assert!(invalid.validate().is_err());

        let valid = ExecutionPlan {
            mode: Some(ExecutionMode::Subagent),
            input: Some("task".to_string()),
            inline_subagent: Some(InlineRunConfig::default()),
            ..ExecutionPlan::default()
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_execution_plan_parent_run_id_accessors_round_trip() {
        let mut plan = ExecutionPlan::default();
        assert_eq!(plan.parent_run_id(), None);

        plan.set_parent_run_id(Some("parent-1".to_string()));
        assert_eq!(plan.parent_run_id(), Some("parent-1"));
        assert_eq!(plan.parent_execution_id.as_deref(), Some("parent-1"));
    }

    #[test]
    fn test_execution_plan_serializes_parent_run_id_canonically() {
        let mut plan = ExecutionPlan::default();
        plan.set_parent_run_id(Some("parent-1".to_string()));

        let serialized = serde_json::to_value(plan).expect("serialize execution plan");
        assert_eq!(serialized["parent_run_id"], "parent-1");
        assert!(serialized.get("parent_execution_id").is_none());
    }

    #[test]
    fn test_execution_plan_accepts_legacy_parent_execution_id_alias() {
        let plan: ExecutionPlan = serde_json::from_value(serde_json::json!({
            "mode": "subagent",
            "input": "task",
            "inline_subagent": {},
            "parent_execution_id": "legacy-parent"
        }))
        .expect("deserialize execution plan");

        assert_eq!(plan.parent_run_id(), Some("legacy-parent"));
        assert_eq!(plan.parent_execution_id.as_deref(), Some("legacy-parent"));
    }

    #[test]
    fn test_execution_plan_accepts_model_provider_only_subagent_mode() {
        let valid = ExecutionPlan {
            mode: Some(ExecutionMode::Subagent),
            input: Some("task".to_string()),
            model: Some("gpt-5.3-codex".to_string()),
            provider: Some("openai".to_string()),
            ..ExecutionPlan::default()
        };

        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_execution_plan_interactive_only_requires_session_and_input() {
        let valid = ExecutionPlan {
            mode: Some(ExecutionMode::Interactive),
            chat_session_id: Some("session-1".to_string()),
            input: Some("hello".to_string()),
            ..ExecutionPlan::default()
        };

        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_execution_plan_rejects_whitespace_only_fields() {
        let invalid = ExecutionPlan {
            mode: Some(ExecutionMode::Subagent),
            agent_id: Some("   ".to_string()),
            input: Some("   ".to_string()),
            ..ExecutionPlan::default()
        };

        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_execution_plan_accepts_optional_run_id_for_subagent_mode() {
        let valid = ExecutionPlan {
            mode: Some(ExecutionMode::Subagent),
            agent_id: Some("child".to_string()),
            input: Some("task".to_string()),
            run_id: Some("child-run-1".to_string()),
            ..ExecutionPlan::default()
        };

        assert!(valid.validate().is_ok());
        assert_eq!(valid.run_id.as_deref(), Some("child-run-1"));
    }
}
