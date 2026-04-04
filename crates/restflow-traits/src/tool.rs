//! Tool trait and types for AI agent tools.

use async_trait::async_trait;
pub use restflow_contracts::ToolErrorCategory;
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
        // Default-open fallback for environments where security policies
        // are intentionally not configured.
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
///
/// TODO(ToolSearch): Add tool discovery/deferral metadata to support lazy-loading.
/// Claude Code's ToolSearch pattern (src/tools/ToolSearchTool/) saves prompt tokens
/// by only exposing core tools upfront and loading the rest on-demand.
///
/// Proposed additions:
/// ```ignore
/// /// Whether this tool can run concurrently with other tools.
/// /// When false, the executor runs it serially (after concurrent batch).
/// fn is_concurrency_safe(&self, _input: &Value) -> bool { false }
///
/// /// Whether this tool only reads data (no side effects).
/// /// Read-only tools in the same batch can run in parallel.
/// fn is_read_only(&self, _input: &Value) -> bool { false }
///
/// /// Whether this tool performs irreversible operations (delete, overwrite, send).
/// fn is_destructive(&self, _input: &Value) -> bool { false }
///
/// /// Whether to defer loading this tool's schema until the model requests it.
/// /// Deferred tools are hidden from the initial prompt and discovered via ToolSearch.
/// fn should_defer(&self) -> bool { false }
///
/// /// Whether this tool must always appear in the initial prompt (never deferred).
/// fn always_load(&self) -> bool { false }
///
/// /// Short capability phrase for keyword-based tool search (3-10 words).
/// fn search_hint(&self) -> Option<&str> { None }
/// ```
///
/// Implementation plan:
/// 1. Add these methods with defaults to this trait
/// 2. Create a `ToolSearchTool` in restflow-tools that does keyword matching
///    over deferred tools (no embedding needed — Claude Code uses pure keyword
///    scoring: name parts +10, search_hint +4, description +2)
/// 3. In executor (restflow-ai/src/agent/executor/mod.rs:560), split tools into
///    `always_load` vs `should_defer`, only send loaded tools to the LLM
/// 4. When the model calls ToolSearch, return matching tool schemas as text
///    (Anthropic's `tool_reference` beta is provider-specific, so return full
///    schema text and inject into next API call's tools array)
/// 5. Auto-enable when deferred tool schemas exceed 10% of context window
///    (see Claude Code's `tst-auto` mode in src/utils/toolSearch.ts)
///
/// Benefits: 58 tools × ~800 tokens/schema = ~46K tokens. ToolSearch cuts this
/// to ~5K for the average request (6 core tools + ToolSearch).
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
