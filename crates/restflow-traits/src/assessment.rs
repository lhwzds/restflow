use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::store::{
    AgentCreateRequest, AgentUpdateRequest, BackgroundAgentControlRequest,
    BackgroundAgentConvertSessionRequest, BackgroundAgentCreateRequest,
    BackgroundAgentDeleteRequest, BackgroundAgentUpdateRequest,
};
use crate::subagent::ContractSubagentSpawnRequest;

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

pub fn normalize_legacy_approval_replay(value: &mut serde_json::Value) {
    let serde_json::Value::Object(map) = value else {
        return;
    };

    let needs_approval_id = map
        .get("approval_id")
        .map(serde_json::Value::is_null)
        .unwrap_or(true);
    if needs_approval_id
        && let Some(legacy) = map.get("confirmation_token").cloned()
        && !legacy.is_null()
    {
        map.insert("approval_id".to_string(), legacy);
    }
    map.remove("confirmation_token");
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BackgroundAgentCommandOutcome<T> {
    Preview { assessment: OperationAssessment },
    Blocked { assessment: OperationAssessment },
    ConfirmationRequired { assessment: OperationAssessment },
    Executed { result: T },
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

    async fn assess_background_agent_create(
        &self,
        request: BackgroundAgentCreateRequest,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_background_agent_convert_session(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_background_agent_update(
        &self,
        request: BackgroundAgentUpdateRequest,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_background_agent_delete(
        &self,
        request: BackgroundAgentDeleteRequest,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_background_agent_control(
        &self,
        request: BackgroundAgentControlRequest,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_background_agent_template(
        &self,
        operation: &str,
        intent: OperationAssessmentIntent,
        agent_ids: Vec<String>,
        template_mode: bool,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_subagent_spawn(
        &self,
        operation: &str,
        request: ContractSubagentSpawnRequest,
        template_mode: bool,
    ) -> Result<OperationAssessment, ToolError>;

    async fn assess_subagent_batch(
        &self,
        operation: &str,
        requests: Vec<ContractSubagentSpawnRequest>,
        template_mode: bool,
    ) -> Result<OperationAssessment, ToolError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn operation_assessment_serializes_with_approval_id() {
        let assessment = OperationAssessment::warning_with_confirmation(
            "delete_background_agent",
            OperationAssessmentIntent::Save,
            vec![OperationAssessmentIssue {
                code: "warning".to_string(),
                message: "Needs approval".to_string(),
                field: None,
                suggestion: None,
            }],
        );

        let payload = serde_json::to_value(&assessment).expect("serialize assessment");
        assert!(payload.get("approval_id").and_then(|value| value.as_str()).is_some());
        assert!(payload.get("confirmation_token").is_none());
    }

    #[test]
    fn normalize_legacy_approval_replay_promotes_confirmation_token() {
        let mut payload = json!({
            "operation": "delete",
            "confirmation_token": "approval-1"
        });

        normalize_legacy_approval_replay(&mut payload);

        assert_eq!(payload["approval_id"], "approval-1");
        assert!(payload.get("confirmation_token").is_none());
    }

    #[test]
    fn normalize_legacy_approval_replay_does_not_override_approval_id() {
        let mut payload = json!({
            "operation": "delete",
            "approval_id": "preferred",
            "confirmation_token": "legacy"
        });

        normalize_legacy_approval_replay(&mut payload);

        assert_eq!(payload["approval_id"], "preferred");
        assert!(payload.get("confirmation_token").is_none());
    }
}
