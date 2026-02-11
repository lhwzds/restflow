use anyhow::Result;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManageOpsOperation {
    DaemonStatus,
    DaemonHealth,
    BackgroundSummary,
    SessionSummary,
    LogTail,
}

impl ManageOpsOperation {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::DaemonStatus => "daemon_status",
            Self::DaemonHealth => "daemon_health",
            Self::BackgroundSummary => "background_summary",
            Self::SessionSummary => "session_summary",
            Self::LogTail => "log_tail",
        }
    }
}

impl TryFrom<&str> for ManageOpsOperation {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "daemon_status" => Ok(Self::DaemonStatus),
            "daemon_health" => Ok(Self::DaemonHealth),
            "background_summary" => Ok(Self::BackgroundSummary),
            "session_summary" => Ok(Self::SessionSummary),
            "log_tail" => Ok(Self::LogTail),
            other => Err(anyhow::anyhow!(
                "Unknown operation: {}. Supported: daemon_status, daemon_health, background_summary, session_summary, log_tail",
                other
            )),
        }
    }
}

pub fn parse_operation(raw: &str) -> Result<ManageOpsOperation> {
    ManageOpsOperation::try_from(raw)
}

pub fn build_response(
    operation: ManageOpsOperation,
    evidence: Value,
    verification: Value,
) -> Value {
    json!({
        "operation": operation.as_str(),
        "evidence": evidence,
        "verification": verification
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_operation_case_insensitive() {
        assert_eq!(
            parse_operation("  DaEmOn_StAtUs ").unwrap(),
            ManageOpsOperation::DaemonStatus
        );
        assert_eq!(
            parse_operation("background_summary").unwrap(),
            ManageOpsOperation::BackgroundSummary
        );
    }

    #[test]
    fn test_parse_operation_rejects_unknown() {
        let err = parse_operation("unknown").unwrap_err().to_string();
        assert!(err.contains("Unknown operation"));
    }

    #[test]
    fn test_build_response_schema() {
        let response = build_response(
            ManageOpsOperation::SessionSummary,
            json!({"total": 1}),
            json!({"ok": true}),
        );
        assert_eq!(response["operation"], "session_summary");
        assert!(response.get("evidence").is_some());
        assert!(response.get("verification").is_some());
    }
}
