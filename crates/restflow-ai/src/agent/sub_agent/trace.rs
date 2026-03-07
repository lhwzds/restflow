use crate::agent::stream::StreamEmitter;
pub use restflow_trace::{RunTraceContext, RunTraceOutcome};

/// Optional sink for run trace lifecycle and tool-call events.
pub trait RunTraceSink: Send + Sync {
    fn on_run_started(&self, context: &RunTraceContext);
    fn build_run_emitter(&self, context: &RunTraceContext) -> Box<dyn StreamEmitter>;
    fn on_run_finished(&self, context: &RunTraceContext, outcome: &RunTraceOutcome);
}
