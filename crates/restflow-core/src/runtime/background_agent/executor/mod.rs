//! Real agent executor implementation for the task runner.
//!
//! This module provides `AgentRuntimeExecutor`, which implements the
//! `AgentExecutor` trait by running the shared agent execution engine.
//! It loads agent configuration from storage, builds the appropriate LLM
//! client, and executes the agent with the configured tools.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::runtime::{AgentOrchestratorImpl, ExecutionContext};
use crate::{
    ModelId, Provider,
    auth::{AuthProfileManager, resolve_model_from_credentials, secret_exists},
    models::{
        AgentCheckpoint, AgentNode, ApiKeyConfig, ChatMessage, ChatRole, ChatSession,
        DurabilityMode, MemoryConfig, SharedEntry, Skill, SteerMessage, Visibility,
    },
    process::ProcessRegistry,
    prompt_files,
    services::skill_triggers::match_triggers,
    storage::Storage,
};
use restflow_ai::agent::{
    CheckpointDurability, ModelRoutingConfig as AiModelRoutingConfig,
    ModelSwitcher as AiModelSwitcher, PromptFlags, SharedStreamEmitter, StreamEmitter,
};
use restflow_ai::llm::{CompletionRequest, Message};
use restflow_ai::{
    AgentConfig as ReActAgentConfig, AgentExecutor as ReActAgentExecutor, AiError, CodexClient,
    DefaultLlmClientFactory, LlmClient, LlmClientFactory, ResourceLimits as AgentResourceLimits,
    SwappableLlm,
};
use restflow_models::LlmProvider;
use restflow_tools::{ProcessTool, ReplyTool, SwitchModelTool};
use restflow_traits::{ExecutionOutcome, ExecutionPlan, ReplySender};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, info, warn};

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
    BashConfig, SubagentDeps, SubagentManager, SubagentManagerImpl, ToolRegistry,
    build_agent_system_prompt, effective_main_agent_tool_names, main_agent_default_tool_names,
    registry_from_allowlist, secret_resolver_from_storage,
};
use restflow_ai::agent::SubagentDefLookup;
use restflow_ai::agent::{
    SubagentConfig, SubagentExecutionBridge, SubagentTracker, execute_subagent_once,
};
use restflow_ai::llm::LlmSwitcherImpl;

fn share_stream_emitter(emitter: Option<Box<dyn StreamEmitter>>) -> Option<SharedStreamEmitter> {
    emitter.map(SharedStreamEmitter::new)
}

fn clone_shared_emitter(emitter: &Option<SharedStreamEmitter>) -> Option<Box<dyn StreamEmitter>> {
    emitter
        .as_ref()
        .map(|shared| Box::new(shared.clone()) as Box<dyn StreamEmitter>)
}

/// Real agent executor that bridges to restflow_ai::AgentExecutor.
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
    skill_snapshot_cache: Arc<SkillSnapshotCache>,
    reply_sender: Option<Arc<dyn ReplySender>>,
    reply_sender_factory: Option<Arc<dyn ReplySenderFactory>>,
}

/// Factory for constructing execution-scoped reply senders.
///
/// Background-agent execution needs a sender bound to the current task ID,
/// while interactive chat execution usually uses a static sender per session.
pub trait ReplySenderFactory: Send + Sync {
    fn for_background_task(&self, task_id: &str, agent_id: &str) -> Option<Arc<dyn ReplySender>>;
}

const TOOL_RESULT_CONTEXT_RATIO: f64 = 0.08;
const TOOL_RESULT_MIN_CHARS: usize = 512;
const TOOL_RESULT_MAX_CHARS: usize = 24_000;
const TOOL_RESULT_CHARS_PER_TOKEN_ESTIMATE: usize = 4;
const ACK_PHASE_MAX_HISTORY: usize = 6;
const ACK_PHASE_MAX_TOKENS: u32 = 96;
const ACK_PHASE_TIMEOUT_SECS: u64 = 4;
const ACK_PHASE_MAX_CHARS: usize = 280;
const ACK_PHASE_SYSTEM_DIRECTIVE: &str = r#"You are in a temporary acknowledgement phase.

Reply with exactly one short assistant message that:
1. Confirms you received the user's latest message.
2. States you are starting the task.

Rules:
- Do not solve the task yet.
- Do not mention tools or internal reasoning.
- Keep it concise and concrete.
- Match the user's language.
- Output plain text only.
"#;

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
    triggered_skill_ids: Vec<String>,
    resolved_skills: Vec<Skill>,
}

struct RuntimeModelSwitcher {
    swappable: Arc<SwappableLlm>,
    factory: Arc<dyn LlmClientFactory>,
    agent_node: AgentNode,
}

#[async_trait]
impl AiModelSwitcher for RuntimeModelSwitcher {
    fn current_model(&self) -> String {
        self.swappable.current_model()
    }

    async fn switch_model(&self, target_model: &str) -> std::result::Result<(), AiError> {
        let model = ModelId::from_api_name(target_model)
            .ok_or_else(|| AiError::Agent(format!("Unsupported routed model: {}", target_model)))?;
        let client = AgentRuntimeExecutor::create_llm_client(
            self.factory.as_ref(),
            model,
            None,
            &self.agent_node,
        )
        .map_err(|error| AiError::Agent(error.to_string()))?;
        self.swappable.swap(client);
        Ok(())
    }
}

