use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::store::{AgentCreateRequest, AgentUpdateRequest};
use crate::subagent::ContractRunSpawnRequest;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationAssessmentStatus {
    Ok,
    Warning,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationAssessmentIntent {
    Save,
    Run,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssessmentModelRef {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationAssessmentIssue {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationAssessment {
    pub operation: String,
    pub intent: OperationAssessmentIntent,
    pub status: OperationAssessmentStatus,
    pub requires_confirmation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_model_ref: Option<AssessmentModelRef>,
    #[serde(default)]
    pub warnings: Vec<OperationAssessmentIssue>,
    #[serde(default)]
    pub blockers: Vec<OperationAssessmentIssue>,
}

impl OperationAssessment {
    pub fn ok(operation: impl Into<String>, intent: OperationAssessmentIntent) -> Self {
        Self {
            operation: operation.into(),
            intent,
            status: OperationAssessmentStatus::Ok,
            requires_confirmation: false,
            approval_id: None,
            effective_model_ref: None,
            warnings: Vec::new(),
            blockers: Vec::new(),
        }
    }

    pub fn warning_with_confirmation(
        operation: impl Into<String>,
        intent: OperationAssessmentIntent,
        warnings: Vec<OperationAssessmentIssue>,
    ) -> Self {
        let mut assessment = Self {
            operation: operation.into(),
            intent,
            status: OperationAssessmentStatus::Warning,
            requires_confirmation: true,
            approval_id: None,
            effective_model_ref: None,
            warnings,
            blockers: Vec::new(),
        };
        assessment.approval_id = Some(build_approval_id(&assessment));
        assessment
    }
}

fn build_approval_id(assessment: &OperationAssessment) -> String {
    let payload = serde_json::json!({
        "operation": assessment.operation,
        "intent": assessment.intent,
        "effective_model_ref": assessment.effective_model_ref,
        "warnings": assessment.warnings,
        "blockers": assessment.blockers,
    });
    let encoded = serde_json::to_vec(&payload).unwrap_or_default();
    let mut hash = 0xcbf29ce484222325u64;
    for byte in encoded {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[async_trait]
pub trait AgentOperationAssessor: Send + Sync {
    async fn assess_agent_create(
        &self,
        request: AgentCreateRequest,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_agent_update(
        &self,
        request: AgentUpdateRequest,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_subagent_spawn(
        &self,
        operation: &str,
        request: ContractRunSpawnRequest,
        template_mode: bool,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_subagent_batch(
        &self,
        operation: &str,
        requests: Vec<ContractRunSpawnRequest>,
        template_mode: bool,
    ) -> Result<OperationAssessment, ToolError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_assessment_serializes_with_approval_id() {
        let assessment = OperationAssessment::warning_with_confirmation(
            "delete_task",
            OperationAssessmentIntent::Save,
            vec![OperationAssessmentIssue {
                code: "warning".to_string(),
                message: "Needs approval".to_string(),
                field: None,
                suggestion: None,
            }],
        );

        let payload = serde_json::to_value(&assessment).expect("serialize assessment");
        assert!(
            payload
                .get("approval_id")
                .and_then(|value| value.as_str())
                .is_some()
        );
    }
}
