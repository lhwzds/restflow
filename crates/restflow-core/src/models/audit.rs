use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single persisted audit entry produced during one background-agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub task_id: String,
    pub execution_id: String,
    pub timestamp: DateTime<Utc>,
    pub entry_type: AuditEntryType,
}

impl AuditEntry {
    pub fn new(
        task_id: impl Into<String>,
        execution_id: impl Into<String>,
        entry_type: AuditEntryType,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.into(),
            execution_id: execution_id.into(),
            timestamp: Utc::now(),
            entry_type,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditEntryType {
    ExecutionStart {
        agent_id: String,
        model: String,
        input_preview: String,
    },
    LlmCall {
        model: String,
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
        duration_ms: u64,
        iteration: usize,
    },
    ToolCall {
        tool_name: String,
        success: bool,
        duration_ms: u64,
        input_size_bytes: usize,
        output_size_bytes: usize,
        error: Option<String>,
        iteration: usize,
    },
    ModelSwitch {
        from_model: String,
        to_model: String,
        reason: String,
        iteration: usize,
    },
    ExecutionComplete {
        total_iterations: usize,
        total_tokens: u32,
        total_cost_usd: f64,
        total_duration_ms: u64,
        success: bool,
    },
    ExecutionFailed {
        error: String,
        total_duration_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAuditSummary {
    pub tool_name: String,
    pub call_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub total_duration_ms: u64,
    pub avg_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAuditSummary {
    pub model: String,
    pub call_count: usize,
    pub total_tokens: u32,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    pub task_id: String,
    pub execution_id: String,
    pub total_llm_calls: usize,
    pub total_tool_calls: usize,
    pub total_tokens: u32,
    pub total_cost_usd: f64,
    pub total_duration_ms: u64,
    pub success: Option<bool>,
    pub tool_breakdown: Vec<ToolAuditSummary>,
    pub model_breakdown: Vec<ModelAuditSummary>,
}
