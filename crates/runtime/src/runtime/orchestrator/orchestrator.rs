use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::runtime::orchestrator::kernel::{ExecutionBackend, ExecutionKernel};
use crate::runtime::orchestrator::modes::{interactive, subagent};
use crate::runtime::session_runner::{
    AgentRuntimeExecutor, SessionInputMode, SessionTurnRuntimeOptions,
};
use ::agent::StreamDisplayMode;
use ::agent::agent::{NullEmitter, StreamEmitter};
use types::{AgentOrchestrator, ExecutionOutcome, ExecutionPlan, ToolError};
use types::{ChatSession, SteerMessage};

#[derive(Debug)]
pub struct TracedInteractiveExecutionResult {
    pub turn_id: String,
    pub duration_ms: u64,
    pub execution: crate::runtime::session_runner::SessionExecutionResult,
}

pub struct InteractiveSessionRequest<'a> {
    pub session: &'a mut ChatSession,
    pub user_input: &'a str,
    pub max_history: usize,
    pub input_mode: SessionInputMode,
    pub run_id: String,
    pub timeout_secs: Option<u64>,
    pub emitter: Option<Box<dyn StreamEmitter>>,
    pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    pub stream_display_mode: StreamDisplayMode,
    pub workspace_root: Option<PathBuf>,
}

#[derive(Debug)]
pub enum InteractiveExecutionError {
    Timeout { timeout_secs: u64 },
    Execution(anyhow::Error),
}

impl std::fmt::Display for InteractiveExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout { timeout_secs } => {
                write!(f, "execution timed out after {} seconds", timeout_secs)
            }
            Self::Execution(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for InteractiveExecutionError {}

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

    async fn run_interactive_session_turn_with_options(
        &self,
        session: &mut ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
        emitter: Option<Box<dyn StreamEmitter>>,
        options: SessionTurnRuntimeOptions,
    ) -> Result<interactive::InteractiveExecutionResult> {
        interactive::run_with_session_options(
            self.kernel.as_ref(),
            session,
            user_input,
            max_history,
            input_mode,
            emitter,
            options,
        )
        .await
    }

    pub async fn run_traced_interactive_session_turn(
        &self,
        request: InteractiveSessionRequest<'_>,
    ) -> std::result::Result<TracedInteractiveExecutionResult, InteractiveExecutionError> {
        let InteractiveSessionRequest {
            session,
            user_input,
            max_history,
            input_mode,
            run_id,
            timeout_secs,
            emitter,
            steer_rx,
            stream_display_mode,
            workspace_root,
        } = request;
        self.kernel
            .backend()
            .prepare_interactive_session(session)
            .map_err(InteractiveExecutionError::Execution)?;

        let inner_emitter = emitter.unwrap_or_else(|| Box::new(NullEmitter));
        let traced_emitter: Box<dyn StreamEmitter> = inner_emitter;

        let started_at = Instant::now();
        let execution_result = if let Some(timeout_secs) = timeout_secs {
            match tokio::time::timeout(
                tokio::time::Duration::from_secs(timeout_secs),
                self.run_interactive_session_turn_with_options(
                    session,
                    user_input,
                    max_history,
                    input_mode,
                    Some(traced_emitter),
                    SessionTurnRuntimeOptions {
                        steer_rx,
                        stream_display_mode,
                        workspace_root,
                    },
                ),
            )
            .await
            {
                Ok(result) => result.map_err(InteractiveExecutionError::Execution),
                Err(_) => {
                    let duration_ms = started_at.elapsed().as_millis() as u64;
                    let error = InteractiveExecutionError::Timeout { timeout_secs };
                    let _ = duration_ms;
                    return Err(error);
                }
            }
        } else {
            self.run_interactive_session_turn_with_options(
                session,
                user_input,
                max_history,
                input_mode,
                Some(traced_emitter),
                SessionTurnRuntimeOptions {
                    steer_rx,
                    stream_display_mode,
                    workspace_root,
                },
            )
            .await
            .map_err(InteractiveExecutionError::Execution)
        };

        let execution = match execution_result {
            Ok(result) => result.execution,
            Err(error) => {
                return Err(error);
            }
        };

        let duration_ms = started_at.elapsed().as_millis() as u64;

        Ok(TracedInteractiveExecutionResult {
            turn_id: run_id,
            duration_ms,
            execution,
        })
    }
}

