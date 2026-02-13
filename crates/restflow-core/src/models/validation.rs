use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Structured validation error for model and API validation.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Payload returned to clients when validation fails.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct ValidationErrorResponse {
    #[serde(rename = "type")]
    pub error_type: String,
    pub errors: Vec<ValidationError>,
}

impl ValidationErrorResponse {
    pub fn new(errors: Vec<ValidationError>) -> Self {
        Self {
            error_type: "validation_error".to_string(),
            errors,
        }
    }
}

pub fn encode_validation_error(errors: Vec<ValidationError>) -> String {
    let response = ValidationErrorResponse::new(errors);
    serde_json::to_string(&response).unwrap_or_else(|_| {
        "{\"type\":\"validation_error\",\"errors\":[{\"field\":\"_global\",\"message\":\"Validation failed\"}]}".to_string()
    })
}
