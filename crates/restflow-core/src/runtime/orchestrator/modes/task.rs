use anyhow::Result;
use serde_json::json;
use tokio::sync::mpsc;

use crate::models::{MemoryConfig, SteerMessage};
use crate::runtime::orchestrator::kernel::{
    ExecutionKernel, map_anyhow_error, parse_optional_metadata,
};
use crate::runtime::task_runtime::ExecutionResult;
use restflow_ai::AgentState;
use restflow_ai::agent::StreamEmitter;
use restflow_traits::{ExecutionOutcome, ExecutionPlan};

pub struct TaskExecutionRequest {
    pub agent_id: String,
    pub task_id: Option<String>,
    pub input: Option<String>,
    pub memory_config: MemoryConfig,
    pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    pub emitter: Option<Box<dyn StreamEmitter>>,
    pub state: Option<AgentState>,
}

pub async fn run_with_request(
    kernel: &ExecutionKernel,
    request: TaskExecutionRequest,
) -> Result<ExecutionResult> {
    if let Some(state) = request.state {
        kernel
            .backend()
            .execute_task_from_state(
                &request.agent_id,
                request.task_id.as_deref(),
                state,
                &request.memory_config,
                request.steer_rx,
                request.emitter,
            )
            .await
    } else {
        kernel
            .backend()
            .execute_task(
                &request.agent_id,
                request.task_id.as_deref(),
                request.input.as_deref(),
                &request.memory_config,
                request.steer_rx,
                request.emitter,
            )
            .await
    }
}

pub async fn run_plan(
    kernel: &ExecutionKernel,
    plan: ExecutionPlan,
) -> std::result::Result<ExecutionOutcome, restflow_traits::ToolError> {
    let agent_id = plan.agent_id.clone().ok_or_else(|| {
        restflow_traits::ToolError::Tool("Task execution requires 'agent_id'.".to_string())
    })?;
    let memory_config =
        parse_optional_metadata::<MemoryConfig>(&plan, "memory_config")?.unwrap_or_default();
    let state = parse_optional_metadata::<AgentState>(&plan, "agent_state")?;

    let request = TaskExecutionRequest {
        agent_id,
        task_id: plan.task_id.clone(),
        input: plan.input.clone(),
        memory_config,
        steer_rx: None,
        emitter: None,
        state,
    };

    run_with_request(kernel, request)
        .await
        .map(|result| {
            let compaction = result.metrics.compaction.as_ref().map(|metrics| {
                json!({
                    "event_count": metrics.event_count,
                    "tokens_before": metrics.tokens_before,
                    "tokens_after": metrics.tokens_after,
                    "messages_compacted": metrics.messages_compacted,
                })
            });
            ExecutionOutcome {
                success: result.success,
                text: Some(result.output),
                metadata: Some(json!({
                    "message_count": result.messages.len(),
                    "compaction": compaction,
                    "task_id": plan.task_id,
                })),
                ..ExecutionOutcome::default()
            }
        })
        .map_err(map_anyhow_error)
}
