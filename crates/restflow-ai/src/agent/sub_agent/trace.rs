use crate::agent::stream::StreamEmitter;
pub use restflow_telemetry::{RunTraceContext, RunTraceLifecycleSink, RunTraceOutcome};

/// AI-specific factory for wrapping a stream emitter with trace persistence.
pub trait RunTraceEmitterFactory: Send + Sync {
    fn build_run_emitter(&self, context: &RunTraceContext) -> Box<dyn StreamEmitter>;
}

/// Optional sink for run trace lifecycle and tool-call events.
pub trait RunTraceSink: RunTraceLifecycleSink + RunTraceEmitterFactory {}

impl<T> RunTraceSink for T where T: RunTraceLifecycleSink + RunTraceEmitterFactory + ?Sized {}
