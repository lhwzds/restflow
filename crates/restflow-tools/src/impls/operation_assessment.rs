use crate::{Result, ToolError, ToolErrorCategory, ToolOutput};
use restflow_traits::{OperationAssessment, OperationAssessmentStatus};
use serde_json::json;

fn assessment_message(assessment: &OperationAssessment) -> String {
    let issues = match assessment.status {
        OperationAssessmentStatus::Ok => return "Operation is ready.".to_string(),
        OperationAssessmentStatus::Warning => &assessment.warnings,
        OperationAssessmentStatus::Block => &assessment.blockers,
    };

    let message = issues
        .iter()
        .map(|issue| issue.message.trim())
        .filter(|message| !message.is_empty())
        .collect::<Vec<_>>()
        .join("; ");

    if message.is_empty() {
        match assessment.status {
            OperationAssessmentStatus::Warning => {
                "Operation requires confirmation before continuing.".to_string()
            }
            OperationAssessmentStatus::Block => {
                "Operation is blocked by validation or capability checks.".to_string()
            }
            OperationAssessmentStatus::Ok => "Operation is ready.".to_string(),
        }
    } else {
        message
    }
}

fn serialize_assessment_error(kind: &str, assessment: &OperationAssessment) -> ToolError {
    let payload = json!({
        "type": kind,
        "message": assessment_message(assessment),
        "assessment": assessment,
    });
    ToolError::Tool(payload.to_string())
}

pub(crate) fn preview_output(assessment: OperationAssessment) -> ToolOutput {
    ToolOutput::success(json!({
        "status": "preview",
        "assessment": assessment,
    }))
}

pub(crate) fn confirmation_required_output(assessment: OperationAssessment) -> ToolOutput {
    ToolOutput {
        success: false,
        result: json!({
            "pending_approval": true,
            "approval_id": assessment.approval_id,
            "assessment": assessment,
        }),
        error: Some(assessment_message(&assessment)),
        error_category: Some(ToolErrorCategory::Auth),
        retryable: Some(false),
        retry_after_ms: None,
    }
}

pub(crate) fn enforce_confirmation_or_defer(
    assessment: &OperationAssessment,
    approval_id: Option<&str>,
) -> Result<Option<ToolOutput>> {
    match assessment.status {
        OperationAssessmentStatus::Ok => Ok(None),
        OperationAssessmentStatus::Block => {
            Err(serialize_assessment_error("operation_blocked", assessment))
        }
        OperationAssessmentStatus::Warning => {
            if !assessment.requires_confirmation {
                return Ok(None);
            }

            let expected = assessment.approval_id.as_deref();
            let provided = approval_id.map(str::trim).filter(|value| !value.is_empty());

            if expected.is_some() && provided == expected {
                Ok(None)
            } else {
                Ok(Some(confirmation_required_output(assessment.clone())))
            }
        }
    }
}

pub(crate) fn guarded_confirmation_required_output(
    result: &serde_json::Value,
) -> Option<ToolOutput> {
    match result.get("status").and_then(|value| value.as_str()) {
        Some("confirmation_required") => {
            let assessment = result.get("assessment")?.clone();
            let approval_id = assessment
                .get("approval_id")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let message = first_assessment_issue_message(
                &assessment,
                "warnings",
                "Operation requires confirmation before continuing.",
            );

            Some(ToolOutput {
                success: false,
                result: json!({
                    "pending_approval": true,
                    "approval_id": approval_id,
                    "assessment": assessment,
                }),
                error: Some(message),
                error_category: Some(ToolErrorCategory::Auth),
                retryable: Some(false),
                retry_after_ms: None,
            })
        }
        Some("blocked") => {
            let assessment = result.get("assessment")?.clone();
            let message = first_assessment_issue_message(
                &assessment,
                "blockers",
                "Operation is blocked by validation or capability checks.",
            );

            Some(ToolOutput {
                success: false,
                result: json!({
                    "blocked": true,
                    "assessment": assessment,
                }),
                error: Some(message),
                error_category: Some(ToolErrorCategory::Config),
                retryable: Some(false),
                retry_after_ms: None,
            })
        }
        _ => None,
    }
}

fn first_assessment_issue_message(
    assessment: &serde_json::Value,
    field: &str,
    fallback: &str,
) -> String {
    assessment
        .get(field)
        .and_then(|issues| issues.as_array())
        .and_then(|issues| issues.first())
        .and_then(|issue| issue.get("message"))
        .and_then(|value| value.as_str())
        .unwrap_or(fallback)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guarded_blocked_result_becomes_failed_tool_output() {
        let output = guarded_confirmation_required_output(&json!({
            "status": "blocked",
            "assessment": {
                "blockers": [
                    { "message": "Task is bound to a protected session." }
                ]
            }
        }))
        .expect("blocked output");

        assert!(!output.success);
        assert_eq!(output.result["blocked"], true);
        assert_eq!(
            output.error.as_deref(),
            Some("Task is bound to a protected session.")
        );
        assert_eq!(output.error_category, Some(ToolErrorCategory::Config));
    }

    #[test]
    fn guarded_confirmation_result_preserves_pending_approval() {
        let output = guarded_confirmation_required_output(&json!({
            "status": "confirmation_required",
            "assessment": {
                "approval_id": "approval-1",
                "warnings": [
                    { "message": "This task requires confirmation." }
                ]
            }
        }))
        .expect("confirmation output");

        assert!(!output.success);
        assert_eq!(output.result["pending_approval"], true);
        assert_eq!(output.result["approval_id"], "approval-1");
        assert_eq!(
            output.error.as_deref(),
            Some("This task requires confirmation.")
        );
        assert_eq!(output.error_category, Some(ToolErrorCategory::Auth));
    }
}
