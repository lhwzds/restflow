use anyhow::Error as AnyhowError;
use restflow_ai::AiError;
use restflow_traits::{ToolErrorCategory, ToolOutput};

use super::outcome::{
    ExecutionErrorClassification, ExecutionErrorKind, ExecutionFailure, RetryClass,
};

pub fn classify_execution_error(error: &AnyhowError) -> ExecutionErrorClassification {
    if let Some(ai_error) = error.downcast_ref::<AiError>() {
        return classify_ai_error(ai_error);
    }

    classify_execution_error_message(&error.to_string())
}

pub fn classify_execution_error_message(message: &str) -> ExecutionErrorClassification {
    let lower = message.to_lowercase();

    if contains_any(
        &lower,
        &[
            "interrupted",
            "cancelled",
            "canceled",
            "user interrupt",
            "user requested stop",
        ],
    ) {
        return ExecutionErrorClassification::new(
            ExecutionErrorKind::UserInterrupted,
            RetryClass::NonRetryable,
        );
    }

    if contains_any(
        &lower,
        &[
            "unauthorized",
            "forbidden",
            "authentication",
            "auth failed",
            "invalid api key",
            "invalid token",
            "api key",
            "api_key",
            "secret",
            "credential",
            "401",
            "403",
            "billing",
        ],
    ) {
        return ExecutionErrorClassification::new(
            ExecutionErrorKind::Authentication,
            RetryClass::NonRetryable,
        );
    }

    if contains_any(
        &lower,
        &[
            "rate limit",
            "rate-limit",
            "too many requests",
            "retry after",
            "retry-after",
            "quota",
            "429",
        ],
    ) {
        return ExecutionErrorClassification::new(
            ExecutionErrorKind::RateLimited,
            RetryClass::Retryable,
        );
    }

    if contains_any(
        &lower,
        &[
            "timeout",
            "timed out",
            "connection refused",
            "connection reset",
            "connection aborted",
            "broken pipe",
            "transport error",
            "connection closed",
            "network error",
            "network unreachable",
            "error sending request",
            "request failed",
            "temporary failure",
            "temporarily unavailable",
            "service unavailable",
            "internal server error",
            "500",
            "503",
            "504",
            "502",
            "bad gateway",
            "gateway timeout",
            "overloaded",
            "capacity",
            "please try again",
        ],
    ) {
        return ExecutionErrorClassification::new(
            ExecutionErrorKind::Timeout,
            RetryClass::Retryable,
        );
    }

    if contains_any(
        &lower,
        &[
            "bad request",
            "invalid request",
            "validation error",
            "invalid model",
            "model not found",
            "configuration error",
            "not found",
            "404",
            "400",
        ],
    ) {
        return ExecutionErrorClassification::new(
            ExecutionErrorKind::Validation,
            RetryClass::NonRetryable,
        );
    }

    if contains_any(&lower, &["tool error", "tool not found"]) {
        return ExecutionErrorClassification::new(
            ExecutionErrorKind::Tool,
            RetryClass::NonRetryable,
        );
    }

    ExecutionErrorClassification::new(ExecutionErrorKind::Internal, RetryClass::NonRetryable)
}

pub fn classify_tool_output_failure(output: &ToolOutput) -> ExecutionFailure {
    let classification = match output.error_category {
        Some(ToolErrorCategory::Auth) => ExecutionErrorClassification::new(
            ExecutionErrorKind::Authentication,
            RetryClass::NonRetryable,
        ),
        Some(ToolErrorCategory::RateLimit) => ExecutionErrorClassification::new(
            ExecutionErrorKind::RateLimited,
            RetryClass::Retryable,
        ),
        Some(ToolErrorCategory::Network) => {
            ExecutionErrorClassification::new(ExecutionErrorKind::Timeout, RetryClass::Retryable)
        }
        Some(ToolErrorCategory::Config | ToolErrorCategory::NotFound) => {
            ExecutionErrorClassification::new(
                ExecutionErrorKind::Validation,
                RetryClass::NonRetryable,
            )
        }
        Some(ToolErrorCategory::Execution) | None => {
            if output.retryable.unwrap_or(false) {
                ExecutionErrorClassification::new(ExecutionErrorKind::Tool, RetryClass::Retryable)
            } else {
                ExecutionErrorClassification::new(
                    ExecutionErrorKind::Tool,
                    RetryClass::NonRetryable,
                )
            }
        }
    };

    ExecutionFailure {
        message: output
            .error
            .clone()
            .unwrap_or_else(|| "Tool execution failed".to_string()),
        classification,
        cause: None,
    }
}

pub fn is_retryable_classification(classification: ExecutionErrorClassification) -> bool {
    matches!(classification.retry_class, RetryClass::Retryable)
}

