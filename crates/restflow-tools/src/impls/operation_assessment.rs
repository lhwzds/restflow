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
            "approval_id": assessment.confirmation_token,
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
    confirmation_token: Option<&str>,
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

            let expected = assessment.confirmation_token.as_deref();
            let provided = confirmation_token
                .map(str::trim)
                .filter(|value| !value.is_empty());

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
    if result.get("status").and_then(|value| value.as_str()) != Some("confirmation_required") {
        return None;
    }

    let assessment = result.get("assessment")?.clone();
    let approval_id = assessment
        .get("confirmation_token")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let message = assessment
        .get("warnings")
        .and_then(|warnings| warnings.as_array())
        .and_then(|warnings| warnings.first())
        .and_then(|warning| warning.get("message"))
        .and_then(|value| value.as_str())
        .unwrap_or("Operation requires confirmation before continuing.")
        .to_string();

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
