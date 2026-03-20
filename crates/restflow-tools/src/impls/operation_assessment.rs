use crate::{Result, ToolError, ToolOutput};
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

pub(crate) fn enforce_confirmation(
    assessment: &OperationAssessment,
    confirmation_token: Option<&str>,
) -> Result<()> {
    match assessment.status {
        OperationAssessmentStatus::Ok => Ok(()),
        OperationAssessmentStatus::Block => {
            Err(serialize_assessment_error("operation_blocked", assessment))
        }
        OperationAssessmentStatus::Warning => {
            if !assessment.requires_confirmation {
                return Ok(());
            }

            let expected = assessment
                .confirmation_token
                .as_deref()
                .ok_or_else(|| serialize_assessment_error("confirmation_required", assessment))?;
            let provided = confirmation_token
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| serialize_assessment_error("confirmation_required", assessment))?;

            if provided == expected {
                Ok(())
            } else {
                Err(serialize_assessment_error(
                    "confirmation_required",
                    assessment,
                ))
            }
        }
    }
}
