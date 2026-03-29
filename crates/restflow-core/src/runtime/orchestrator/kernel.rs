use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::models::{ChatSession, MemoryConfig, SteerMessage};
use crate::runtime::background_agent::{
    AgentExecutor as BackgroundAgentExecutor, AgentRuntimeExecutor, ExecutionResult,
    SessionExecutionResult, SessionInputMode, SessionTurnRuntimeOptions,
};
use restflow_ai::AgentState;
use restflow_ai::agent::StreamEmitter;
use restflow_traits::{ExecutionOutcome, ExecutionPlan};

#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    fn load_chat_session(&self, session_id: &str) -> Result<ChatSession>;

    fn prepare_interactive_session(&self, _session: &mut ChatSession) -> Result<()> {
        Ok(())
    }

    async fn execute_interactive_session_turn(
        &self,
        session: &mut ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
        emitter: Option<Box<dyn StreamEmitter>>,
        options: SessionTurnRuntimeOptions,
    ) -> Result<SessionExecutionResult>;

    async fn execute_background(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult>;

    async fn execute_background_from_state(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult>;

    async fn execute_subagent_plan(&self, plan: ExecutionPlan) -> Result<ExecutionOutcome>;
}

#[derive(Clone)]
pub struct ExecutionKernel {
    backend: Arc<dyn ExecutionBackend>,
}

impl ExecutionKernel {
    pub fn new(backend: Arc<dyn ExecutionBackend>) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> Arc<dyn ExecutionBackend> {
        self.backend.clone()
    }
}

#[async_trait]
impl ExecutionBackend for AgentRuntimeExecutor {
    fn load_chat_session(&self, session_id: &str) -> Result<ChatSession> {
        self.load_chat_session(session_id)
    }

    fn prepare_interactive_session(&self, session: &mut ChatSession) -> Result<()> {
        let _ = self.resolve_stored_agent_for_session(session)?;
        Ok(())
    }

    async fn execute_interactive_session_turn(
        &self,
        session: &mut ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
        emitter: Option<Box<dyn StreamEmitter>>,
        options: SessionTurnRuntimeOptions,
    ) -> Result<SessionExecutionResult> {
        self.execute_session_turn_with_emitter_and_steer(
            session,
            user_input,
            max_history,
            input_mode,
            emitter,
            options,
        )
        .await
    }

    async fn execute_background(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        BackgroundAgentExecutor::execute_with_emitter(
            self,
            agent_id,
            background_task_id,
            input,
            memory_config,
            steer_rx,
            emitter,
        )
        .await
    }

    async fn execute_background_from_state(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        BackgroundAgentExecutor::execute_from_state(
            self,
            agent_id,
            background_task_id,
            state,
            memory_config,
            steer_rx,
            emitter,
        )
        .await
    }

    async fn execute_subagent_plan(&self, plan: ExecutionPlan) -> Result<ExecutionOutcome> {
        self.execute_subagent_plan(plan).await
    }
}

pub fn parse_optional_metadata<T: serde::de::DeserializeOwned>(
    plan: &restflow_traits::ExecutionPlan,
    field: &str,
) -> std::result::Result<Option<T>, restflow_traits::ToolError> {
    let Some(metadata) = plan.metadata.as_ref() else {
        return Ok(None);
    };
    let Some(value) = metadata.get(field) else {
        return Ok(None);
    };

    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|error| {
            restflow_traits::ToolError::Tool(format!("Invalid '{field}' metadata: {error}"))
        })
}

pub fn require_mode_input<'a>(
    plan: &'a restflow_traits::ExecutionPlan,
    field: &'static str,
) -> std::result::Result<&'a str, restflow_traits::ToolError> {
    plan.input
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            restflow_traits::ToolError::Tool(format!(
                "Execution plan requires non-empty '{field}'."
            ))
        })
}

pub fn map_anyhow_error(error: anyhow::Error) -> restflow_traits::ToolError {
    restflow_traits::ToolError::Tool(error.to_string())
}
