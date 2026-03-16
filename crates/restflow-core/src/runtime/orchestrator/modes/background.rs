use anyhow::Result;
use serde_json::json;
use tokio::sync::mpsc;

use crate::models::{MemoryConfig, SteerMessage};
use crate::runtime::background_agent::ExecutionResult;
use crate::runtime::orchestrator::kernel::{
    ExecutionKernel, map_anyhow_error, parse_optional_metadata,
};
use restflow_ai::AgentState;
use restflow_ai::agent::StreamEmitter;
use restflow_traits::{ExecutionOutcome, ExecutionPlan};

pub struct BackgroundExecutionRequest {
    pub agent_id: String,
    pub background_task_id: Option<String>,
    pub input: Option<String>,
    pub memory_config: MemoryConfig,
    pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    pub emitter: Option<Box<dyn StreamEmitter>>,
    pub state: Option<AgentState>,
}

pub async fn run_with_request(
    kernel: &ExecutionKernel,
    request: BackgroundExecutionRequest,
) -> Result<ExecutionResult> {
    if let Some(state) = request.state {
        kernel
            .backend()
            .execute_background_from_state(
                &request.agent_id,
                request.background_task_id.as_deref(),
                state,
                &request.memory_config,
                request.steer_rx,
                request.emitter,
            )
            .await
    } else {
        kernel
            .backend()
            .execute_background(
                &request.agent_id,
                request.background_task_id.as_deref(),
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
        restflow_traits::ToolError::Tool("Background execution requires 'agent_id'.".to_string())
    })?;
    let memory_config =
        parse_optional_metadata::<MemoryConfig>(&plan, "memory_config")?.unwrap_or_default();
    let state = parse_optional_metadata::<AgentState>(&plan, "agent_state")?;

    let request = BackgroundExecutionRequest {
        agent_id,
        background_task_id: plan.background_task_id.clone(),
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
                    "background_task_id": plan.background_task_id,
                })),
                ..ExecutionOutcome::default()
            }
        })
        .map_err(map_anyhow_error)
}
