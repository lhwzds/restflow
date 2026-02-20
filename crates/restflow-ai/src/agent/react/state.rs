//! Agent state machine for ReAct loop.

/// Current state of the agent execution
#[derive(Debug, Clone)]
pub enum AgentState {
    Ready,
    Thinking,
    Acting { tool: String },
    Observing,
    Completed { output: String },
    Failed { error: String },
}
