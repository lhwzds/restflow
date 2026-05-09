//! Real agent executor implementation for the task runner.
//!
//! This module provides `AgentRuntimeExecutor`, which implements the
//! `AgentExecutor` trait by running the shared agent execution engine.
//! It loads agent configuration from storage, builds the appropriate LLM
//! client, and executes the agent with the configured tools.

use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::runtime::{AgentOrchestratorImpl, ExecutionContext};
use crate::{
    ModelId, Provider,
    auth::{AuthProfileManager, resolve_model_from_credentials, secret_exists},
    models::{
        AgentNode, ApiKeyConfig, ChatMessage, ChatRole, ChatSession, ChatTurnEventKind,
        ChatTurnStatus, Skill, SteerMessage, TaskStatus,
    },
    process::ProcessRegistry,
    services::session::SessionService,
    services::skill_triggers::match_triggers,
    storage::Storage,
};
use ai::agent::{LlmToolCallReviewer, SharedStreamEmitter, StreamEmitter};
use ai::llm::Message;
use ai::{
    AgentConfig as ReActAgentConfig, AgentExecutor as ReActAgentExecutor, CodexClient,
    DefaultLlmClientFactory, LlmClient, LlmClientFactory, ResourceLimits as AgentResourceLimits,
    SwappableLlm,
};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tools::{ProcessTool, ReplyTool, SwitchModelTool};
use tracing::{debug, info, warn};
use types::llm::LlmProvider;
use types::{ExecutionOutcome, ExecutionPlan, ReplySender};

use super::error_classification::{classify_execution_error, is_authentication_classification};
use super::failover::{FailoverConfig, FailoverManager, execute_with_failover};
use super::model_catalog::ModelCatalog;
use super::outcome::SessionExecutionResult;
use super::preflight::{PreflightCategory, PreflightIssue, run_preflight};
use super::retry::{RetryConfig, RetryState};
use super::runner::{AgentExecutor, ExecutionResult};
use super::skill_snapshot::{
    SkillSnapshotCache, SkillSnapshotKey, SkillSnapshotPayload, build_skill_filter_signature,
    build_skill_version_hash, build_trigger_context_signature,
};
use crate::runtime::agent::{
    BashConfig, SkillActivationPolicy, ToolRegistry, build_agent_system_prompt,
    effective_tool_allowlist_for_turn, main_agent_default_tool_names, registry_from_allowlist,
    secret_resolver_from_storage,
};
use ai::agent::SubagentDefLookup;
use ai::agent::{SubagentConfig, SubagentExecutionBridge, SubagentTracker, execute_subagent_plan};
use ai::llm::LlmSwitcherImpl;
#[cfg(any(test, feature = "test-utils"))]
use std::sync::{Mutex, OnceLock};

fn share_stream_emitter(emitter: Option<Box<dyn StreamEmitter>>) -> Option<SharedStreamEmitter> {
    emitter.map(SharedStreamEmitter::new)
}

fn clone_shared_emitter(emitter: &Option<SharedStreamEmitter>) -> Option<Box<dyn StreamEmitter>> {
    emitter
        .as_ref()
        .map(|shared| Box::new(shared.clone()) as Box<dyn StreamEmitter>)
}

#[cfg(any(test, feature = "test-utils"))]
type TestLlmFactorySlot = Mutex<Option<Arc<dyn LlmClientFactory>>>;

#[cfg(any(test, feature = "test-utils"))]
fn test_llm_factory_slot() -> &'static TestLlmFactorySlot {
    static SLOT: OnceLock<TestLlmFactorySlot> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(any(test, feature = "test-utils"))]
