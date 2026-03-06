use crate::agent::stream::StreamEmitter;

/// Context describing a traced run execution.
#[derive(Debug, Clone)]
pub struct RunTraceContext {
    pub run_id: String,
    pub actor_id: String,
    pub parent_run_id: Option<String>,
}

/// Outcome for traced run completion.
#[derive(Debug, Clone)]
pub struct RunTraceOutcome {
    pub success: bool,
    pub error: Option<String>,
}

/// Optional sink for run trace lifecycle and tool-call events.
pub trait RunTraceSink: Send + Sync {
    fn on_run_started(&self, context: &RunTraceContext);
    fn build_run_emitter(&self, context: &RunTraceContext) -> Box<dyn StreamEmitter>;
    fn on_run_finished(&self, context: &RunTraceContext, outcome: &RunTraceOutcome);
}
