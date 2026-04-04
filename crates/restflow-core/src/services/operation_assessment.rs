use std::sync::Arc;

use anyhow::{Result, anyhow};
use restflow_contracts::request::{
    AgentNode as ContractAgentNode, RunSpawnRequest as ContractRunSpawnRequest,
};
use restflow_storage::AuthProfileStorage;
use restflow_traits::ModelProvider as SharedModelProvider;
use sha2::{Digest, Sha256};

use crate::AppCore;
use crate::auth::{
    AuthManagerConfig, AuthProfileManager, provider_available as auth_provider_available,
    resolve_model_from_credentials, secret_or_env_exists,
};
use crate::models::{AgentNode, ApiKeyConfig, ModelId, ModelRef, Provider, ValidationError};
use crate::runtime::subagent::StorageBackedSubagentLookup as StorageBackedRunDefinitionLookup;
use crate::services::background_agent_conversion::derive_conversion_input;
use crate::storage::agent::StoredAgent;
use crate::storage::{
    AgentStorage, BackgroundAgentStorage, ChannelSessionBindingStorage, ConfigStorage,
    DeliverableStorage, ExecutionTraceStorage, KvStoreStorage, MemoryStorage, SecretStorage,
    SkillStorage, Storage, TerminalSessionStorage, TriggerStorage, WorkItemStorage,
};
use restflow_tools::ToolError;
use restflow_traits::assessment::{
    AgentOperationAssessor, AssessmentModelRef, OperationAssessment, OperationAssessmentIntent,
    OperationAssessmentIssue, OperationAssessmentStatus,
};
use restflow_traits::boundary::subagent::spawn_request_from_contract as run_spawn_request_from_contract;
use restflow_traits::store::{
    AgentCreateRequest, AgentUpdateRequest, TaskControlRequest, TaskConvertSessionRequest,
    TaskCreateRequest, TaskDeleteRequest, TaskUpdateRequest,
};
use restflow_traits::subagent::{
    SpawnRequest as RunSpawnRequest, SubagentDefLookup as RunDefinitionLookup,
};

#[derive(Clone)]
pub struct OperationAssessorAdapter {
    context: AssessmentContext,
}

#[derive(Clone)]
struct AssessmentContext {
    db: Arc<redb::Database>,
    secrets: SecretStorage,
    skills: SkillStorage,
    memory: MemoryStorage,
    chat_sessions: crate::storage::ChatSessionStorage,
    channel_session_bindings: ChannelSessionBindingStorage,
    execution_traces: ExecutionTraceStorage,
    kv_store: KvStoreStorage,
    work_items: WorkItemStorage,
    config: ConfigStorage,
    agents: AgentStorage,
    background_agents: BackgroundAgentStorage,
    triggers: TriggerStorage,
    terminal_sessions: TerminalSessionStorage,
    deliverables: DeliverableStorage,
}

impl AssessmentContext {
    fn from_core(core: &Arc<AppCore>) -> Self {
        Self::from_storage(core.storage.as_ref())
    }

    fn from_storage(storage: &Storage) -> Self {
        Self {
            db: storage.get_db(),
            secrets: storage.secrets.clone(),
            skills: storage.skills.clone(),
            memory: storage.memory.clone(),
            chat_sessions: storage.chat_sessions.clone(),
            channel_session_bindings: storage.channel_session_bindings.clone(),
            execution_traces: storage.execution_traces.clone(),
            kv_store: storage.kv_store.clone(),
            work_items: storage.work_items.clone(),
            config: storage.config.clone(),
            agents: storage.agents.clone(),
            background_agents: storage.background_agents.clone(),
            triggers: storage.triggers.clone(),
            terminal_sessions: storage.terminal_sessions.clone(),
            deliverables: storage.deliverables.clone(),
        }
    }
}

impl OperationAssessorAdapter {
    pub fn new(core: Arc<AppCore>) -> Self {
        Self {
            context: AssessmentContext::from_core(&core),
        }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        Self {
            context: AssessmentContext::from_storage(storage),
        }
    }
}

