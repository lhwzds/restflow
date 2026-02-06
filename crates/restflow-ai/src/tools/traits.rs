//! Tool trait and types for AI agent tools

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::error::Result;
use crate::security::{SecurityGate, ToolAction};

pub type SecretResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

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

/// JSON Schema for tool parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema object
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub success: bool,
    pub result: Value,
    pub error: Option<String>,
}

impl ToolOutput {
    /// Create a successful tool output
    pub fn success(result: Value) -> Self {
        Self {
            success: true,
            result,
            error: None,
        }
    }

    /// Create an error tool output
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            result: Value::Null,
            error: Some(message.into()),
        }
    }
}

/// Skill info for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Skill content for reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContent {
    pub id: String,
    pub name: String,
    pub content: String,
}

/// Provider trait for accessing skills (implemented in restflow-workflow)
pub trait SkillProvider: Send + Sync {
    /// List all available skills
    fn list_skills(&self) -> Vec<SkillInfo>;
    /// Get skill content by ID
    fn get_skill(&self, id: &str) -> Option<SkillContent>;
}

/// Core trait for agent tools
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name (used in LLM function calls)
    fn name(&self) -> &str;

    /// Human-readable description for LLM context
    fn description(&self) -> &str;

    /// JSON Schema for input parameters
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with given input
    async fn execute(&self, input: Value) -> Result<ToolOutput>;

    /// Whether this tool supports parallel execution by default.
    /// Override to false for tools with side effects.
    fn supports_parallel(&self) -> bool {
        true
    }

    /// Whether this tool supports parallel execution for a specific input.
    /// Defaults to `supports_parallel()`.
    fn supports_parallel_for(&self, _input: &Value) -> bool {
        self.supports_parallel()
    }

    /// Build complete schema for LLM
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}
