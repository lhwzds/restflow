use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    ConfirmationRequired,
    NotFound,
    Conflict,
    Unauthorized,
    Forbidden,
    RateLimit,
    Timeout,
    Protocol,
    Internal,
}

impl ErrorKind {
    pub fn from_code(code: i32) -> Self {
        match code {
            400 => Self::Validation,
            428 => Self::ConfirmationRequired,
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            408 | 504 => Self::Timeout,
            409 => Self::Conflict,
            429 => Self::RateLimit,
            code if code < 0 => Self::Protocol,
            _ => Self::Internal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorPayload {
    pub code: i32,
    pub kind: ErrorKind,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ErrorPayload {
    pub fn new(code: i32, message: impl Into<String>, details: Option<Value>) -> Self {
        Self {
            code,
            kind: ErrorKind::from_code(code),
            message: message.into(),
            details,
        }
    }

    pub fn with_kind(
        code: i32,
        kind: ErrorKind,
        message: impl Into<String>,
        details: Option<Value>,
    ) -> Self {
        Self {
            code,
            kind,
            message: message.into(),
            details,
        }
    }

    pub fn not_found(what: &str) -> Self {
        Self::with_kind(
            404,
            ErrorKind::NotFound,
            format!("{} not found", what),
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_payload_round_trips() {
        let payload = ErrorPayload::with_kind(
            500,
            ErrorKind::Internal,
            "failed",
            Some(serde_json::json!({ "field": "agent_id" })),
        );

        let encoded = serde_json::to_string(&payload).unwrap();
        let decoded: ErrorPayload = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, payload);
    }

    #[test]
    fn error_kind_maps_from_code() {
        assert_eq!(ErrorKind::from_code(404), ErrorKind::NotFound);
        assert_eq!(ErrorKind::from_code(429), ErrorKind::RateLimit);
        assert_eq!(ErrorKind::from_code(428), ErrorKind::ConfirmationRequired);
        assert_eq!(ErrorKind::from_code(-2), ErrorKind::Protocol);
    }
}
