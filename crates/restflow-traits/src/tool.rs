//! Tool trait and types for AI agent tools.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::error::Result;
use crate::security::{SecurityGate, ToolAction};

/// Type alias for secret resolution callbacks.
pub type SecretResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Check security gate and return a blocking message if the action is denied.
pub async fn check_security(
    gate: Option<&dyn SecurityGate>,
    action: ToolAction,
    agent_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<Option<String>> {
    let Some(gate) = gate else {
        return Ok(None);
    };

    let decision = gate.check_tool_action(&action, agent_id, task_id).await?;

    if decision.allowed {
        return Ok(None);
    }

    if decision.requires_approval {
        let approval_id = decision
            .approval_id
            .unwrap_or_else(|| "unknown".to_string());
        return Ok(Some(format!(
            "Action requires user approval (ID: {}). Waiting for approval of: {}",
            approval_id, action.summary
        )));
    }

    let reason = decision
        .reason
        .unwrap_or_else(|| "Action blocked by policy".to_string());
    Ok(Some(format!("Action blocked: {}", reason)))
}

/// JSON Schema for tool parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema object
}

/// Result of tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub success: bool,
    pub result: Value,
    pub error: Option<String>,
    pub error_category: Option<ToolErrorCategory>,
    pub retryable: Option<bool>,
    pub retry_after_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolErrorCategory {
    Network,
    Auth,
    Config,
    Execution,
    RateLimit,
    NotFound,
}

impl ToolOutput {
    /// Create a successful tool output.
    pub fn success(result: Value) -> Self {
        Self {
            success: true,
            result,
            error: None,
            error_category: None,
            retryable: None,
            retry_after_ms: None,
        }
    }

    /// Create an error tool output.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            result: Value::Null,
            error: Some(message.into()),
            error_category: None,
            retryable: None,
            retry_after_ms: None,
        }
    }

    pub fn retryable_error(message: impl Into<String>, category: ToolErrorCategory) -> Self {
        Self {
            success: false,
            result: Value::Null,
            error: Some(message.into()),
            error_category: Some(category),
            retryable: Some(true),
            retry_after_ms: None,
        }
    }

    pub fn non_retryable_error(message: impl Into<String>, category: ToolErrorCategory) -> Self {
        Self {
            success: false,
            result: Value::Null,
            error: Some(message.into()),
            error_category: Some(category),
            retryable: Some(false),
            retry_after_ms: None,
        }
    }

    pub fn with_error_message(mut self, message: impl Into<String>) -> Self {
        self.error = Some(message.into());
        self
    }

    pub fn classify_if_error(
        mut self,
        classifier: impl FnOnce(&str) -> (ToolErrorCategory, bool, Option<u64>),
    ) -> Self {
        if !self.success
            && let Some(err) = self.error.as_deref()
        {
            let (category, retryable, retry_after_ms) = classifier(err);
            self.error_category = Some(category);
            self.retryable = Some(retryable);
            self.retry_after_ms = retry_after_ms;
        }
        self
    }
}

/// Core trait for agent tools.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name (used in LLM function calls).
    fn name(&self) -> &str;

    /// Human-readable description for LLM context.
    fn description(&self) -> &str;

    /// JSON Schema for input parameters.
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with given input.
    async fn execute(&self, input: Value) -> Result<ToolOutput>;

    /// Build complete schema for LLM.
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}