#[async_trait]
impl AgentOrchestrator for AgentOrchestratorImpl {
    async fn run(&self, plan: ExecutionPlan) -> std::result::Result<ExecutionOutcome, ToolError> {
        plan.validate()?;
        match plan.mode.clone().expect("validated mode") {
            types::ExecutionMode::Interactive => {
                interactive::run_plan(self.kernel.as_ref(), plan).await
            }
            types::ExecutionMode::Background => Err(ToolError::InvalidInput(
                "background task execution has been removed; use an interactive session or subagent"
                    .to_string(),
            )),
            types::ExecutionMode::Subagent => subagent::run_plan(self.kernel.as_ref(), plan).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use anyhow::Result;
    use async_trait::async_trait;

    use crate::runtime::orchestrator::kernel::ExecutionBackend;
    use crate::runtime::session_runner::{SessionExecutionResult, SessionInputMode};
    use ::agent::agent::StreamEmitter;
    use types::{ChatSession, ModelId};
    use types::{ExecutionMode, ExecutionPlan, InlineSubagentConfig};

    use super::*;

    #[derive(Default)]
    struct MockBackend {
        session: Mutex<Option<ChatSession>>,
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
            _options: SessionTurnRuntimeOptions,
        ) -> Result<SessionExecutionResult> {
            session.agent_id = "fallback-agent".to_string();
            let result = SessionExecutionResult::new(
                "interactive-output".to_string(),
                3,
                "gpt-5.3-codex".to_string(),
                ModelId::CodexCli,
            );
            Ok(result)
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
    async fn run_traced_interactive_session_turn_returns_timeout_error() {
        #[derive(Default)]
        struct SlowBackend;

        #[async_trait]
        impl ExecutionBackend for SlowBackend {
            fn load_chat_session(&self, _session_id: &str) -> Result<ChatSession> {
                Ok(ChatSession::new("agent-a".to_string(), "gpt-5".to_string()))
            }

            async fn execute_interactive_session_turn(
                &self,
                _session: &mut ChatSession,
                _user_input: &str,
                _max_history: usize,
                _input_mode: SessionInputMode,
                _emitter: Option<Box<dyn StreamEmitter>>,
                _options: SessionTurnRuntimeOptions,
            ) -> Result<SessionExecutionResult> {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                Ok(SessionExecutionResult::new(
                    "too-late".to_string(),
                    1,
                    "gpt-5".to_string(),
                    ModelId::Gpt5,
                ))
            }

            async fn execute_subagent_plan(
                &self,
                _plan: ExecutionPlan,
            ) -> Result<ExecutionOutcome> {
                unreachable!("subagent path not used")
            }
        }

        let orchestrator = AgentOrchestratorImpl::new(Arc::new(SlowBackend));
        let mut session = ChatSession::new("agent-a".to_string(), "gpt-5".to_string());

        let error = orchestrator
            .run_traced_interactive_session_turn(InteractiveSessionRequest {
                session: &mut session,
                user_input: "hello",
                max_history: 20,
                input_mode: SessionInputMode::EphemeralInput,
                run_id: "run-timeout".to_string(),
                timeout_secs: Some(0),
                emitter: None,
                steer_rx: None,
                stream_display_mode: StreamDisplayMode::Buffered,
                workspace_root: None,
            })
            .await
            .expect_err("interactive run should time out");

        assert!(matches!(
            error,
            InteractiveExecutionError::Timeout { timeout_secs: 0 }
        ));
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
