use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::models::{ChatSession, MemoryConfig, SteerMessage};
use crate::runtime::background_agent::{
    AgentExecutor, AgentRuntimeExecutor, ExecutionResult, SessionInputMode,
};
use crate::runtime::orchestrator::kernel::{ExecutionBackend, ExecutionKernel};
use crate::runtime::orchestrator::modes::{background, interactive, subagent};
use restflow_ai::AgentState;
use restflow_ai::agent::StreamEmitter;
use restflow_traits::{AgentOrchestrator, ExecutionOutcome, ExecutionPlan, ToolError};

#[derive(Clone)]
pub struct AgentOrchestratorImpl {
    kernel: Arc<ExecutionKernel>,
}

impl AgentOrchestratorImpl {
    pub fn new(backend: Arc<dyn ExecutionBackend>) -> Self {
        Self {
            kernel: Arc::new(ExecutionKernel::new(backend)),
        }
    }

    pub fn from_runtime_executor(executor: AgentRuntimeExecutor) -> Self {
        Self::new(Arc::new(executor))
    }

    pub async fn run_interactive_session_turn(
        &self,
        session: &mut ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
        emitter: Option<Box<dyn StreamEmitter>>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<interactive::InteractiveExecutionResult> {
        interactive::run_with_session(
            self.kernel.as_ref(),
            session,
            user_input,
            max_history,
            input_mode,
            emitter,
            steer_rx,
        )
        .await
    }

    pub async fn run_background_execution(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        background::run_with_request(
            self.kernel.as_ref(),
            background::BackgroundExecutionRequest {
                agent_id: agent_id.to_string(),
                background_task_id: background_task_id.map(ToOwned::to_owned),
                input: input.map(ToOwned::to_owned),
                memory_config: memory_config.clone(),
                steer_rx,
                emitter,
                state: None,
            },
        )
        .await
    }

    pub async fn run_background_execution_from_state(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        background::run_with_request(
            self.kernel.as_ref(),
            background::BackgroundExecutionRequest {
                agent_id: agent_id.to_string(),
                background_task_id: background_task_id.map(ToOwned::to_owned),
                input: None,
                memory_config: memory_config.clone(),
                steer_rx,
                emitter,
                state: Some(state),
            },
        )
        .await
    }
}

#[async_trait]
impl AgentOrchestrator for AgentOrchestratorImpl {
    async fn run(&self, plan: ExecutionPlan) -> std::result::Result<ExecutionOutcome, ToolError> {
        plan.validate()?;
        match plan.mode.clone().expect("validated mode") {
            restflow_traits::ExecutionMode::Interactive => {
                interactive::run_plan(self.kernel.as_ref(), plan).await
            }
            restflow_traits::ExecutionMode::Background => {
                background::run_plan(self.kernel.as_ref(), plan).await
            }
            restflow_traits::ExecutionMode::Subagent => {
                subagent::run_plan(self.kernel.as_ref(), plan).await
            }
        }
    }
}

#[derive(Clone)]
pub struct OrchestratingAgentExecutor {
    orchestrator: Arc<AgentOrchestratorImpl>,
}

impl OrchestratingAgentExecutor {
    pub fn new(orchestrator: Arc<AgentOrchestratorImpl>) -> Self {
        Self { orchestrator }
    }

    pub fn from_runtime_executor(executor: AgentRuntimeExecutor) -> Self {
        Self::new(Arc::new(AgentOrchestratorImpl::from_runtime_executor(
            executor,
        )))
    }

