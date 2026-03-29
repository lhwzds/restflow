use std::sync::Arc;

use crate::{
    DEFAULT_TELEMETRY_TEXT_LIMIT, ExecutionEvent, ExecutionEventEnvelope, RestflowTrace,
    TelemetryContext, TelemetrySink, sanitize_telemetry_secrets, truncate_telemetry_text,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    Interactive,
    BackgroundTask,
    Subagent,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunDescriptor {
    pub kind: RunKind,
    pub run_id: String,
    pub actor_id: String,
    pub parent_run_id: Option<String>,
    pub session_id: String,
    pub scope_id: String,
}

impl RunDescriptor {
    pub fn new(
        kind: RunKind,
        run_id: impl Into<String>,
        session_id: impl Into<String>,
        scope_id: impl Into<String>,
        actor_id: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            run_id: run_id.into(),
            actor_id: actor_id.into(),
            parent_run_id: None,
            session_id: session_id.into(),
            scope_id: scope_id.into(),
        }
    }

    pub fn with_parent_run_id(mut self, parent_run_id: Option<String>) -> Self {
        self.parent_run_id = parent_run_id;
        self
    }

    pub fn trace(&self) -> RestflowTrace {
        RestflowTrace::from_run(
            self.run_id.clone(),
            self.actor_id.clone(),
            self.parent_run_id.clone(),
            Some(self.session_id.clone()),
            Some(self.scope_id.clone()),
        )
    }
}

#[derive(Clone, Default)]
pub struct RunLifecycleService {
    sink: Option<Arc<dyn TelemetrySink>>,
}

impl RunLifecycleService {
    pub fn new(sink: Arc<dyn TelemetrySink>) -> Self {
        Self { sink: Some(sink) }
    }

    pub fn noop() -> Self {
        Self { sink: None }
    }

    pub fn with_optional_sink(sink: Option<Arc<dyn TelemetrySink>>) -> Self {
        Self { sink }
    }

    pub fn handle(&self, descriptor: RunDescriptor) -> RunHandle {
        RunHandle::new(self.sink.clone(), descriptor)
    }
}

#[derive(Clone)]
pub struct RunHandle {
    sink: Option<Arc<dyn TelemetrySink>>,
    descriptor: RunDescriptor,
    telemetry_context: TelemetryContext,
}

impl RunHandle {
    pub fn new(sink: Option<Arc<dyn TelemetrySink>>, descriptor: RunDescriptor) -> Self {
        let telemetry_context = TelemetryContext::new(descriptor.trace());
        Self {
            sink,
            descriptor,
            telemetry_context,
        }
    }

    pub fn descriptor(&self) -> &RunDescriptor {
        &self.descriptor
    }

    pub fn run_id(&self) -> &str {
        &self.descriptor.run_id
    }

    pub fn parent_run_id(&self) -> Option<&str> {
        self.descriptor.parent_run_id.as_deref()
    }

    pub fn session_id(&self) -> &str {
        &self.descriptor.session_id
    }

    pub fn scope_id(&self) -> &str {
        &self.descriptor.scope_id
    }

    pub fn actor_id(&self) -> &str {
        &self.descriptor.actor_id
    }

    pub fn context(&self) -> &TelemetryContext {
        &self.telemetry_context
    }

    pub fn cloned_context(&self) -> TelemetryContext {
        self.telemetry_context.clone()
    }

    pub fn with_requested_model(&self, requested_model: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.telemetry_context = next.telemetry_context.clone().with_requested_model(requested_model);
        next
    }

    pub fn with_effective_model(&self, effective_model: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.telemetry_context = next.telemetry_context.clone().with_effective_model(effective_model);
        next
    }

    pub fn with_provider(&self, provider: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.telemetry_context = next.telemetry_context.clone().with_provider(provider);
        next
    }

    pub fn child_descriptor(
        &self,
        kind: RunKind,
        run_id: impl Into<String>,
        actor_id: impl Into<String>,
        session_id: Option<String>,
        scope_id: Option<String>,
    ) -> RunDescriptor {
        RunDescriptor::new(
            kind,
            run_id,
            session_id.unwrap_or_else(|| self.session_id().to_string()),
            scope_id.unwrap_or_else(|| self.scope_id().to_string()),
            actor_id,
        )
        .with_parent_run_id(Some(self.run_id().to_string()))
    }

    pub async fn start(&self) {
        self.emit(ExecutionEvent::RunStarted).await;
    }

    pub async fn complete(&self, ai_duration_ms: Option<u64>) {
        self.emit(ExecutionEvent::RunCompleted { ai_duration_ms }).await;
    }

