use super::AgentResult;

/// Stream step emitted during agent execution.
#[derive(Debug)]
pub enum ExecutionStep {
    // Lifecycle
    Started {
        execution_id: String,
    },
    IterationBegin {
        iteration: usize,
    },
    // LLM streaming
    TextDelta {
        content: String,
    },
    ThinkingDelta {
        content: String,
    },
    // Tool execution
    ToolCallStart {
        id: String,
        name: String,
        arguments: String,
    },
    ToolCallResult {
        id: String,
        name: String,
        result: String,
        success: bool,
    },
    // Completion
    Completed {
        result: Box<AgentResult>,
    },
    Failed {
        error: String,
    },
    // Guardrails
    StuckDetected {
        tool: String,
        repeat_count: usize,
    },
    ResourceWarning {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentState, ResourceUsage};
    use std::time::Duration;

    fn sample_result() -> AgentResult {
        AgentResult {
            success: true,
            answer: Some("done".to_string()),
            error: None,
            iterations: 1,
            total_tokens: 0,
            total_cost_usd: 0.0,
            state: AgentState::new("execution-1".to_string(), 1),
            compaction_results: Vec::new(),
            resource_usage: ResourceUsage {
                tool_calls: 0,
                wall_clock: Duration::ZERO,
                depth: 0,
            },
        }
    }

    #[test]
    fn test_execution_step_variants() {
        let steps = vec![
            ExecutionStep::Started {
                execution_id: "exec-1".to_string(),
            },
            ExecutionStep::IterationBegin { iteration: 1 },
            ExecutionStep::TextDelta {
                content: "text".to_string(),
            },
            ExecutionStep::ThinkingDelta {
                content: "thinking".to_string(),
            },
            ExecutionStep::ToolCallStart {
                id: "call_1".to_string(),
                name: "echo".to_string(),
                arguments: "{}".to_string(),
            },
            ExecutionStep::ToolCallResult {
                id: "call_1".to_string(),
                name: "echo".to_string(),
                result: "{\"ok\":true}".to_string(),
                success: true,
            },
            ExecutionStep::Completed {
                result: Box::new(sample_result()),
            },
            ExecutionStep::Failed {
                error: "failure".to_string(),
            },
            ExecutionStep::StuckDetected {
                tool: "echo".to_string(),
                repeat_count: 3,
            },
            ExecutionStep::ResourceWarning {
                message: "limit near".to_string(),
            },
        ];

        assert_eq!(steps.len(), 10);
    }
}