    pub fn orchestrator(&self) -> Arc<AgentOrchestratorImpl> {
        self.orchestrator.clone()
    }
}

#[async_trait]
impl AgentExecutor for OrchestratingAgentExecutor {
    async fn execute(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<ExecutionResult> {
        self.orchestrator
            .run_background_execution(
                agent_id,
                background_task_id,
                input,
                memory_config,
                steer_rx,
                None,
            )
            .await
    }

    async fn execute_with_emitter(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        self.orchestrator
            .run_background_execution(
                agent_id,
                background_task_id,
                input,
                memory_config,
                steer_rx,
                emitter,
            )
            .await
    }

    async fn execute_from_state(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        self.orchestrator
            .run_background_execution_from_state(
                agent_id,
                background_task_id,
                state,
                memory_config,
                steer_rx,
                emitter,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use anyhow::Result;
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use crate::models::{ChatSession, MemoryConfig, SteerMessage};
    use crate::runtime::background_agent::{
        ExecutionResult, SessionExecutionResult, SessionInputMode,
    };
    use crate::runtime::orchestrator::kernel::ExecutionBackend;
    use restflow_ai::AgentState;
    use restflow_ai::agent::StreamEmitter;
    use restflow_ai::llm::Message;
    use restflow_traits::{ExecutionMode, ExecutionPlan, InlineSubagentConfig};

    use super::*;

    #[derive(Default)]
    struct MockBackend {
        session: Mutex<Option<ChatSession>>,
        last_background: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ExecutionBackend for MockBackend {
        fn load_chat_session(&self, _session_id: &str) -> Result<ChatSession> {
            self.session
                .lock()
                .expect("session lock")
                .clone()
                .ok_or_else(|| anyhow::anyhow!("missing session"))
        }

        async fn execute_interactive_session_turn(
            &self,
            session: &mut ChatSession,
            _user_input: &str,
            _max_history: usize,
            _input_mode: SessionInputMode,
            _emitter: Option<Box<dyn StreamEmitter>>,
            _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        ) -> Result<SessionExecutionResult> {
            session.agent_id = "fallback-agent".to_string();
            Ok(SessionExecutionResult {
                output: "interactive-output".to_string(),
                iterations: 3,
                active_model: "gpt-5.3-codex".to_string(),
            })
        }

        async fn execute_background(
            &self,
            agent_id: &str,
            background_task_id: Option<&str>,
            _input: Option<&str>,
            _memory_config: &MemoryConfig,
            _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
            _emitter: Option<Box<dyn StreamEmitter>>,
        ) -> Result<ExecutionResult> {
            self.last_background
                .lock()
                .expect("background lock")
                .push(format!(
                    "{}:{}",
                    agent_id,
                    background_task_id.unwrap_or_default()
                ));
            Ok(ExecutionResult::success(
                "background-output".to_string(),
                vec![Message::assistant("done".to_string())],
            ))
        }

        async fn execute_background_from_state(
            &self,
            agent_id: &str,
            background_task_id: Option<&str>,
            _state: AgentState,
            _memory_config: &MemoryConfig,
            _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
            _emitter: Option<Box<dyn StreamEmitter>>,
        ) -> Result<ExecutionResult> {
            self.last_background
                .lock()
                .expect("background lock")
                .push(format!(
                    "resume:{}:{}",
                    agent_id,
                    background_task_id.unwrap_or_default()
                ));
            Ok(ExecutionResult::success(
                "resumed-output".to_string(),
                vec![Message::assistant("resumed".to_string())],
            ))
        }

        async fn execute_subagent_plan(&self, _plan: ExecutionPlan) -> Result<ExecutionOutcome> {
            Ok(ExecutionOutcome {
                success: true,
                text: Some("subagent-output".to_string()),
                ..ExecutionOutcome::default()
            })
        }
    }

    #[tokio::test]
    async fn run_interactive_session_turn_updates_session_and_result() {
        let backend = Arc::new(MockBackend::default());
        let mut session = ChatSession::new("agent-a".to_string(), "gpt-5".to_string());
        backend
            .session
            .lock()
            .expect("session lock")
            .replace(session.clone());
        let orchestrator = AgentOrchestratorImpl::new(backend);

        let result = orchestrator
            .run_interactive_session_turn(
                &mut session,
                "hello",
                20,
                SessionInputMode::EphemeralInput,
                None,
                None,
            )
            .await
            .expect("interactive run should succeed");

        assert_eq!(session.agent_id, "fallback-agent");
        assert_eq!(result.execution.output, "interactive-output");
        assert_eq!(result.outcome.iterations, Some(3));
        assert_eq!(result.outcome.model.as_deref(), Some("gpt-5.3-codex"));
    }

    #[tokio::test]
    async fn run_background_executor_delegates_through_orchestrator() {
        let backend = Arc::new(MockBackend::default());
        let executor =
            OrchestratingAgentExecutor::new(Arc::new(AgentOrchestratorImpl::new(backend.clone())));

        let result = executor
            .execute(
                "agent-a",
                Some("task-1"),
                Some("run"),
                &MemoryConfig::default(),
                None,
            )
            .await
            .expect("background execution should succeed");

        assert!(result.success);
        assert_eq!(result.output, "background-output");
        assert_eq!(
            backend
                .last_background
                .lock()
                .expect("background lock")
                .as_slice(),
            ["agent-a:task-1"]
        );
    }

    #[tokio::test]
    async fn run_plan_dispatches_interactive_mode() {
        let backend = Arc::new(MockBackend::default());
        let session = ChatSession::new("agent-a".to_string(), "gpt-5".to_string());
        let session_id = session.id.clone();
        backend
            .session
            .lock()
            .expect("session lock")
            .replace(session);
        let orchestrator = AgentOrchestratorImpl::new(backend);

        let outcome = orchestrator
            .run(ExecutionPlan {
                mode: Some(ExecutionMode::Interactive),
                agent_id: Some("agent-a".to_string()),
                chat_session_id: Some(session_id),
                input: Some("hello".to_string()),
                ..ExecutionPlan::default()
            })
            .await
            .expect("interactive plan should succeed");

        assert!(outcome.success);
        assert_eq!(outcome.text.as_deref(), Some("interactive-output"));
    }

    #[tokio::test]
    async fn run_plan_dispatches_subagent_mode() {
        let orchestrator = AgentOrchestratorImpl::new(Arc::new(MockBackend::default()));

        let outcome = orchestrator
            .run(ExecutionPlan {
                mode: Some(ExecutionMode::Subagent),
                input: Some("task".to_string()),
                inline_subagent: Some(InlineSubagentConfig::default()),
                ..ExecutionPlan::default()
            })
            .await
            .expect("subagent mode should delegate");

        assert_eq!(outcome.text.as_deref(), Some("subagent-output"));
    }
}
