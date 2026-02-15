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
    /// Create a successful tool output
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

    /// Create an error tool output
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

    pub fn with_retry_after_ms(mut self, retry_after_ms: Option<u64>) -> Self {
        self.retry_after_ms = retry_after_ms;
        self
    }

    pub fn with_error_metadata(
        mut self,
        category: ToolErrorCategory,
        retryable: bool,
        retry_after_ms: Option<u64>,
    ) -> Self {
        self.error_category = Some(category);
        self.retryable = Some(retryable);
        self.retry_after_ms = retry_after_ms;
        self
    }

    pub fn with_error_message(mut self, message: impl Into<String>) -> Self {
        self.error = Some(message.into());
        self
    }

    pub fn with_error_details(
        mut self,
        message: impl Into<String>,
        category: ToolErrorCategory,
        retryable: bool,
        retry_after_ms: Option<u64>,
    ) -> Self {
        self.error = Some(message.into());
        self.error_category = Some(category);
        self.retryable = Some(retryable);
        self.retry_after_ms = retry_after_ms;
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

/// Skill record for create/update operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub content: String,
}

/// Skill update payload
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillUpdate {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub tags: Option<Option<Vec<String>>>,
    pub content: Option<String>,
}

/// Provider trait for accessing skills (implemented in restflow-workflow)
pub trait SkillProvider: Send + Sync {
    /// List all available skills
    fn list_skills(&self) -> Vec<SkillInfo>;
    /// Get skill content by ID
    fn get_skill(&self, id: &str) -> Option<SkillContent>;
    /// Create a new skill
    fn create_skill(&self, skill: SkillRecord) -> std::result::Result<SkillRecord, String>;
    /// Update an existing skill
    fn update_skill(
        &self,
        id: &str,
        update: SkillUpdate,
    ) -> std::result::Result<SkillRecord, String>;
    /// Delete a skill
    fn delete_skill(&self, id: &str) -> std::result::Result<bool, String>;
    /// Export a skill to markdown
    fn export_skill(&self, id: &str) -> std::result::Result<String, String>;
    /// Import a skill from markdown
    fn import_skill(
        &self,
        id: &str,
        markdown: &str,
        overwrite: bool,
    ) -> std::result::Result<SkillRecord, String>;
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
