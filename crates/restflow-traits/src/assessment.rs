use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::store::{
    AgentCreateRequest, AgentUpdateRequest, BackgroundAgentControlRequest,
    BackgroundAgentConvertSessionRequest, BackgroundAgentCreateRequest,
    BackgroundAgentUpdateRequest,
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
    pub confirmation_token: Option<String>,
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
            confirmation_token: None,
            effective_model_ref: None,
            warnings: Vec::new(),
            blockers: Vec::new(),
        }
    }
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