    pub async fn fail(&self, error: impl AsRef<str>, ai_duration_ms: Option<u64>) {
        let sanitized_error = truncate_telemetry_text(
            &sanitize_telemetry_secrets(error.as_ref()),
            DEFAULT_TELEMETRY_TEXT_LIMIT,
        );
        self.emit(ExecutionEvent::RunFailed {
            error: sanitized_error,
            ai_duration_ms,
        })
        .await;
    }

    pub async fn interrupt(&self, reason: impl AsRef<str>, ai_duration_ms: Option<u64>) {
        let sanitized_reason = truncate_telemetry_text(
            &sanitize_telemetry_secrets(reason.as_ref()),
            DEFAULT_TELEMETRY_TEXT_LIMIT,
        );
        self.emit(ExecutionEvent::RunInterrupted {
            reason: sanitized_reason,
            ai_duration_ms,
        })
        .await;
    }

    pub async fn emit_model_switch(
        &self,
        from_model: impl Into<String>,
        to_model: impl Into<String>,
        reason: Option<String>,
        success: bool,
        context: Option<&TelemetryContext>,
    ) {
        let event = ExecutionEvent::ModelSwitch {
            from_model: from_model.into(),
            to_model: to_model.into(),
            reason,
            success,
        };
        if let Some(sink) = &self.sink {
            sink.emit(ExecutionEventEnvelope::from_telemetry_context(
                context.unwrap_or(&self.telemetry_context),
                event,
            ))
            .await;
        }
    }

    async fn emit(&self, event: ExecutionEvent) {
        if let Some(sink) = &self.sink {
            sink.emit(ExecutionEventEnvelope::from_telemetry_context(
                &self.telemetry_context,
                event,
            ))
            .await;
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunAttemptTracker<T> {
    previous_value: Option<T>,
    next_attempt: u32,
}

impl<T> Default for RunAttemptTracker<T> {
    fn default() -> Self {
        Self {
            previous_value: None,
            next_attempt: 1,
        }
    }
}

impl<T> RunAttemptTracker<T>
where
    T: Clone,
{
    pub fn register_attempt(&mut self, current_value: T) -> (u32, Option<T>) {
        let attempt = self.next_attempt;
        self.next_attempt = self.next_attempt.saturating_add(1);
        let previous_value = self.previous_value.replace(current_value);
        (attempt, previous_value)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use crate::{ExecutionEvent, ExecutionEventEnvelope, TelemetrySink};

    use super::{RunAttemptTracker, RunDescriptor, RunKind, RunLifecycleService};

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<ExecutionEventEnvelope>>,
    }

    #[async_trait::async_trait]
    impl TelemetrySink for RecordingSink {
        async fn emit(&self, event: ExecutionEventEnvelope) {
            self.events.lock().await.push(event);
        }
    }

    #[tokio::test]
    async fn run_handle_emits_started_and_completed_events() {
        let sink = Arc::new(RecordingSink::default());
        let service = RunLifecycleService::new(sink.clone());
        let handle = service.handle(RunDescriptor::new(
            RunKind::Interactive,
            "run-1",
            "session-1",
            "scope-1",
            "agent-1",
        ));

        handle.start().await;
        handle.complete(Some(42)).await;

        let events = sink.events.lock().await.clone();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].event, ExecutionEvent::RunStarted));
        assert!(matches!(
            events[1].event,
            ExecutionEvent::RunCompleted {
                ai_duration_ms: Some(42)
            }
        ));
    }

    #[test]
    fn attempt_tracker_increments_monotonically() {
        let mut tracker = RunAttemptTracker::default();
        let (first_attempt, first_previous) = tracker.register_attempt("gpt-5");
        let (second_attempt, second_previous) = tracker.register_attempt("claude");

        assert_eq!(first_attempt, 1);
        assert_eq!(first_previous, None);
        assert_eq!(second_attempt, 2);
        assert_eq!(second_previous, Some("gpt-5"));
    }

    #[test]
    fn child_descriptor_inherits_parent_scope() {
        let handle = RunLifecycleService::noop().handle(
            RunDescriptor::new(
                RunKind::Interactive,
                "parent-run",
                "session-1",
                "scope-1",
                "agent-1",
            )
            .with_parent_run_id(Some("root".to_string())),
        );

        let child = handle.child_descriptor(
            RunKind::Subagent,
            "child-run",
            "agent-child",
            None,
            None,
        );

        assert_eq!(child.parent_run_id.as_deref(), Some("parent-run"));
        assert_eq!(child.session_id, "session-1");
        assert_eq!(child.scope_id, "scope-1");
    }
}
