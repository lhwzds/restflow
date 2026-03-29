use anyhow::Result;
use serde_json::json;
use tokio::sync::mpsc;

use crate::models::{ChatSession, SteerMessage};
use crate::runtime::background_agent::{
    SessionExecutionResult, SessionInputMode, SessionTurnRuntimeOptions,
};
use crate::runtime::orchestrator::kernel::{
    ExecutionKernel, map_anyhow_error, parse_optional_metadata, require_mode_input,
};
use restflow_ai::agent::StreamEmitter;
use restflow_traits::{ExecutionOutcome, ExecutionPlan};

#[derive(Debug, Clone)]
pub struct InteractiveExecutionResult {
    pub session: ChatSession,
    pub execution: SessionExecutionResult,
    pub outcome: ExecutionOutcome,
}

pub async fn run_with_session(
    kernel: &ExecutionKernel,
    session: &mut ChatSession,
    user_input: &str,
    max_history: usize,
    input_mode: SessionInputMode,
    emitter: Option<Box<dyn StreamEmitter>>,
    steer_rx: Option<mpsc::Receiver<SteerMessage>>,
) -> Result<InteractiveExecutionResult> {
    run_with_session_options(
        kernel,
        session,
        user_input,
        max_history,
        input_mode,
        emitter,
        SessionTurnRuntimeOptions {
            steer_rx,
            telemetry_context: None,
        },
    )
    .await
}

pub async fn run_with_session_options(
    kernel: &ExecutionKernel,
    session: &mut ChatSession,
    user_input: &str,
    max_history: usize,
    input_mode: SessionInputMode,
    emitter: Option<Box<dyn StreamEmitter>>,
    options: SessionTurnRuntimeOptions,
) -> Result<InteractiveExecutionResult> {
    let execution = kernel
        .backend()
        .execute_interactive_session_turn(
            session,
            user_input,
            max_history,
            input_mode,
            emitter,
            options,
        )
        .await?;
    let outcome = ExecutionOutcome {
        success: true,
        text: Some(execution.output.clone()),
        iterations: Some(execution.iterations),
        model: Some(execution.final_model.as_serialized_str().to_string()),
        metadata: Some(json!({
            "chat_session_id": session.id,
            "resolved_agent_id": session.agent_id,
        })),
        ..ExecutionOutcome::default()
    };

    Ok(InteractiveExecutionResult {
        session: session.clone(),
        execution,
        outcome,
    })
}

pub async fn run_plan(
    kernel: &ExecutionKernel,
    plan: ExecutionPlan,
) -> std::result::Result<ExecutionOutcome, restflow_traits::ToolError> {
    let session_id = plan.chat_session_id.as_deref().ok_or_else(|| {
        restflow_traits::ToolError::Tool(
            "Interactive execution requires 'chat_session_id'.".to_string(),
        )
    })?;
    let mut session = kernel
        .backend()
        .load_chat_session(session_id)
        .map_err(map_anyhow_error)?;
    let input = require_mode_input(&plan, "input")?;
    let max_history = parse_optional_metadata::<usize>(&plan, "max_history")?
        .unwrap_or(restflow_traits::DEFAULT_CHAT_MAX_SESSION_HISTORY);
    let input_mode = parse_optional_metadata::<SessionInputModeWrapper>(&plan, "input_mode")?
        .map(Into::into)
        .unwrap_or(SessionInputMode::EphemeralInput);

    run_with_session(
        kernel,
        &mut session,
        input,
        max_history,
        input_mode,
        None,
        None,
    )
    .await
    .map(|result| result.outcome)
    .map_err(map_anyhow_error)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum SessionInputModeWrapper {
    PersistedInSession,
    EphemeralInput,
}

impl From<SessionInputModeWrapper> for SessionInputMode {
    fn from(value: SessionInputModeWrapper) -> Self {
        match value {
            SessionInputModeWrapper::PersistedInSession => SessionInputMode::PersistedInSession,
            SessionInputModeWrapper::EphemeralInput => SessionInputMode::EphemeralInput,
        }
    }
}
