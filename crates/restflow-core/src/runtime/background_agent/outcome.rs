use restflow_ai::llm::Message;
use restflow_telemetry::TelemetryContext;

use crate::models::ModelId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionErrorKind {
    Authentication,
    RateLimited,
    Timeout,
    Tool,
    Model,
    Validation,
    Internal,
    UserInterrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    Retryable,
    NonRetryable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionErrorClassification {
    pub kind: ExecutionErrorKind,
    pub retry_class: RetryClass,
}

impl ExecutionErrorClassification {
    pub const fn new(kind: ExecutionErrorKind, retry_class: RetryClass) -> Self {
        Self { kind, retry_class }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompactionMetrics {
    pub event_count: u32,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub messages_compacted: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecutionMetrics {
    pub iterations: Option<u32>,
    pub active_model: Option<String>,
    pub final_model: Option<ModelId>,
    pub message_count: usize,
    pub compaction: Option<CompactionMetrics>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFailure {
    pub message: String,
    pub classification: ExecutionErrorClassification,
    pub cause: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionOutcome {
    pub output: String,
    pub messages: Vec<Message>,
    pub success: bool,
    pub metrics: ExecutionMetrics,
    pub failure: Option<ExecutionFailure>,
}

impl ExecutionOutcome {
    pub fn success(output: String, messages: Vec<Message>) -> Self {
        let metrics = ExecutionMetrics {
            message_count: messages.len(),
            ..ExecutionMetrics::default()
        };
        Self {
            output,
            messages,
            success: true,
            metrics,
            failure: None,
        }
    }

    pub fn success_with_compaction(
        output: String,
        messages: Vec<Message>,
        compaction: CompactionMetrics,
    ) -> Self {
        let message_count = messages.len();
        Self {
            output,
            messages,
            success: true,
            metrics: ExecutionMetrics {
                message_count,
                compaction: Some(compaction),
                ..ExecutionMetrics::default()
            },
            failure: None,
        }
    }

    pub fn failure(
        message: impl Into<String>,
        classification: ExecutionErrorClassification,
        cause: Option<String>,
    ) -> Self {
        let message = message.into();
        Self {
            output: message.clone(),
            messages: Vec::new(),
            success: false,
            metrics: ExecutionMetrics::default(),
            failure: Some(ExecutionFailure {
                message,
                classification,
                cause,
            }),
        }
    }

    pub fn with_metrics(mut self, metrics: ExecutionMetrics) -> Self {
        self.metrics = metrics;
        self
    }
}

#[derive(Debug, Clone)]
pub struct SessionExecutionResult {
    pub output: String,
    pub iterations: u32,
    pub active_model: String,
    pub final_model: ModelId,
    pub metrics: ExecutionMetrics,
    pub final_telemetry_context: Option<TelemetryContext>,
}

impl SessionExecutionResult {
    pub fn new(
        output: String,
        iterations: u32,
        active_model: String,
        final_model: ModelId,
    ) -> Self {
        Self {
            output,
            iterations,
            active_model: active_model.clone(),
            final_model,
            metrics: ExecutionMetrics {
                iterations: Some(iterations),
                active_model: Some(active_model),
                final_model: Some(final_model),
                ..ExecutionMetrics::default()
            },
            final_telemetry_context: None,
        }
    }

    pub fn with_final_telemetry_context(mut self, telemetry_context: TelemetryContext) -> Self {
        self.final_telemetry_context = Some(telemetry_context);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_outcome_sets_message_count() {
        let outcome = ExecutionOutcome::success("ok".to_string(), Vec::new());
        assert!(outcome.success);
        assert_eq!(outcome.metrics.message_count, 0);
        assert!(outcome.failure.is_none());
    }

    #[test]
    fn failure_outcome_captures_classification() {
        let outcome = ExecutionOutcome::failure(
            "boom",
            ExecutionErrorClassification::new(
                ExecutionErrorKind::Internal,
                RetryClass::NonRetryable,
            ),
            Some("panic".to_string()),
        );
        assert!(!outcome.success);
        let failure = outcome.failure.expect("failure");
        assert_eq!(failure.message, "boom");
        assert_eq!(failure.cause.as_deref(), Some("panic"));
        assert_eq!(failure.classification.kind, ExecutionErrorKind::Internal);
    }

    #[test]
    fn session_execution_result_populates_metrics() {
        let result =
            SessionExecutionResult::new("ok".to_string(), 3, "gpt-5".to_string(), ModelId::Gpt5);
        assert_eq!(result.metrics.iterations, Some(3));
        assert_eq!(result.metrics.active_model.as_deref(), Some("gpt-5"));
        assert_eq!(result.metrics.final_model, Some(ModelId::Gpt5));
        assert_eq!(result.final_model, ModelId::Gpt5);
        assert!(result.final_telemetry_context.is_none());
    }
}
