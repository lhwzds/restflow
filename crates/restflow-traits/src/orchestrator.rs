//! Shared orchestration contracts for agent execution surfaces.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ToolError;
use crate::subagent::InlineSubagentConfig;

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
    /// Optional inline temporary-subagent configuration.
    #[serde(default)]
    pub inline_subagent: Option<InlineSubagentConfig>,
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
    /// Optional parent execution ID.
    #[serde(default)]
    pub parent_execution_id: Option<String>,
    /// Optional trace session ID.
    #[serde(default)]
    pub trace_session_id: Option<String>,
    /// Optional trace scope ID.
    #[serde(default)]
    pub trace_scope_id: Option<String>,
    /// Optional authoritative run ID.
    #[serde(default)]
    pub run_id: Option<String>,
    /// Mode-specific metadata payload.
    #[serde(default)]
    pub metadata: Option<Value>,
}

impl ExecutionPlan {
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
            inline_subagent: Some(InlineSubagentConfig::default()),
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
            inline_subagent: Some(InlineSubagentConfig::default()),
            ..ExecutionPlan::default()
        };
        assert!(valid.validate().is_ok());
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
}