pub struct TestLlmFactoryGuard {
    previous: Option<Arc<dyn LlmClientFactory>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl Drop for TestLlmFactoryGuard {
    fn drop(&mut self) {
        *test_llm_factory_slot()
            .lock()
            .expect("test llm factory slot lock") = self.previous.take();
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn install_test_llm_factory(factory: Arc<dyn LlmClientFactory>) -> TestLlmFactoryGuard {
    let mut guard = test_llm_factory_slot()
        .lock()
        .expect("test llm factory slot lock");
    let previous = guard.replace(factory);
    TestLlmFactoryGuard { previous }
}

#[cfg(any(test, feature = "test-utils"))]
fn current_test_llm_factory() -> Option<Arc<dyn LlmClientFactory>> {
    test_llm_factory_slot()
        .lock()
        .expect("test llm factory slot lock")
        .clone()
}

/// Real agent executor that bridges to ai::AgentExecutor.
///
/// This executor:
/// - Loads agent configuration from storage
/// - Resolves API keys (direct or from secrets)
/// - Creates the appropriate LLM client for the model
/// - Builds the system prompt from the agent's skill
/// - Executes the agent via the ReAct loop
#[derive(Clone)]
pub struct AgentRuntimeExecutor {
    storage: Arc<Storage>,
    process_registry: Arc<ProcessRegistry>,
    auth_manager: Arc<AuthProfileManager>,
    subagent_tracker: Arc<SubagentTracker>,
    subagent_definitions: Arc<dyn SubagentDefLookup>,
    subagent_config: SubagentConfig,
    session_service: SessionService,
    skill_snapshot_cache: Arc<SkillSnapshotCache>,
    reply_sender: Option<Arc<dyn ReplySender>>,
    reply_sender_factory: Option<Arc<dyn ReplySenderFactory>>,
}

/// Factory for constructing execution-scoped reply senders.
///
/// Background-agent execution needs a sender bound to the current task ID,
/// while interactive chat execution usually uses a static sender per session.
pub trait ReplySenderFactory: Send + Sync {
    fn for_task(&self, task_id: &str, agent_id: &str) -> Option<Arc<dyn ReplySender>>;
}

const TOOL_RESULT_CONTEXT_RATIO: f64 = 0.08;
const TOOL_RESULT_MIN_CHARS: usize = 512;
const TOOL_RESULT_MAX_CHARS: usize = 24_000;
const TOOL_RESULT_CHARS_PER_TOKEN_ESTIMATE: usize = 4;
/// Controls whether the latest user input has already been persisted
/// to the chat session before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionInputMode {
    /// Latest user input is already stored as the newest session message.
    PersistedInSession,
    /// Latest user input is provided only as runtime input for this turn.
    EphemeralInput,
}

#[derive(Debug, Clone)]
struct ResolvedSkillSnapshot {
    resolved_skills: Vec<Skill>,
}

impl AgentRuntimeExecutor {
    pub(crate) fn load_chat_session(&self, session_id: &str) -> Result<ChatSession> {
        self.session_service
            .get_session_view(session_id)?
            .ok_or_else(|| anyhow!("Session not found: {}", session_id))
    }

    pub(crate) async fn execute_subagent_plan(
        &self,
        plan: ExecutionPlan,
    ) -> Result<ExecutionOutcome> {
        let llm_client: Arc<dyn LlmClient> = Arc::new(CodexClient::new());
        let factory: Arc<dyn LlmClientFactory> = Arc::new(DefaultLlmClientFactory::new(
            self.build_api_keys(None, Provider::OpenAI).await,
            ModelId::build_model_specs(),
        ));
        let swappable = Arc::new(SwappableLlm::new(llm_client.clone()));
        let agent_defaults = self
            .storage
            .config
            .get_effective_config()
            .ok()
            .map(|config| config.agent)
            .unwrap_or_default();
        let bash_config = BashConfig {
            timeout_secs: agent_defaults.bash_timeout_secs,
            ..BashConfig::default()
        };
        let default_tools = main_agent_default_tool_names();
        let tool_registry = self.build_tool_registry(
            Some(&default_tools),
            llm_client.clone(),
            swappable,
            factory.clone(),
            None,
            Some(bash_config),
            None,
            None,
        )?;
        execute_subagent_plan(
            self.subagent_definitions.clone(),
            llm_client,
            tool_registry,
            self.subagent_config.clone(),
            plan,
            SubagentExecutionBridge {
                llm_client_factory: Some(factory),
                orchestrator: None,
            },
        )
        .await
        .map_err(|error| anyhow!(error.to_string()))
    }

    fn validate_prerequisites(&self, prerequisites: &[String]) -> Result<()> {
        if prerequisites.is_empty() {
            return Ok(());
        }

        let mut failed = Vec::new();
        for task_id in prerequisites {
            match self.storage.tasks.get_task(task_id)? {
                Some(task) if task.status == TaskStatus::Completed => {}
                Some(task) => failed.push(format!("{} ({})", task.id, task.status.as_str())),
                None => failed.push(format!("{task_id} (not found)")),
            }
        }

        if failed.is_empty() {
            Ok(())
        } else {
            Err(anyhow!("Prerequisites not met: {}", failed.join(", ")))
        }
    }

    /// Create a new AgentRuntimeExecutor with access to storage.
    pub fn new(
        storage: Arc<Storage>,
        process_registry: Arc<ProcessRegistry>,
        auth_manager: Arc<AuthProfileManager>,
        subagent_tracker: Arc<SubagentTracker>,
        subagent_definitions: Arc<dyn SubagentDefLookup>,
        subagent_config: SubagentConfig,
    ) -> Self {
        let session_service = SessionService::from_storage(storage.as_ref());
        Self {
            storage,
            process_registry,
            auth_manager,
            subagent_tracker,
            subagent_definitions,
            subagent_config,
            session_service,
            skill_snapshot_cache: Arc::new(SkillSnapshotCache::default()),
            reply_sender: None,
            reply_sender_factory: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_session_service(mut self, session_service: SessionService) -> Self {
        self.session_service = session_service;
        self
    }

    /// Set a reply sender so the agent can send intermediate messages.
    pub fn with_reply_sender(mut self, sender: Arc<dyn ReplySender>) -> Self {
        self.reply_sender = Some(sender);
        self
    }

    /// Set a reply sender factory for execution-scoped contexts (for example
    /// task executions where each task has distinct routing semantics).
    pub fn with_reply_sender_factory(mut self, factory: Arc<dyn ReplySenderFactory>) -> Self {
        self.reply_sender_factory = Some(factory);
        self
    }
}

fn is_credential_error(error: &anyhow::Error) -> bool {
    is_authentication_classification(classify_execution_error(error))
}

mod model_resolution;
mod preflight;
mod session_execution;
mod task_execution;
mod tooling;

pub use session_execution::SessionTurnRuntimeOptions;

#[cfg(test)]
mod tests;
