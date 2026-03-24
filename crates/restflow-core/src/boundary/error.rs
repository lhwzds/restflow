use crate::daemon::IpcResponse;
use crate::models::{ValidationError, ValidationErrorResponse};

pub(crate) fn invalid_request_response(error: anyhow::Error) -> IpcResponse {
    IpcResponse::error(400, format!("Invalid request payload: {error:#}"))
}

pub(crate) fn invalid_validation_response(errors: Vec<ValidationError>) -> IpcResponse {
    let details = serde_json::to_value(ValidationErrorResponse::new(errors))
        .expect("validation error response should serialize");
    IpcResponse::error_with_details(400, "Validation failed", Some(details))
}
