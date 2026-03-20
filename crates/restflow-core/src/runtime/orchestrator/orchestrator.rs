use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::models::{ChatSession, MemoryConfig, SteerMessage};
use crate::runtime::background_agent::{
    AgentExecutor, AgentRuntimeExecutor, ExecutionResult, SessionInputMode,
};
use crate::runtime::orchestrator::kernel::{ExecutionBackend, ExecutionKernel};
use crate::runtime::orchestrator::modes::{background, interactive, subagent};
use crate::runtime::trace::{
    RestflowTrace, TraceEvent, append_trace_event, build_restflow_trace_emitter,
};
use crate::storage::{ExecutionTraceStorage, ToolTraceStorage};
use restflow_ai::AgentState;
use restflow_ai::agent::{NullEmitter, StreamEmitter};
use restflow_traits::{AgentOrchestrator, ExecutionOutcome, ExecutionPlan, ToolError};

#[derive(Debug)]
pub struct TracedInteractiveExecutionResult {
    pub trace: RestflowTrace,
    pub duration_ms: u64,
    pub execution: crate::runtime::background_agent::SessionExecutionResult,
}

pub struct InteractiveSessionRequest<'a> {
    pub session: &'a mut ChatSession,
    pub user_input: &'a str,
    pub max_history: usize,
    pub input_mode: SessionInputMode,
    pub run_id: String,
    pub tool_trace_storage: ToolTraceStorage,
    pub execution_trace_storage: ExecutionTraceStorage,
    pub timeout_secs: Option<u64>,
    pub emitter: Option<Box<dyn StreamEmitter>>,
    pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
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
            tool_trace_storage,
            execution_trace_storage,
            timeout_secs,
            emitter,
            steer_rx,
        } = request;
        let trace = RestflowTrace::new(
            run_id,
            session.id.clone(),
            session.id.clone(),
            session.agent_id.clone(),
        );
        append_trace_event(
            &tool_trace_storage,
            Some(&execution_trace_storage),
            &TraceEvent::run_started(trace.clone()),
        );

        let inner_emitter = emitter.unwrap_or_else(|| Box::new(NullEmitter));
        let traced_emitter: Box<dyn StreamEmitter> = build_restflow_trace_emitter(
            inner_emitter,
            tool_trace_storage.clone(),
            Some(execution_trace_storage.clone()),
            &trace,
        );

        let started_at = Instant::now();
        let execution_result = if let Some(timeout_secs) = timeout_secs {
            match tokio::time::timeout(
                tokio::time::Duration::from_secs(timeout_secs),
                self.run_interactive_session_turn(
                    session,
                    user_input,
                    max_history,
                    input_mode,
                    Some(traced_emitter),
                    steer_rx,
                ),
            )
            .await
            {
                Ok(result) => result.map_err(InteractiveExecutionError::Execution),
                Err(_) => {
                    let duration_ms = started_at.elapsed().as_millis() as u64;
                    let error = InteractiveExecutionError::Timeout { timeout_secs };
                    append_trace_event(
                        &tool_trace_storage,
                        Some(&execution_trace_storage),
                        &TraceEvent::run_failed(
                            trace.clone(),
                            error.to_string(),
                            Some(duration_ms),
                        ),
                    );
                    return Err(error);
                }
            }
        } else {
            self.run_interactive_session_turn(
                session,
                user_input,
                max_history,
                input_mode,
                Some(traced_emitter),
                steer_rx,
            )
            .await
            .map_err(InteractiveExecutionError::Execution)
        };

        let execution = match execution_result {
            Ok(result) => result.execution,
            Err(error) => {
                append_trace_event(
                    &tool_trace_storage,
                    Some(&execution_trace_storage),
                    &TraceEvent::run_failed(
                        trace.clone(),
                        error.to_string(),
                        Some(started_at.elapsed().as_millis() as u64),
                    ),
                );
                return Err(error);
            }
        };

        let duration_ms = started_at.elapsed().as_millis() as u64;
        append_trace_event(
            &tool_trace_storage,
            Some(&execution_trace_storage),
            &TraceEvent::run_completed(trace.clone(), Some(duration_ms)),
        );

        Ok(TracedInteractiveExecutionResult {
            trace,
            duration_ms,
            execution,
        })
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
    use redb::Database;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    use crate::models::{
        ChatSession, ExecutionTraceCategory, ExecutionTraceQuery, MemoryConfig, ModelId,
        SteerMessage,
    };
    use crate::runtime::background_agent::{
        ExecutionResult, SessionExecutionResult, SessionInputMode,
    };
    use crate::runtime::orchestrator::kernel::ExecutionBackend;
    use crate::storage::{ExecutionTraceStorage, ToolTraceStorage};
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
            Ok(SessionExecutionResult::new(
                "interactive-output".to_string(),
                3,
                "gpt-5.3-codex".to_string(),
                ModelId::CodexCli,
            ))
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

    fn setup_trace_storages() -> (TempDir, ToolTraceStorage, ExecutionTraceStorage) {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("orchestrator-trace.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        (
            temp_dir,
            ToolTraceStorage::new(db.clone()).expect("tool trace storage"),
            ExecutionTraceStorage::new(db).expect("execution trace storage"),
        )
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
    async fn run_traced_interactive_session_turn_records_trace_events() {
        let backend = Arc::new(MockBackend::default());
        let mut session = ChatSession::new("agent-a".to_string(), "gpt-5".to_string());
        let session_id = session.id.clone();
        backend
            .session
            .lock()
            .expect("session lock")
            .replace(session.clone());
        let orchestrator = AgentOrchestratorImpl::new(backend);
        let (_temp_dir, tool_trace_storage, execution_trace_storage) = setup_trace_storages();

        let result = orchestrator
            .run_traced_interactive_session_turn(InteractiveSessionRequest {
                session: &mut session,
                user_input: "hello",
                max_history: 20,
                input_mode: SessionInputMode::EphemeralInput,
                run_id: "run-traced".to_string(),
                tool_trace_storage: tool_trace_storage.clone(),
                execution_trace_storage,
                timeout_secs: None,
                emitter: None,
                steer_rx: None,
            })
            .await
            .expect("traced interactive run should succeed");

        let events = tool_trace_storage
            .list_by_session_turn(&session_id, "run-run-traced", None)
            .expect("trace list");
        assert_eq!(events.len(), 2);
        assert_eq!(result.execution.output, "interactive-output");
        assert!(result.duration_ms <= 1_000);
    }

    #[tokio::test]
    async fn run_traced_interactive_session_turn_persists_llm_and_model_switch_events() {
        #[derive(Default)]
        struct TraceBackend;

        #[async_trait]
        impl ExecutionBackend for TraceBackend {
            fn load_chat_session(&self, _session_id: &str) -> Result<ChatSession> {
                unreachable!("not used")
            }

            async fn execute_interactive_session_turn(
                &self,
                _session: &mut ChatSession,
                _user_input: &str,
                _max_history: usize,
                _input_mode: SessionInputMode,
                mut emitter: Option<Box<dyn StreamEmitter>>,
                _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
            ) -> Result<SessionExecutionResult> {
                if let Some(emitter) = emitter.as_mut() {
                    emitter
                        .emit_model_switch(
                            "minimax-coding-plan-m2-5-highspeed",
                            "minimax-coding-plan-m2-5",
                            Some("failover"),
                        )
                        .await;
                    emitter
                        .emit_llm_call(
                            "minimax-coding-plan-m2-5",
                            Some(10),
                            Some(5),
                            Some(15),
                            Some(0.01),
                            Some(120),
                            None,
                            Some(2),
                        )
                        .await;
                }
                Ok(SessionExecutionResult::new(
                    "done".to_string(),
                    1,
                    "minimax-coding-plan-m2-5".to_string(),
                    ModelId::MiniMaxM25CodingPlan,
                ))
            }

            async fn execute_background(
                &self,
                _agent_id: &str,
                _background_task_id: Option<&str>,
                _input: Option<&str>,
                _memory_config: &MemoryConfig,
                _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                _emitter: Option<Box<dyn StreamEmitter>>,
            ) -> Result<ExecutionResult> {
                unreachable!("background path not used")
            }

            async fn execute_background_from_state(
                &self,
                _agent_id: &str,
                _background_task_id: Option<&str>,
                _state: AgentState,
                _memory_config: &MemoryConfig,
                _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                _emitter: Option<Box<dyn StreamEmitter>>,
            ) -> Result<ExecutionResult> {
                unreachable!("background resume path not used")
            }

            async fn execute_subagent_plan(
                &self,
                _plan: ExecutionPlan,
            ) -> Result<ExecutionOutcome> {
                unreachable!("subagent path not used")
            }
        }

        let orchestrator = AgentOrchestratorImpl::new(Arc::new(TraceBackend));
        let (_temp_dir, tool_trace_storage, execution_trace_storage) = setup_trace_storages();
        let mut session = ChatSession::new(
            "agent-a".to_string(),
            "minimax-coding-plan-m2-5-highspeed".to_string(),
        );
        let session_id = session.id.clone();
        let started_at = chrono::Utc::now().timestamp_millis();

        let result = orchestrator
            .run_traced_interactive_session_turn(InteractiveSessionRequest {
                session: &mut session,
                user_input: "hello",
                max_history: 20,
                input_mode: SessionInputMode::EphemeralInput,
                run_id: "run-traced-llm".to_string(),
                tool_trace_storage,
                execution_trace_storage: execution_trace_storage.clone(),
                timeout_secs: None,
                emitter: None,
                steer_rx: None,
            })
            .await
            .expect("traced interactive run should succeed");

        assert_eq!(result.execution.final_model, ModelId::MiniMaxM25CodingPlan);

        let by_task = execution_trace_storage
            .query(&ExecutionTraceQuery {
                task_id: Some(session_id.clone()),
                ..Default::default()
            })
            .expect("query by task");
        assert!(
            by_task
                .iter()
                .any(|event| event.category == ExecutionTraceCategory::LlmCall)
        );
        assert!(
            by_task
                .iter()
                .any(|event| event.category == ExecutionTraceCategory::ModelSwitch)
        );

        let by_agent = execution_trace_storage
            .query(&ExecutionTraceQuery {
                agent_id: Some("agent-a".to_string()),
                from_timestamp: Some(started_at - 1_000),
                to_timestamp: Some(chrono::Utc::now().timestamp_millis() + 1_000),
                ..Default::default()
            })
            .expect("query by agent");
        assert!(
            by_agent
                .iter()
                .any(|event| event.category == ExecutionTraceCategory::LlmCall)
        );
        assert!(
            by_agent
                .iter()
                .any(|event| event.category == ExecutionTraceCategory::ModelSwitch)
        );
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
                _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
            ) -> Result<SessionExecutionResult> {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                Ok(SessionExecutionResult::new(
                    "too-late".to_string(),
                    1,
                    "gpt-5".to_string(),
                    ModelId::Gpt5,
                ))
            }

            async fn execute_background(
                &self,
                _agent_id: &str,
                _background_task_id: Option<&str>,
                _input: Option<&str>,
                _memory_config: &MemoryConfig,
                _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                _emitter: Option<Box<dyn StreamEmitter>>,
            ) -> Result<ExecutionResult> {
                unreachable!("background path not used")
            }

            async fn execute_background_from_state(
                &self,
                _agent_id: &str,
                _background_task_id: Option<&str>,
                _state: AgentState,
                _memory_config: &MemoryConfig,
                _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                _emitter: Option<Box<dyn StreamEmitter>>,
            ) -> Result<ExecutionResult> {
                unreachable!("background resume path not used")
            }

            async fn execute_subagent_plan(
                &self,
                _plan: ExecutionPlan,
            ) -> Result<ExecutionOutcome> {
                unreachable!("subagent path not used")
            }
        }

        let orchestrator = AgentOrchestratorImpl::new(Arc::new(SlowBackend));
        let (_temp_dir, tool_trace_storage, execution_trace_storage) = setup_trace_storages();
        let mut session = ChatSession::new("agent-a".to_string(), "gpt-5".to_string());

        let error = orchestrator
            .run_traced_interactive_session_turn(InteractiveSessionRequest {
                session: &mut session,
                user_input: "hello",
                max_history: 20,
                input_mode: SessionInputMode::EphemeralInput,
                run_id: "run-timeout".to_string(),
                tool_trace_storage,
                execution_trace_storage,
                timeout_secs: Some(0),
                emitter: None,
                steer_rx: None,
            })
            .await
            .expect_err("interactive run should time out");

        assert!(matches!(
            error,
            InteractiveExecutionError::Timeout { timeout_secs: 0 }
        ));
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
