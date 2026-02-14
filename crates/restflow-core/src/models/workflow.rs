use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// High-level declarative workflow definition attached to a background agent.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[ts(export)]
pub struct WorkflowDefinition {
    #[serde(default)]
    pub phases: Vec<WorkflowPhase>,
}

/// Durable workflow runtime state for a specific background task.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct AgentWorkflow {
    pub id: String,
    pub task_id: String,
    #[serde(default)]
    pub phases: Vec<WorkflowPhase>,
    #[serde(default)]
    pub current_phase: usize,
    #[serde(default)]
    pub status: WorkflowStatus,
    #[serde(default)]
    pub phase_outputs: BTreeMap<u32, String>,
    #[ts(type = "number")]
    pub created_at: i64,
}

impl AgentWorkflow {
    pub fn from_definition(id: String, task_id: String, definition: WorkflowDefinition) -> Self {
        Self {
            id,
            task_id,
            phases: definition.phases,
            current_phase: 0,
            status: WorkflowStatus::Running,
            phase_outputs: BTreeMap::new(),
            created_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// Execution status for a workflow.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowStatus {
    #[default]
    Running,
    PhaseFailed {
        phase_idx: usize,
        error: String,
    },
    Completed,
    Suspended,
}

/// One workflow phase with optional dependencies and retry policy.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkflowPhase {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub input_template: Option<String>,
    #[serde(default)]
    pub retry_config: WorkflowRetryConfig,
    #[serde(default)]
    pub depends_on: Vec<usize>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Retry settings for one workflow phase.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkflowRetryConfig {
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_initial_backoff_ms")]
    pub initial_backoff_ms: u64,
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f32,
    #[serde(default)]
    pub non_retryable_errors: Vec<String>,
}

impl Default for WorkflowRetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            initial_backoff_ms: default_initial_backoff_ms(),
            max_backoff_ms: default_max_backoff_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            non_retryable_errors: Vec::new(),
        }
    }
}

/// Serialized checkpoint persisted between attempts or restarts.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkflowCheckpoint {
    pub workflow_id: String,
    pub phase_idx: usize,
    pub attempt: u32,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub state: serde_json::Value,
    #[serde(default)]
    pub phase_outputs: BTreeMap<u32, String>,
    #[ts(type = "number")]
    pub created_at: i64,
}

const fn default_max_attempts() -> u32 {
    3
}

const fn default_initial_backoff_ms() -> u64 {
    10_000
}

const fn default_max_backoff_ms() -> u64 {
    300_000
}

const fn default_backoff_multiplier() -> f32 {
    2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_retry_config_defaults() {
        let cfg = WorkflowRetryConfig::default();
        assert_eq!(cfg.max_attempts, 3);
        assert_eq!(cfg.initial_backoff_ms, 10_000);
        assert_eq!(cfg.max_backoff_ms, 300_000);
        assert!((cfg.backoff_multiplier - 2.0).abs() < f32::EPSILON);
        assert!(cfg.non_retryable_errors.is_empty());
    }

    #[test]
    fn test_agent_workflow_from_definition() {
        let definition = WorkflowDefinition {
            phases: vec![WorkflowPhase {
                name: "research".to_string(),
                description: None,
                skill_id: None,
                input_template: Some("Research {{topic}}".to_string()),
                retry_config: WorkflowRetryConfig::default(),
                depends_on: vec![],
                timeout_secs: None,
            }],
        };

        let workflow =
            AgentWorkflow::from_definition("wf-1".to_string(), "task-1".to_string(), definition);
        assert_eq!(workflow.current_phase, 0);
        assert_eq!(workflow.status, WorkflowStatus::Running);
        assert_eq!(workflow.phases.len(), 1);
        assert!(workflow.phase_outputs.is_empty());
    }
}