#[async_trait::async_trait]
impl AgentOperationAssessor for OperationAssessorAdapter {
    async fn assess_agent_create(
        &self,
        request: AgentCreateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_agent_create_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_agent_update(
        &self,
        request: AgentUpdateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_agent_update_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_create(
        &self,
        request: restflow_traits::store::BackgroundAgentCreateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_create_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_task_create(
        &self,
        request: TaskCreateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_create_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_convert_session(
        &self,
        request: restflow_traits::store::BackgroundAgentConvertSessionRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_convert_session_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_task_convert_session(
        &self,
        request: TaskConvertSessionRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_convert_session_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_update(
        &self,
        request: restflow_traits::store::BackgroundAgentUpdateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_update_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_task_update(
        &self,
        request: TaskUpdateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_update_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_delete(
        &self,
        request: restflow_traits::store::BackgroundAgentDeleteRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_delete_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_task_delete(
        &self,
        request: TaskDeleteRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_delete_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_control(
        &self,
        request: restflow_traits::store::BackgroundAgentControlRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_control_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_task_control(
        &self,
        request: TaskControlRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_control_with_context(&self.context, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_template(
        &self,
        operation: &str,
        intent: OperationAssessmentIntent,
        agent_ids: Vec<String>,
        template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_template_with_context(
            &self.context,
            operation,
            intent,
            agent_ids,
            template_mode,
        )
        .await
        .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_task_template(
        &self,
        operation: &str,
        intent: OperationAssessmentIntent,
        agent_ids: Vec<String>,
        template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_task_template_with_context(
            &self.context,
            operation,
            intent,
            agent_ids,
            template_mode,
        )
        .await
        .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_subagent_spawn(
        &self,
        operation: &str,
        request: ContractRunSpawnRequest,
        template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_run_spawn_with_context(&self.context, operation, request, template_mode)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_subagent_batch(
        &self,
        operation: &str,
        requests: Vec<ContractRunSpawnRequest>,
        template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_run_batch_with_context(&self.context, operation, requests, template_mode)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }
}

pub fn assessment_requires_confirmation(assessment: &OperationAssessment) -> bool {
    assessment.status == OperationAssessmentStatus::Warning && assessment.requires_confirmation
}

pub fn ensure_assessment_confirmed(
    assessment: &OperationAssessment,
    approval_id: Option<&str>,
) -> Result<()> {
    if !assessment_requires_confirmation(assessment) {
        return Ok(());
    }

    let expected = assessment
        .approval_id
        .as_deref()
        .ok_or_else(|| anyhow!("confirmation required"))?;
    let provided = approval_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("confirmation required"))?;
    if provided != expected {
        return Err(anyhow!("invalid confirmation token"));
    }
    Ok(())
}

pub fn assessment_summary(assessment: &OperationAssessment) -> String {
    let issues = match assessment.status {
        OperationAssessmentStatus::Block => &assessment.blockers,
        OperationAssessmentStatus::Warning => &assessment.warnings,
        OperationAssessmentStatus::Ok => return "Operation is ready".to_string(),
    };
    let summary = issues
        .iter()
        .map(|issue| issue.message.clone())
        .collect::<Vec<_>>()
        .join("; ");
    if summary.is_empty() {
        "Operation requires confirmation".to_string()
    } else {
        summary
    }
}

fn issue(
    code: impl Into<String>,
    message: impl Into<String>,
    field: Option<&str>,
    suggestion: Option<&str>,
) -> OperationAssessmentIssue {
    OperationAssessmentIssue {
        code: code.into(),
        message: message.into(),
        field: field.map(ToOwned::to_owned),
        suggestion: suggestion.map(ToOwned::to_owned),
    }
}

fn issues_from_validation(errors: Vec<ValidationError>) -> Vec<OperationAssessmentIssue> {
    errors
        .into_iter()
        .map(|error| OperationAssessmentIssue {
            code: "validation_error".to_string(),
            message: error.message,
            field: Some(error.field),
            suggestion: None,
        })
        .collect()
}

async fn build_auth(context: &AssessmentContext) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig {
        auto_discover: false,
        ..AuthManagerConfig::default()
    };
    let secrets = Arc::new(context.secrets.clone());
    let profile_storage = AuthProfileStorage::new(context.db.clone())?;
    let manager = AuthProfileManager::with_storage(config, secrets, Some(profile_storage));
    manager.initialize().await?;
    let _ = manager.discover().await;
    Ok(manager)
}

fn agent_has_local_credential(context: &AssessmentContext, agent: &AgentNode) -> bool {
    match agent.api_key_config.as_ref() {
        Some(ApiKeyConfig::Direct(value)) => !value.trim().is_empty(),
        Some(ApiKeyConfig::Secret(secret_name)) => {
            secret_or_env_exists(&context.secrets, secret_name)
        }
        None => false,
    }
}

async fn provider_available(
    context: &AssessmentContext,
    auth_manager: &AuthProfileManager,
    provider: Provider,
) -> bool {
    auth_provider_available(auth_manager, provider, |key| {
        secret_or_env_exists(&context.secrets, key)
    })
    .await
}

async fn resolve_model_from_stored_credentials(
    context: &AssessmentContext,
    auth_manager: &AuthProfileManager,
) -> Result<Option<ModelId>> {
    Ok(resolve_model_from_credentials(auth_manager, |key| {
        secret_or_env_exists(&context.secrets, key)
    })
    .await)
}

fn to_assessment_model_ref(model_ref: ModelRef) -> AssessmentModelRef {
    AssessmentModelRef {
        provider: model_ref.provider.as_canonical_str().to_string(),
        model: model_ref.model.as_serialized_str().to_string(),
    }
}

fn finalize_assessment(assessment: OperationAssessment) -> OperationAssessment {
    finalize_assessment_with_seed(assessment, None)
}

fn finalize_assessment_with_seed(
    mut assessment: OperationAssessment,
    confirmation_seed: Option<serde_json::Value>,
) -> OperationAssessment {
    if !assessment.blockers.is_empty() {
        assessment.status = OperationAssessmentStatus::Block;
        assessment.requires_confirmation = false;
        assessment.approval_id = None;
        return assessment;
    }

    if !assessment.warnings.is_empty() {
        assessment.status = OperationAssessmentStatus::Warning;
        assessment.requires_confirmation = true;
        assessment.approval_id = Some(build_approval_id(&assessment, confirmation_seed.as_ref()));
        return assessment;
    }

    assessment.status = OperationAssessmentStatus::Ok;
    assessment.requires_confirmation = false;
    assessment.approval_id = None;
    assessment
}

fn build_approval_id(
    assessment: &OperationAssessment,
    confirmation_seed: Option<&serde_json::Value>,
) -> String {
    let payload = serde_json::json!({
        "operation": assessment.operation,
        "intent": assessment.intent,
        "effective_model_ref": assessment.effective_model_ref,
        "warnings": assessment.warnings,
        "blockers": assessment.blockers,
        "confirmation_seed": confirmation_seed,
    });
    let encoded = serde_json::to_vec(&payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    hex::encode(hasher.finalize())
}

fn parse_agent_node(value: ContractAgentNode) -> Result<AgentNode> {
    AgentNode::try_from_contract_node(value)
        .map_err(|errors| anyhow!(crate::models::encode_validation_error(errors)))
}

async fn load_agent(context: &AssessmentContext, id_or_prefix: &str) -> Result<StoredAgent> {
    let trimmed = id_or_prefix.trim();
    let resolved_id = if trimmed.eq_ignore_ascii_case("default") {
        context.agents.resolve_default_agent_id()?
    } else {
        context.agents.resolve_existing_agent_id(trimmed)?
    };
    context
        .agents
        .get_agent(resolved_id.clone())?
        .ok_or_else(|| anyhow!("Agent not found: {resolved_id}"))
}

fn normalize_run_spawn_request(
    context: &AssessmentContext,
    request: ContractRunSpawnRequest,
) -> Result<RunSpawnRequest> {
    let definitions = StorageBackedRunDefinitionLookup::new(context.agents.clone());
    let available_agents = definitions.list_callable();
    run_spawn_request_from_contract(&available_agents, request)
        .map_err(|error| anyhow!(error.to_string()))
}

async fn validate_agent_async(
    context: &AssessmentContext,
    agent: &AgentNode,
) -> std::result::Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    let tool_registry = match crate::services::tool_registry::create_tool_registry(
        context.skills.clone(),
        context.memory.clone(),
        context.chat_sessions.clone(),
        context.channel_session_bindings.clone(),
        context.execution_traces.clone(),
        context.kv_store.clone(),
        context.work_items.clone(),
        context.secrets.clone(),
        context.config.clone(),
        context.agents.clone(),
        context.background_agents.clone(),
        context.triggers.clone(),
        context.terminal_sessions.clone(),
        context.deliverables.clone(),
        None,
        None,
        None,
    ) {
        Ok(registry) => registry,
        Err(err) => {
            errors.push(ValidationError::new(
                "tools",
                format!("Failed to create tool registry: {err}"),
            ));
            return Err(errors);
        }
    };

    if let Some(tools) = &agent.tools {
        for tool_name in tools {
            let normalized = tool_name.trim();
            if normalized.is_empty() {
                errors.push(ValidationError::new("tools", "tool name must not be empty"));
                continue;
            }
            if !tool_registry.has(normalized) {
                errors.push(ValidationError::new(
                    "tools",
                    format!("unknown tool: {}", normalized),
                ));
            }
        }
    }

    if let Some(skills) = &agent.skills {
        for skill_id in skills {
            let normalized = skill_id.trim();
            if normalized.is_empty() {
                errors.push(ValidationError::new("skills", "skill ID must not be empty"));
                continue;
            }
            match context.skills.exists(normalized) {
                Ok(true) => {}
                Ok(false) => errors.push(ValidationError::new(
                    "skills",
                    format!("unknown skill: {}", normalized),
                )),
                Err(err) => errors.push(ValidationError::new(
                    "skills",
                    format!("failed to verify skill '{}': {}", normalized, err),
                )),
            }
        }
    }

    if let Some(ApiKeyConfig::Secret(secret_name)) = &agent.api_key_config {
        let normalized = secret_name.trim();
        if !normalized.is_empty() {
            match context.secrets.has_available_secret(normalized) {
                Ok(true) => {}
                Ok(false) => errors.push(ValidationError::new(
                    "api_key_config",
                    format!("secret not found in storage or env: {}", normalized),
                )),
                Err(err) => errors.push(ValidationError::new(
                    "api_key_config",
                    format!("failed to verify secret '{}': {}", normalized, err),
                )),
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

async fn assess_agent_node(
    context: &AssessmentContext,
    auth_manager: &AuthProfileManager,
    operation: &str,
    intent: OperationAssessmentIntent,
    agent: &AgentNode,
    child_run_parent_fallback: bool,
) -> Result<OperationAssessment> {
    let mut assessment = OperationAssessment::ok(operation.to_string(), intent.clone());

    if let Err(errors) = agent.validate() {
        assessment.blockers.extend(issues_from_validation(errors));
    }
    if let Err(errors) = validate_agent_async(context, agent).await {
        assessment.blockers.extend(issues_from_validation(errors));
    }

    if !assessment.blockers.is_empty() {
        return Ok(finalize_assessment(assessment));
    }

    if let Some(model_ref) = agent.resolved_model_ref() {
        assessment.effective_model_ref = Some(to_assessment_model_ref(model_ref));
        if !provider_available(context, auth_manager, model_ref.provider).await
            && !agent_has_local_credential(context, agent)
        {
            let current_issue = issue(
                "provider_unavailable",
                format!(
                    "Provider '{}' is not configured in the current environment.",
                    model_ref.provider.as_canonical_str()
                ),
                Some("model_ref.provider"),
                Some("Configure a compatible API key or auth profile before running."),
            );
            match intent {
                OperationAssessmentIntent::Save => assessment.warnings.push(current_issue),
                OperationAssessmentIntent::Run => assessment.blockers.push(current_issue),
            }
        }
        return Ok(finalize_assessment(assessment));
    }

    if child_run_parent_fallback {
        assessment.warnings.push(issue(
            "inherits_parent_model",
            "No explicit model is configured. This child run will inherit the parent runtime model.",
            Some("model"),
            Some("Set model/model_ref when you need deterministic provider behavior."),
        ));
        return Ok(finalize_assessment(assessment));
    }

    match resolve_model_from_stored_credentials(context, auth_manager).await? {
        Some(model) => {
            let model_ref = ModelRef::from_model(model);
            assessment.effective_model_ref = Some(to_assessment_model_ref(model_ref));
            assessment.warnings.push(issue(
                "auto_model_resolution",
                format!(
                    "No explicit model is configured. Current runtime would resolve this agent to '{}'.",
                    model.as_serialized_str()
                ),
                Some("model"),
                Some("Set model/model_ref to make the agent deterministic."),
            ));
        }
        None => {
            let current_issue = issue(
                "auto_model_unresolved",
                "No explicit model is configured and no compatible credential is currently available.",
                Some("model"),
                Some("Set model/model_ref or configure a compatible API key/auth profile."),
            );
            match intent {
                OperationAssessmentIntent::Save => assessment.warnings.push(current_issue),
                OperationAssessmentIntent::Run => assessment.blockers.push(current_issue),
            }
        }
    }

    Ok(finalize_assessment(assessment))
}

fn merge_assessment(
    target: &mut OperationAssessment,
    child: OperationAssessment,
    context_prefix: &str,
) {
    if target.effective_model_ref.is_none() {
        target.effective_model_ref = child.effective_model_ref;
    }
    target
        .warnings
        .extend(child.warnings.into_iter().map(|mut issue| {
            issue.message = format!("{context_prefix}: {}", issue.message);
            issue
        }));
    target
        .blockers
        .extend(child.blockers.into_iter().map(|mut issue| {
            issue.message = format!("{context_prefix}: {}", issue.message);
            issue
        }));
}

pub async fn assess_agent_create(
    core: &Arc<AppCore>,
    request: AgentCreateRequest,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_agent_create_with_context(&context, request).await
}

async fn assess_agent_create_with_context(
    context: &AssessmentContext,
    request: AgentCreateRequest,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(context).await?;
    let agent = parse_agent_node(request.agent)?;
    assess_agent_node(
        context,
        &auth_manager,
        "create_agent",
        OperationAssessmentIntent::Save,
        &agent,
        false,
    )
    .await
}

pub async fn assess_agent_update(
    core: &Arc<AppCore>,
    request: AgentUpdateRequest,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_agent_update_with_context(&context, request).await
}

async fn assess_agent_update_with_context(
    context: &AssessmentContext,
    request: AgentUpdateRequest,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(context).await?;
    let Some(agent_value) = request.agent else {
        return Ok(OperationAssessment::ok(
            "update_agent",
            OperationAssessmentIntent::Save,
        ));
    };
    let agent = parse_agent_node(agent_value)?;
    assess_agent_node(
        context,
        &auth_manager,
        "update_agent",
        OperationAssessmentIntent::Save,
        &agent,
        false,
    )
    .await
}

pub async fn assess_task_create(
    core: &Arc<AppCore>,
    request: TaskCreateRequest,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_task_create_with_context(&context, request).await
}

pub async fn assess_background_agent_create(
    core: &Arc<AppCore>,
    request: restflow_traits::store::BackgroundAgentCreateRequest,
) -> Result<OperationAssessment> {
    assess_task_create(core, request).await
}

async fn assess_task_create_with_context(
    context: &AssessmentContext,
    request: TaskCreateRequest,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(context).await?;
    let stored_agent = load_agent(context, &request.agent_id).await?;
    assess_agent_node(
        context,
        &auth_manager,
        "create_task",
        OperationAssessmentIntent::Save,
        &stored_agent.agent,
        false,
    )
    .await
}

pub async fn assess_task_convert_session(
    core: &Arc<AppCore>,
    request: TaskConvertSessionRequest,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_task_convert_session_with_context(&context, request).await
}

pub async fn assess_background_agent_convert_session(
    core: &Arc<AppCore>,
    request: restflow_traits::store::BackgroundAgentConvertSessionRequest,
) -> Result<OperationAssessment> {
    assess_task_convert_session(core, request).await
}

async fn assess_task_convert_session_with_context(
    context: &AssessmentContext,
    request: TaskConvertSessionRequest,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(context).await?;
    let session = context
        .chat_sessions
        .get(&request.session_id)?
        .ok_or_else(|| anyhow!("Session not found: {}", request.session_id))?;
    let intent = if request.run_now.unwrap_or(false) {
        OperationAssessmentIntent::Run
    } else {
        OperationAssessmentIntent::Save
    };
    if derive_conversion_input(request.input.clone(), &session.messages).is_none() {
        let mut assessment = OperationAssessment::ok("convert_session_to_task", intent);
        assessment.blockers.push(issue(
            "missing_conversion_input",
            "Cannot convert session: no non-empty user message found; please provide input.",
            Some("input"),
            Some("Provide a non-empty input value before converting the session."),
        ));
        return Ok(finalize_assessment(assessment));
    }
    let stored_agent = load_agent(context, &session.agent_id).await?;
    let assessment = assess_agent_node(
        context,
        &auth_manager,
        "convert_session_to_task",
        intent,
        &stored_agent.agent,
        false,
    )
    .await?;

    Ok(finalize_assessment_with_seed(
        assessment,
        Some(serde_json::json!({
            "session_id": request.session_id,
        })),
    ))
}

pub async fn assess_task_update(
    core: &Arc<AppCore>,
    request: TaskUpdateRequest,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_task_update_with_context(&context, request).await
}

pub async fn assess_background_agent_update(
    core: &Arc<AppCore>,
    request: restflow_traits::store::BackgroundAgentUpdateRequest,
) -> Result<OperationAssessment> {
    assess_task_update(core, request).await
}

async fn assess_task_update_with_context(
    context: &AssessmentContext,
    request: TaskUpdateRequest,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(context).await?;
    let task_id = context
        .background_agents
        .resolve_existing_task_id(&request.id)?;
    let task = context
        .background_agents
        .get_task(&task_id)?
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))?;
    let next_agent_id = request
        .agent_id
        .as_deref()
        .unwrap_or(task.agent_id.as_str());
    let stored_agent = load_agent(context, next_agent_id).await?;
    assess_agent_node(
        context,
        &auth_manager,
        "update_task",
        OperationAssessmentIntent::Save,
        &stored_agent.agent,
        false,
    )
    .await
}

pub async fn assess_task_delete(
    core: &Arc<AppCore>,
    request: TaskDeleteRequest,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_task_delete_with_context(&context, request).await
}

pub async fn assess_background_agent_delete(
    core: &Arc<AppCore>,
    request: restflow_traits::store::BackgroundAgentDeleteRequest,
) -> Result<OperationAssessment> {
    assess_task_delete(core, request).await
}

async fn assess_task_delete_with_context(
    context: &AssessmentContext,
    request: TaskDeleteRequest,
) -> Result<OperationAssessment> {
    let task_id = context
        .background_agents
        .resolve_existing_task_id(&request.id)?;
    let task = context
        .background_agents
        .get_task(&task_id)?
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))?;
    let mut assessment = OperationAssessment::ok("delete_task", OperationAssessmentIntent::Save);
    assessment.warnings.push(issue(
        "destructive_delete",
        format!(
            "Deleting task '{}' removes its persisted definition and run history.",
            task.name
        ),
        Some("id"),
        Some("Confirm the deletion only if you intend to permanently remove this task."),
    ));
    Ok(finalize_assessment_with_seed(
        assessment,
        Some(serde_json::json!({
            "task_id": task.id,
        })),
    ))
}

pub async fn assess_task_control(
    core: &Arc<AppCore>,
    request: TaskControlRequest,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_task_control_with_context(&context, request).await
}

pub async fn assess_background_agent_control(
    core: &Arc<AppCore>,
    request: restflow_traits::store::BackgroundAgentControlRequest,
) -> Result<OperationAssessment> {
    assess_task_control(core, request).await
}

async fn assess_task_control_with_context(
    context: &AssessmentContext,
    request: TaskControlRequest,
) -> Result<OperationAssessment> {
    let action = request.action.trim().to_lowercase();
    if action != "run_now" && action != "run-now" && action != "runnow" {
        return Ok(OperationAssessment::ok("control_task", OperationAssessmentIntent::Run));
    }

    let auth_manager = build_auth(context).await?;
    let task_id = context
        .background_agents
        .resolve_existing_task_id(&request.id)?;
    let task = context
        .background_agents
        .get_task(&task_id)?
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))?;
    let stored_agent = load_agent(context, &task.agent_id).await?;
    assess_agent_node(
        context,
        &auth_manager,
        "run_task",
        OperationAssessmentIntent::Run,
        &stored_agent.agent,
        false,
    )
    .await
}

pub async fn assess_task_template(
    core: &Arc<AppCore>,
    operation: &str,
    intent: OperationAssessmentIntent,
    agent_ids: Vec<String>,
    template_mode: bool,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_task_template_with_context(&context, operation, intent, agent_ids, template_mode).await
}

pub async fn assess_background_agent_template(
    core: &Arc<AppCore>,
    operation: &str,
    intent: OperationAssessmentIntent,
    agent_ids: Vec<String>,
    template_mode: bool,
) -> Result<OperationAssessment> {
    assess_task_template(core, operation, intent, agent_ids, template_mode).await
}

async fn assess_task_template_with_context(
    context: &AssessmentContext,
    operation: &str,
    intent: OperationAssessmentIntent,
    agent_ids: Vec<String>,
    template_mode: bool,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(context).await?;
    let mut assessment = OperationAssessment::ok(operation.to_string(), intent.clone());

    for agent_id in agent_ids {
        match load_agent(context, &agent_id).await {
            Ok(agent) => {
                let child = assess_agent_node(
                    context,
                    &auth_manager,
                    operation,
                    intent.clone(),
                    &agent.agent,
                    false,
                )
                .await?;
                merge_assessment(&mut assessment, child, &format!("Agent '{}'", agent.name));
            }
            Err(error) => {
                let destination = if template_mode { "template" } else { "batch" };
                assessment.blockers.push(issue(
                    "agent_not_found",
                    format!(
                        "Referenced agent '{}' for {} operation was not found: {}",
                        agent_id, destination, error
                    ),
                    Some("agent_id"),
                    Some("Choose an existing agent before continuing."),
                ));
            }
        }
    }

    Ok(finalize_assessment(assessment))
}

pub async fn assess_subagent_spawn(
    core: &Arc<AppCore>,
    operation: &str,
    request: ContractRunSpawnRequest,
    template_mode: bool,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_run_spawn_with_context(&context, operation, request, template_mode).await
}

async fn assess_run_spawn_with_context(
    context: &AssessmentContext,
    operation: &str,
    request: ContractRunSpawnRequest,
    template_mode: bool,
) -> Result<OperationAssessment> {
    let request = normalize_run_spawn_request(context, request)?;
    let auth_manager = build_auth(context).await?;
    let intent = if template_mode {
        OperationAssessmentIntent::Save
    } else {
        OperationAssessmentIntent::Run
    };

    if let (Some(model), Some(provider)) = (
        request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        request
            .model_provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    ) {
        let normalized_model = ModelId::normalize_model_id(model)
            .ok_or_else(|| anyhow!("Unsupported model identifier: {}", model))?;
        let requested_provider = SharedModelProvider::parse_alias(provider)
            .map(Provider::from_model_provider)
            .ok_or_else(|| anyhow!("Unsupported provider identifier: {}", provider))?;
        let resolved_model = ModelId::for_provider_and_model(requested_provider, &normalized_model)
            .ok_or_else(|| anyhow!("Unsupported model identifier: {}", normalized_model))?;
        let model_ref = ModelRef::from_model(resolved_model);
        let mut assessment = OperationAssessment::ok(operation.to_string(), intent.clone());
        assessment.effective_model_ref = Some(to_assessment_model_ref(model_ref));

        if model_ref.provider != requested_provider {
            assessment.blockers.push(issue(
                "model_provider_mismatch",
                format!(
                    "Model '{}' does not belong to provider '{}'.",
                    resolved_model.as_serialized_str(),
                    requested_provider.as_canonical_str()
                ),
                Some("provider"),
                Some("Choose a model that belongs to the selected provider."),
            ));
            return Ok(finalize_assessment(assessment));
        }

        if !provider_available(context, &auth_manager, requested_provider).await {
            let current_issue = issue(
                "provider_unavailable",
                format!(
                    "Provider '{}' is not configured in the current environment.",
                    requested_provider.as_canonical_str()
                ),
                Some("provider"),
                Some("Configure a compatible API key or auth profile before running."),
            );
            match intent {
                OperationAssessmentIntent::Save => assessment.warnings.push(current_issue),
                OperationAssessmentIntent::Run => assessment.blockers.push(current_issue),
            }
        }

        return Ok(finalize_assessment(assessment));
    }

    if let Some(agent_id) = request.agent_id.as_deref() {
        let stored_agent = load_agent(context, agent_id).await?;
        return assess_agent_node(
            context,
            &auth_manager,
            operation,
            intent,
            &stored_agent.agent,
            true,
        )
        .await;
    }

    let mut assessment = OperationAssessment::ok(operation.to_string(), intent);
    assessment.warnings.push(issue(
        "inherits_parent_model",
        "This temporary child run has no explicit model and will inherit the parent runtime model.",
        Some("model"),
        Some("Set model/provider to make this child run deterministic."),
    ));
    Ok(finalize_assessment(assessment))
}

pub async fn assess_subagent_batch(
    core: &Arc<AppCore>,
    operation: &str,
    requests: Vec<ContractRunSpawnRequest>,
    template_mode: bool,
) -> Result<OperationAssessment> {
    let context = AssessmentContext::from_core(core);
    assess_run_batch_with_context(&context, operation, requests, template_mode).await
}

async fn assess_run_batch_with_context(
    context: &AssessmentContext,
    operation: &str,
    requests: Vec<ContractRunSpawnRequest>,
    template_mode: bool,
) -> Result<OperationAssessment> {
    let intent = if template_mode {
        OperationAssessmentIntent::Save
    } else {
        OperationAssessmentIntent::Run
    };
    let mut assessment = OperationAssessment::ok(operation.to_string(), intent);

    for (index, request) in requests.into_iter().enumerate() {
        let child =
            assess_run_spawn_with_context(context, operation, request, template_mode).await?;
        merge_assessment(&mut assessment, child, &format!("Worker {}", index + 1));
    }

    Ok(finalize_assessment(assessment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ApiKeyConfig, ChatMessage, ModelRef};
    use crate::prompt_files;
    use crate::services::agent::create_agent;
    use restflow_contracts::request::{ApiKeyConfig as ContractApiKeyConfig, WireModelRef};
    use restflow_traits::{
        BackgroundAgentConvertSessionRequest, BackgroundAgentDeleteRequest,
    };
    use tempfile::tempdir;

    struct AgentsDirEnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl AgentsDirEnvGuard {
        fn new() -> Self {
            Self {
                _lock: prompt_files::agents_dir_env_lock(),
            }
        }
    }

    impl Drop for AgentsDirEnvGuard {
        fn drop(&mut self) {
            unsafe { std::env::remove_var(prompt_files::AGENTS_DIR_ENV) };
        }
    }

    #[allow(clippy::await_holding_lock)]
    async fn create_test_core_isolated() -> (
        Arc<AppCore>,
        tempfile::TempDir,
        tempfile::TempDir,
        AgentsDirEnvGuard,
    ) {
        let env_guard = AgentsDirEnvGuard::new();
        let temp_db = tempdir().expect("temp db");
        let temp_agents = tempdir().expect("temp agents");
        unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, temp_agents.path()) };
        let db_path = temp_db.path().join("test.db");
        let core = Arc::new(
            AppCore::new(db_path.to_str().expect("db path"))
                .await
                .unwrap(),
        );
        (core, temp_db, temp_agents, env_guard)
    }

    fn create_test_agent_node(prompt: &str) -> AgentNode {
        AgentNode {
            model: Some(ModelId::ClaudeSonnet4_5),
            model_ref: Some(ModelRef::from_model(ModelId::ClaudeSonnet4_5)),
            prompt: Some(prompt.to_string()),
            temperature: Some(0.7),
            codex_cli_reasoning_effort: None,
            codex_cli_execution_mode: None,
            api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
            tools: Some(vec!["http_request".to_string()]),
            skills: None,
            skill_variables: None,
            skill_preflight_policy_mode: None,
            model_routing: None,
        }
    }

    #[tokio::test]
    async fn assess_agent_create_accepts_valid_contract_agent_node() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let assessment = assess_agent_create(
            &core,
            AgentCreateRequest {
                name: "Typed Agent".to_string(),
                agent: ContractAgentNode {
                    model_ref: Some(WireModelRef {
                        provider: "openai".to_string(),
                        model: "gpt-5-mini".to_string(),
                    }),
                    api_key_config: Some(ContractApiKeyConfig::Direct("test-key".to_string())),
                    prompt: Some("hello".to_string()),
                    ..ContractAgentNode::default()
                },
            },
        )
        .await
        .expect("assessment should succeed");

        assert_eq!(assessment.status, OperationAssessmentStatus::Ok);
        assert_eq!(
            assessment
                .effective_model_ref
                .as_ref()
                .map(|model_ref| model_ref.provider.as_str()),
            Some("openai")
        );
    }

    #[tokio::test]
    async fn assess_agent_create_rejects_invalid_model_ref() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let error = assess_agent_create(
            &core,
            AgentCreateRequest {
                name: "Bad Agent".to_string(),
                agent: ContractAgentNode {
                    model_ref: Some(WireModelRef {
                        provider: "openai".to_string(),
                        model: "claude-sonnet-4".to_string(),
                    }),
                    ..ContractAgentNode::default()
                },
            },
        )
        .await
        .expect_err("invalid model_ref should fail");

        let message = error.to_string();
        assert!(message.contains("validation_error"));
        assert!(message.contains("model_ref"));
    }

    #[tokio::test]
    async fn assess_agent_update_rejects_conflicting_model_fields() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let error = assess_agent_update(
            &core,
            AgentUpdateRequest {
                id: "agent-1".to_string(),
                name: None,
                agent: Some(ContractAgentNode {
                    model: Some("gpt-5-mini".to_string()),
                    model_ref: Some(WireModelRef {
                        provider: "anthropic".to_string(),
                        model: "claude-sonnet-4".to_string(),
                    }),
                    ..ContractAgentNode::default()
                }),
            },
        )
        .await
        .expect_err("conflicting model fields should fail");

        let message = error.to_string();
        assert!(message.contains("validation_error"));
        assert!(message.contains("model_ref"));
    }

    #[tokio::test]
    async fn assess_subagent_spawn_accepts_contract_request_and_sets_effective_model_ref() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let assessment = assess_subagent_spawn(
            &core,
            "spawn_subagent",
            ContractRunSpawnRequest {
                task: "Summarize the workspace".to_string(),
                model: Some("gpt-5-mini".to_string()),
                model_provider: Some("openai".to_string()),
                ..ContractRunSpawnRequest::default()
            },
            true,
        )
        .await
        .expect("assessment should succeed for a valid contract request");

        assert!(matches!(
            assessment.status,
            OperationAssessmentStatus::Ok | OperationAssessmentStatus::Warning
        ));
        assert_eq!(
            assessment
                .effective_model_ref
                .as_ref()
                .map(|model_ref| model_ref.provider.as_str()),
            Some("openai")
        );
        assert_eq!(
            assessment
                .effective_model_ref
                .as_ref()
                .map(|model_ref| model_ref.model.as_str()),
            Some("gpt-5-mini")
        );
    }

    #[tokio::test]
    async fn assess_subagent_spawn_rejects_invalid_contract_request_before_runtime() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let error = assess_subagent_spawn(
            &core,
            "spawn_subagent",
            ContractRunSpawnRequest {
                task: "Summarize the workspace".to_string(),
                model: Some("gpt-5-mini".to_string()),
                model_provider: None,
                ..ContractRunSpawnRequest::default()
            },
            false,
        )
        .await
        .expect_err("model/provider mismatch should fail at the boundary");

        assert!(
            error
                .to_string()
                .contains("requires both 'model' and 'provider'")
        );
    }

    #[tokio::test]
    async fn assess_subagent_batch_rejects_invalid_contract_requests() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let error = assess_subagent_batch(
            &core,
            "spawn_subagent_batch",
            vec![ContractRunSpawnRequest {
                task: "Summarize the workspace".to_string(),
                model: Some("gpt-5-mini".to_string()),
                model_provider: None,
                ..ContractRunSpawnRequest::default()
            }],
            false,
        )
        .await
        .expect_err("invalid batch request should fail at the boundary");

        assert!(
            error
                .to_string()
                .contains("requires both 'model' and 'provider'")
        );
    }

    #[tokio::test]
    async fn assess_background_agent_convert_session_defaults_run_now_to_save() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let created = create_agent(
            &core,
            "Assessment Agent".to_string(),
            create_test_agent_node("Assess conversions"),
        )
        .await
        .expect("agent");

        let mut session = crate::models::ChatSession::new(
            created.id.clone(),
            ModelId::ClaudeSonnet4_5.as_serialized_str().to_string(),
        );
        session.add_message(ChatMessage::user("Summarize this thread"));
        core.storage
            .chat_sessions
            .create(&session)
            .expect("session");

        let assessment = assess_background_agent_convert_session(
            &core,
            BackgroundAgentConvertSessionRequest {
                session_id: session.id.clone(),
                name: None,
                schedule: None,
                input: None,
                timeout_secs: None,
                durability_mode: None,
                memory: None,
                memory_scope: None,
                resource_limits: None,
                run_now: None,
                preview: false,
                approval_id: None,
            },
        )
        .await
        .expect("assessment");

        assert_eq!(assessment.intent, OperationAssessmentIntent::Save);
    }

    #[tokio::test]
    async fn assess_background_agent_convert_session_blocks_when_input_cannot_be_derived() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let created = create_agent(
            &core,
            "Assessment Agent".to_string(),
            create_test_agent_node("Assess conversions"),
        )
        .await
        .expect("agent");

        let session = crate::models::ChatSession::new(
            created.id.clone(),
            ModelId::ClaudeSonnet4_5.as_serialized_str().to_string(),
        );
        core.storage
            .chat_sessions
            .create(&session)
            .expect("session");

        let assessment = assess_background_agent_convert_session(
            &core,
            BackgroundAgentConvertSessionRequest {
                session_id: session.id.clone(),
                name: None,
                schedule: None,
                input: None,
                timeout_secs: None,
                durability_mode: None,
                memory: None,
                memory_scope: None,
                resource_limits: None,
                run_now: None,
                preview: false,
                approval_id: None,
            },
        )
        .await
        .expect("assessment");

        assert_eq!(assessment.status, OperationAssessmentStatus::Block);
        assert_eq!(assessment.intent, OperationAssessmentIntent::Save);
        assert_eq!(assessment.blockers.len(), 1);
        assert_eq!(assessment.blockers[0].code, "missing_conversion_input");
    }

    #[tokio::test]
    async fn assess_background_agent_convert_session_approval_id_is_bound_to_session() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let created = create_agent(
            &core,
            "Warning Agent".to_string(),
            AgentNode {
                prompt: Some("Assess conversions".to_string()),
                ..AgentNode::default()
            },
        )
        .await
        .expect("agent");

        let mut first_session = crate::models::ChatSession::new(
            created.id.clone(),
            ModelId::ClaudeSonnet4_5.as_serialized_str().to_string(),
        );
        first_session.add_message(ChatMessage::user("Summarize this session"));
        core.storage
            .chat_sessions
            .create(&first_session)
            .expect("first session");

        let mut second_session = crate::models::ChatSession::new(
            created.id.clone(),
            ModelId::ClaudeSonnet4_5.as_serialized_str().to_string(),
        );
        second_session.add_message(ChatMessage::user("Summarize this session"));
        core.storage
            .chat_sessions
            .create(&second_session)
            .expect("second session");

        let first_assessment = assess_background_agent_convert_session(
            &core,
            BackgroundAgentConvertSessionRequest {
                session_id: first_session.id.clone(),
                name: None,
                schedule: None,
                input: None,
                timeout_secs: None,
                durability_mode: None,
                memory: None,
                memory_scope: None,
                resource_limits: None,
                run_now: None,
                preview: false,
                approval_id: None,
            },
        )
        .await
        .expect("first assessment");
        let second_assessment = assess_background_agent_convert_session(
            &core,
            BackgroundAgentConvertSessionRequest {
                session_id: second_session.id.clone(),
                name: None,
                schedule: None,
                input: None,
                timeout_secs: None,
                durability_mode: None,
                memory: None,
                memory_scope: None,
                resource_limits: None,
                run_now: None,
                preview: false,
                approval_id: None,
            },
        )
        .await
        .expect("second assessment");

        assert_eq!(first_assessment.status, OperationAssessmentStatus::Warning);
        assert_eq!(second_assessment.status, OperationAssessmentStatus::Warning);
        assert_ne!(first_assessment.approval_id, second_assessment.approval_id);
    }

    #[tokio::test]
    async fn assess_background_agent_convert_session_approval_id_is_stable_for_same_session() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let created = create_agent(
            &core,
            "Warning Agent".to_string(),
            AgentNode {
                prompt: Some("Assess conversions".to_string()),
                ..AgentNode::default()
            },
        )
        .await
        .expect("agent");

        let mut session = crate::models::ChatSession::new(
            created.id.clone(),
            ModelId::ClaudeSonnet4_5.as_serialized_str().to_string(),
        );
        session.add_message(ChatMessage::user("Summarize this session"));
        core.storage
            .chat_sessions
            .create(&session)
            .expect("session");

        let request = BackgroundAgentConvertSessionRequest {
            session_id: session.id.clone(),
            name: None,
            schedule: None,
            input: None,
            timeout_secs: None,
            durability_mode: None,
            memory: None,
            memory_scope: None,
            resource_limits: None,
            run_now: None,
            preview: false,
            approval_id: None,
        };

        let first_assessment = assess_background_agent_convert_session(&core, request.clone())
            .await
            .expect("first assessment");
        let second_assessment = assess_background_agent_convert_session(&core, request)
            .await
            .expect("second assessment");

        assert_eq!(first_assessment.status, OperationAssessmentStatus::Warning);
        assert_eq!(first_assessment.approval_id, second_assessment.approval_id);
    }

    #[tokio::test]
    async fn assess_background_agent_delete_returns_warning_with_bound_token() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let created = create_agent(
            &core,
            "Delete Warning Agent".to_string(),
            create_test_agent_node("Assess deletions"),
        )
        .await
        .expect("agent");

        let task = core
            .storage
            .background_agents
            .create_background_agent(crate::models::BackgroundAgentSpec {
                name: "Delete Target".to_string(),
                agent_id: created.id.clone(),
                chat_session_id: None,
                description: None,
                input: Some("run".to_string()),
                input_template: None,
                schedule: crate::models::BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("task");

        let assessment = assess_background_agent_delete(
            &core,
            BackgroundAgentDeleteRequest {
                id: task.id.clone(),
                preview: true,
                approval_id: None,
            },
        )
        .await
        .expect("assessment");

        assert_eq!(assessment.status, OperationAssessmentStatus::Warning);
        assert_eq!(assessment.intent, OperationAssessmentIntent::Save);
        assert_eq!(assessment.warnings[0].code, "destructive_delete");
        assert!(assessment.approval_id.is_some());
    }

    #[tokio::test]
    async fn assess_task_convert_session_matches_background_behavior() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let created = create_agent(
            &core,
            "Task Assessment Agent".to_string(),
            create_test_agent_node("Assess task conversions"),
        )
        .await
        .expect("agent");

        let mut session = crate::models::ChatSession::new(
            created.id.clone(),
            ModelId::ClaudeSonnet4_5.as_serialized_str().to_string(),
        );
        session.add_message(ChatMessage::user("Summarize this task"));
        core.storage
            .chat_sessions
            .create(&session)
            .expect("session");

        let assessor = OperationAssessorAdapter::from_storage(core.storage.as_ref());
        let assessment = assessor
            .assess_task_convert_session(TaskConvertSessionRequest {
                session_id: session.id.clone(),
                name: None,
                schedule: None,
                input: None,
                timeout_secs: None,
                durability_mode: None,
                memory: None,
                memory_scope: None,
                resource_limits: None,
                run_now: None,
                preview: false,
                approval_id: None,
            })
            .await
            .expect("task assessment");

        assert_eq!(assessment.intent, OperationAssessmentIntent::Save);
        assert_eq!(assessment.operation, "convert_session_to_task");
    }
}
