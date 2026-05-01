use crate::{ErrorKind, ErrorPayload};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "response_type", content = "data")]
pub enum ResponseEnvelope<T> {
    Pong,
    Success(T),
    Error(ErrorPayload),
}

impl ResponseEnvelope<Value> {
    pub fn success<T: Serialize>(data: T) -> Self {
        match serde_json::to_value(data) {
            Ok(value) => Self::Success(value),
            Err(error) => Self::Error(ErrorPayload::with_kind(
                500,
                ErrorKind::Internal,
                "Failed to serialize response payload",
                Some(serde_json::json!({ "cause": error.to_string() })),
            )),
        }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self::error_with_details(code, message, None)
    }

    pub fn error_with_details(
        code: i32,
        message: impl Into<String>,
        details: Option<Value>,
    ) -> Self {
        Self::Error(ErrorPayload::new(code, message, details))
    }

    pub fn error_payload(payload: ErrorPayload) -> Self {
        Self::Error(payload)
    }

    pub fn not_found(what: &str) -> Self {
        Self::Error(ErrorPayload::not_found(what))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingSerialize;

    impl Serialize for FailingSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("boom"))
        }
    }

    #[test]
    fn response_success_round_trips() {
        let response = ResponseEnvelope::<Value>::success(serde_json::json!({
            "deleted": true
        }));

        let encoded = serde_json::to_string(&response).unwrap();
        let decoded: ResponseEnvelope<Value> = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, response);
        assert!(encoded.contains("response_type"));
    }

    #[test]
    fn response_error_round_trips() {
        let response = ResponseEnvelope::<Value>::error_with_details(
            500,
            "failed",
            Some(serde_json::json!({ "error_kind": "session_policy" })),
        );

        let encoded = serde_json::to_string(&response).unwrap();
        let decoded: ResponseEnvelope<Value> = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn response_success_serialization_failure_returns_error_payload() {
        let response = ResponseEnvelope::<Value>::success(FailingSerialize);

        match response {
            ResponseEnvelope::Error(error) => {
                assert_eq!(error.code, 500);
                assert_eq!(error.kind, ErrorKind::Internal);
                assert_eq!(error.message, "Failed to serialize response payload");
                assert_eq!(error.details.unwrap()["cause"], "boom");
            }
            other => panic!("unexpected response variant: {other:?}"),
        }
    }
}