fn spawn_request_from_plan(plan: &ExecutionPlan) -> Result<restflow_traits::SpawnRequest> {
    Ok(restflow_traits::SpawnRequest {
        agent_id: plan.agent_id.clone(),
        inline: plan.inline_subagent.clone(),
        task: plan
            .input
            .clone()
            .ok_or_else(|| anyhow!("Subagent execution requires 'input'"))?,
        timeout_secs: plan.timeout_secs,
        max_iterations: plan.max_iterations,
        priority: None,
        model: plan.model.clone(),
        model_provider: plan.provider.clone(),
        parent_execution_id: plan.parent_execution_id.clone(),
        trace_session_id: plan.trace_session_id.clone(),
        trace_scope_id: plan.trace_scope_id.clone(),
    })
}

impl AgentRuntimeExecutor {
    pub(crate) fn load_chat_session(&self, session_id: &str) -> Result<ChatSession> {
        self.storage
            .chat_sessions
            .get(session_id)?
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
        execute_subagent_once(
            self.subagent_definitions.clone(),
            llm_client,
            tool_registry,
            self.subagent_config.clone(),
            spawn_request_from_plan(&plan)?,
            SubagentExecutionBridge {
                llm_client_factory: Some(factory),
                orchestrator: None,
                telemetry_sink: Some(crate::telemetry::build_core_telemetry_sink(
                    self.storage.as_ref(),
                )),
            },
        )
        .await
        .map_err(|error| anyhow!(error.to_string()))
    }

    fn save_task_deliverable(&self, task_id: &str, agent_id: &str, output: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let key = format!("deliverable:{task_id}");
        let payload = serde_json::json!({
            "agent_id": agent_id,
            "parts": [
                {
                    "type": "text",
                    "content": output,
                }
            ],
            "completed_at": Utc::now().to_rfc3339(),
        });
        let payload = serde_json::to_string(&payload)?;
        let created_at = self
            .storage
            .kv_store
            .get_unchecked(&key)?
            .map(|entry| entry.created_at)
            .unwrap_or(now);
        let entry = SharedEntry {
            key,
            value: payload,
            visibility: Visibility::Shared,
            owner: Some(agent_id.to_string()),
            content_type: Some("application/json".to_string()),
            type_hint: Some("deliverable".to_string()),
            tags: vec!["deliverable".to_string()],
            created_at,
            updated_at: now,
            last_modified_by: Some(agent_id.to_string()),
        };
        self.storage.kv_store.set(&entry)
    }

    fn validate_prerequisites(&self, prerequisites: &[String]) -> Result<()> {
        if prerequisites.is_empty() {
            return Ok(());
        }

        let mut failed = Vec::new();
        for task_id in prerequisites {
            let key = format!("deliverable:{task_id}");
            match self.storage.kv_store.quick_get(&key, None) {
                Ok(Some(raw)) => match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(value) => {
                        let parts = value.get("parts").and_then(|part| part.as_array());
                        if parts.is_none_or(|items| items.is_empty()) {
                            failed.push(format!("{task_id} (empty deliverable)"));
                        }
                    }
                    Err(_) => failed.push(format!("{task_id} (invalid JSON)")),
                },
                Ok(None) => failed.push(format!("{task_id} (not found)")),
                Err(error) => failed.push(format!("{task_id} ({error})")),
            }
        }

        if failed.is_empty() {
            Ok(())
        } else {
            Err(anyhow!("Prerequisites not met: {}", failed.join(", ")))
        }
    }

    fn persist_deliverable_if_needed(
        &self,
        background_task_id: Option<&str>,
        agent_id: &str,
        output: &str,
    ) -> Result<()> {
        if let Some(task_id) = background_task_id {
            self.save_task_deliverable(task_id, agent_id, output)?;
        }
        Ok(())
    }

    fn create_tool_output_dir_for_task(background_task_id: &str) -> Result<std::path::PathBuf> {
        let base_dir = crate::paths::ensure_restflow_dir()?.join("tool-output");
        std::fs::create_dir_all(&base_dir)?;
        let path = base_dir.join(background_task_id);
        std::fs::create_dir_all(&path)?;
        Ok(path)
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
        Self {
            storage,
            process_registry,
            auth_manager,
            subagent_tracker,
            subagent_definitions,
            subagent_config,
            skill_snapshot_cache: Arc::new(SkillSnapshotCache::default()),
            reply_sender: None,
            reply_sender_factory: None,
        }
    }

    /// Set a reply sender so the agent can send intermediate messages.
    pub fn with_reply_sender(mut self, sender: Arc<dyn ReplySender>) -> Self {
        self.reply_sender = Some(sender);
        self
    }

    /// Set a reply sender factory for execution-scoped contexts (for example
    /// background agents where each task has distinct routing semantics).
    pub fn with_reply_sender_factory(mut self, factory: Arc<dyn ReplySenderFactory>) -> Self {
        self.reply_sender_factory = Some(factory);
        self
    }
}

fn is_credential_error(error: &anyhow::Error) -> bool {
    is_authentication_classification(classify_execution_error(error))
}

mod background_execution;
mod model_resolution;
mod preflight;
mod session_execution;
mod tooling;

pub use session_execution::SessionTurnRuntimeOptions;

#[cfg(test)]
mod tests;