pub fn is_authentication_classification(classification: ExecutionErrorClassification) -> bool {
    matches!(classification.kind, ExecutionErrorKind::Authentication)
}

fn classify_ai_error(error: &AiError) -> ExecutionErrorClassification {
    match error {
        AiError::LlmHttp { status, .. } => match status {
            401 | 403 => ExecutionErrorClassification::new(
                ExecutionErrorKind::Authentication,
                RetryClass::NonRetryable,
            ),
            429 => ExecutionErrorClassification::new(
                ExecutionErrorKind::RateLimited,
                RetryClass::Retryable,
            ),
            502..=504 => ExecutionErrorClassification::new(
                ExecutionErrorKind::Timeout,
                RetryClass::Retryable,
            ),
            400 | 404 | 422 => ExecutionErrorClassification::new(
                ExecutionErrorKind::Validation,
                RetryClass::NonRetryable,
            ),
            _ => ExecutionErrorClassification::new(
                ExecutionErrorKind::Model,
                RetryClass::NonRetryable,
            ),
        },
        AiError::Http(error) if error.is_timeout() || error.is_connect() => {
            ExecutionErrorClassification::new(ExecutionErrorKind::Timeout, RetryClass::Retryable)
        }
        AiError::Http(_) => ExecutionErrorClassification::new(
            ExecutionErrorKind::Internal,
            RetryClass::NonRetryable,
        ),
        AiError::Tool(_) | AiError::ToolNotFound(_) => {
            ExecutionErrorClassification::new(ExecutionErrorKind::Tool, RetryClass::NonRetryable)
        }
        AiError::InvalidFormat(_) => ExecutionErrorClassification::new(
            ExecutionErrorKind::Validation,
            RetryClass::NonRetryable,
        ),
        AiError::MaxIterations(_) => ExecutionErrorClassification::new(
            ExecutionErrorKind::Internal,
            RetryClass::NonRetryable,
        ),
        AiError::Agent(message) | AiError::Llm(message) => {
            classify_execution_error_message(message)
        }
        AiError::Json(_) | AiError::Io(_) => ExecutionErrorClassification::new(
            ExecutionErrorKind::Internal,
            RetryClass::NonRetryable,
        ),
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_authentication_messages() {
        let classification = classify_execution_error_message("HTTP 401 unauthorized");
        assert_eq!(classification.kind, ExecutionErrorKind::Authentication);
        assert_eq!(classification.retry_class, RetryClass::NonRetryable);
    }

    #[test]
    fn classifies_rate_limit_messages() {
        let classification = classify_execution_error_message("429 rate limit exceeded");
        assert_eq!(classification.kind, ExecutionErrorKind::RateLimited);
        assert_eq!(classification.retry_class, RetryClass::Retryable);
    }

    #[test]
    fn classifies_tool_output_failure_from_category() {
        let failure = classify_tool_output_failure(&ToolOutput::non_retryable_error(
            "missing config",
            ToolErrorCategory::Config,
        ));
        assert_eq!(failure.classification.kind, ExecutionErrorKind::Validation);
        assert_eq!(failure.classification.retry_class, RetryClass::NonRetryable);
    }

    #[test]
    fn classifies_timeout_messages() {
        let classification = classify_execution_error_message("503 Service Unavailable");
        assert_eq!(classification.kind, ExecutionErrorKind::Timeout);
        assert_eq!(classification.retry_class, RetryClass::Retryable);
    }

    #[test]
    fn classifies_reqwest_send_failures_as_timeout() {
        let classification = classify_execution_error_message(
            "LLM error: Request failed: error sending request for url (https://api.minimax.io/anthropic/v1/messages)",
        );
        assert_eq!(classification.kind, ExecutionErrorKind::Timeout);
        assert_eq!(classification.retry_class, RetryClass::Retryable);
    }

    #[test]
    fn classifies_validation_messages() {
        let classification = classify_execution_error_message("400 Bad Request");
        assert_eq!(classification.kind, ExecutionErrorKind::Validation);
        assert_eq!(classification.retry_class, RetryClass::NonRetryable);
    }

    #[test]
    fn classifies_interrupt_messages() {
        let classification = classify_execution_error_message("Execution interrupted by user");
        assert_eq!(classification.kind, ExecutionErrorKind::UserInterrupted);
        assert_eq!(classification.retry_class, RetryClass::NonRetryable);
    }

    #[test]
    fn classifies_unknown_messages_as_internal() {
        let classification = classify_execution_error_message("unexpected panic");
        assert_eq!(classification.kind, ExecutionErrorKind::Internal);
        assert_eq!(classification.retry_class, RetryClass::NonRetryable);
    }
}
