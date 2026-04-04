use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::store::{
    AgentCreateRequest, AgentUpdateRequest, BackgroundAgentControlRequest,
    BackgroundAgentConvertSessionRequest, BackgroundAgentCreateRequest,
    BackgroundAgentDeleteRequest, BackgroundAgentUpdateRequest, TaskControlRequest,
    TaskConvertSessionRequest, TaskCreateRequest, TaskDeleteRequest, TaskUpdateRequest,
};
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
pub enum TaskCommandOutcome<T> {
    Preview { assessment: OperationAssessment },
    Blocked { assessment: OperationAssessment },
    ConfirmationRequired { assessment: OperationAssessment },
    Executed { result: T },
}

pub type BackgroundAgentCommandOutcome<T> = TaskCommandOutcome<T>;

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

    async fn assess_task_create(
        &self,
        request: TaskCreateRequest,
    ) -> Result<OperationAssessment, ToolError> {
        self.assess_background_agent_create(request).await
    }

    async fn assess_task_convert_session(
        &self,
        request: TaskConvertSessionRequest,
    ) -> Result<OperationAssessment, ToolError> {
        self.assess_background_agent_convert_session(request).await
    }

    async fn assess_task_update(
        &self,
        request: TaskUpdateRequest,
    ) -> Result<OperationAssessment, ToolError> {
        self.assess_background_agent_update(request).await
    }

    async fn assess_task_delete(
        &self,
        request: TaskDeleteRequest,
    ) -> Result<OperationAssessment, ToolError> {
        self.assess_background_agent_delete(request).await
    }

    async fn assess_task_control(
        &self,
        request: TaskControlRequest,
    ) -> Result<OperationAssessment, ToolError> {
        self.assess_background_agent_control(request).await
    }

    async fn assess_task_template(
        &self,
        operation: &str,
        intent: OperationAssessmentIntent,
        agent_ids: Vec<String>,
        template_mode: bool,
    ) -> Result<OperationAssessment, ToolError> {
        self.assess_background_agent_template(operation, intent, agent_ids, template_mode)
            .await
    }

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
    use serde_json::json;
    use std::sync::{Arc, Mutex};

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
        assert!(
            payload
                .get("approval_id")
                .and_then(|value| value.as_str())
                .is_some()
        );
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

    struct MockAssessor {
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl MockAssessor {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().expect("calls lock").clone()
        }

        fn record(&self, label: &'static str) {
            self.calls.lock().expect("calls lock").push(label);
        }
    }

    #[async_trait]
    impl AgentOperationAssessor for MockAssessor {
        async fn assess_agent_create(
            &self,
            _request: AgentCreateRequest,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("agent_create");
            Ok(OperationAssessment::ok(
                "agent_create",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_agent_update(
            &self,
            _request: AgentUpdateRequest,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("agent_update");
            Ok(OperationAssessment::ok(
                "agent_update",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_background_agent_create(
            &self,
            _request: BackgroundAgentCreateRequest,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("background_agent_create");
            Ok(OperationAssessment::ok(
                "background_agent_create",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_background_agent_convert_session(
            &self,
            _request: BackgroundAgentConvertSessionRequest,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("background_agent_convert_session");
            Ok(OperationAssessment::ok(
                "background_agent_convert_session",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_background_agent_update(
            &self,
            _request: BackgroundAgentUpdateRequest,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("background_agent_update");
            Ok(OperationAssessment::ok(
                "background_agent_update",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_background_agent_delete(
            &self,
            _request: BackgroundAgentDeleteRequest,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("background_agent_delete");
            Ok(OperationAssessment::ok(
                "background_agent_delete",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_background_agent_control(
            &self,
            _request: BackgroundAgentControlRequest,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("background_agent_control");
            Ok(OperationAssessment::ok(
                "background_agent_control",
                OperationAssessmentIntent::Save,
            ))
        }

        async fn assess_background_agent_template(
            &self,
            operation: &str,
            intent: OperationAssessmentIntent,
            _agent_ids: Vec<String>,
            _template_mode: bool,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("background_agent_template");
            Ok(OperationAssessment::ok(operation.to_string(), intent))
        }

        async fn assess_subagent_spawn(
            &self,
            operation: &str,
            _request: ContractRunSpawnRequest,
            _template_mode: bool,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("subagent_spawn");
            Ok(OperationAssessment::ok(
                operation.to_string(),
                OperationAssessmentIntent::Run,
            ))
        }

        async fn assess_subagent_batch(
            &self,
            operation: &str,
            _requests: Vec<ContractRunSpawnRequest>,
            _template_mode: bool,
        ) -> Result<OperationAssessment, ToolError> {
            self.record("subagent_batch");
            Ok(OperationAssessment::ok(
                operation.to_string(),
                OperationAssessmentIntent::Run,
            ))
        }
    }

    #[tokio::test]
    async fn task_assessment_methods_forward_to_background_methods() {
        let assessor = MockAssessor::new();

        let create = assessor
            .assess_task_create(TaskCreateRequest {
                name: "Task".to_string(),
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                schedule: restflow_contracts::request::TaskSchedule::Interval {
                    interval_ms: 1_000,
                    start_at: None,
                },
                input: None,
                input_template: None,
                timeout_secs: None,
                durability_mode: None,
                memory: None,
                memory_scope: None,
                resource_limits: None,
                preview: false,
                approval_id: None,
            })
            .await
            .expect("task create assessment should forward");
        assert_eq!(create.operation, "background_agent_create");

        let convert = assessor
            .assess_task_convert_session(TaskConvertSessionRequest {
                session_id: "session-1".to_string(),
                name: None,
                schedule: None,
                input: None,
                timeout_secs: None,
                durability_mode: None,
                memory: None,
                memory_scope: None,
                resource_limits: None,
                run_now: None,
                preview: false,
                approval_id: None,
            })
            .await
            .expect("task convert assessment should forward");
        assert_eq!(convert.operation, "background_agent_convert_session");

        let control = assessor
            .assess_task_control(TaskControlRequest {
                id: "task-1".to_string(),
                action: "run_now".to_string(),
                preview: false,
                approval_id: None,
            })
            .await
            .expect("task control assessment should forward");
        assert_eq!(control.operation, "background_agent_control");

        let template = assessor
            .assess_task_template(
                "save_task_template",
                OperationAssessmentIntent::Save,
                vec!["agent-1".to_string()],
                true,
            )
            .await
            .expect("task template assessment should forward");
        assert_eq!(template.operation, "save_task_template");

        assert_eq!(
            assessor.calls(),
            vec![
                "background_agent_create",
                "background_agent_convert_session",
                "background_agent_control",
                "background_agent_template",
            ]
        );
    }
}
