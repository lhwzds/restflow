use std::sync::Arc;

use anyhow::{Result, anyhow};
use restflow_contracts::request::{
    AgentNode as ContractAgentNode, SubagentSpawnRequest as ContractSubagentSpawnRequest,
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
use crate::runtime::subagent::StorageBackedSubagentLookup;
use crate::services::background_agent_conversion::derive_conversion_input;
use crate::storage::agent::StoredAgent;
use restflow_tools::ToolError;
use restflow_traits::assessment::{
    AgentOperationAssessor, AssessmentModelRef, OperationAssessment, OperationAssessmentIntent,
    OperationAssessmentIssue, OperationAssessmentStatus,
};
use restflow_traits::boundary::subagent::spawn_request_from_contract;
use restflow_traits::store::{
    AgentCreateRequest, AgentUpdateRequest, BackgroundAgentControlRequest,
    BackgroundAgentConvertSessionRequest, BackgroundAgentCreateRequest,
    BackgroundAgentUpdateRequest,
};
use restflow_traits::subagent::{SpawnRequest, SubagentDefLookup};

#[derive(Clone)]
pub struct OperationAssessorAdapter {
    core: Arc<AppCore>,
}

impl OperationAssessorAdapter {
    pub fn new(core: Arc<AppCore>) -> Self {
        Self { core }
    }
}

#[async_trait::async_trait]
impl AgentOperationAssessor for OperationAssessorAdapter {
    async fn assess_agent_create(
        &self,
        request: AgentCreateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_agent_create(&self.core, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_agent_update(
        &self,
        request: AgentUpdateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_agent_update(&self.core, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_create(
        &self,
        request: BackgroundAgentCreateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_background_agent_create(&self.core, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_convert_session(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_background_agent_convert_session(&self.core, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_update(
        &self,
        request: BackgroundAgentUpdateRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_background_agent_update(&self.core, request)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_background_agent_control(
        &self,
        request: BackgroundAgentControlRequest,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_background_agent_control(&self.core, request)
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
        assess_background_agent_template(&self.core, operation, intent, agent_ids, template_mode)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_subagent_spawn(
        &self,
        operation: &str,
        request: ContractSubagentSpawnRequest,
        template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_subagent_spawn(&self.core, operation, request, template_mode)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }

    async fn assess_subagent_batch(
        &self,
        operation: &str,
        requests: Vec<ContractSubagentSpawnRequest>,
        template_mode: bool,
    ) -> std::result::Result<OperationAssessment, ToolError> {
        assess_subagent_batch(&self.core, operation, requests, template_mode)
            .await
            .map_err(|error| ToolError::Tool(error.to_string()))
    }
}

pub fn assessment_requires_confirmation(assessment: &OperationAssessment) -> bool {
    assessment.status == OperationAssessmentStatus::Warning && assessment.requires_confirmation
}

pub fn ensure_assessment_confirmed(
    assessment: &OperationAssessment,
    confirmation_token: Option<&str>,
) -> Result<()> {
    if !assessment_requires_confirmation(assessment) {
        return Ok(());
    }

    let expected = assessment
        .confirmation_token
        .as_deref()
        .ok_or_else(|| anyhow!("confirmation required"))?;
    let provided = confirmation_token
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

async fn build_auth(core: &Arc<AppCore>) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig {
        auto_discover: false,
        ..AuthManagerConfig::default()
    };
    let db = core.storage.get_db();
    let secrets = Arc::new(core.storage.secrets.clone());
    let profile_storage = AuthProfileStorage::new(db)?;
    let manager = AuthProfileManager::with_storage(config, secrets, Some(profile_storage));
    manager.initialize().await?;
    let _ = manager.discover().await;
    Ok(manager)
}

fn agent_has_local_credential(core: &Arc<AppCore>, agent: &AgentNode) -> bool {
    match agent.api_key_config.as_ref() {
        Some(ApiKeyConfig::Direct(value)) => !value.trim().is_empty(),
        Some(ApiKeyConfig::Secret(secret_name)) => {
            secret_or_env_exists(&core.storage.secrets, secret_name)
        }
        None => false,
    }
}

async fn provider_available(
    core: &Arc<AppCore>,
    auth_manager: &AuthProfileManager,
    provider: Provider,
) -> bool {
    auth_provider_available(auth_manager, provider, |key| {
        secret_or_env_exists(&core.storage.secrets, key)
    })
    .await
}

async fn resolve_model_from_stored_credentials(
    core: &Arc<AppCore>,
    auth_manager: &AuthProfileManager,
) -> Result<Option<ModelId>> {
    Ok(resolve_model_from_credentials(auth_manager, |key| {
        secret_or_env_exists(&core.storage.secrets, key)
    })
    .await)
}

fn to_assessment_model_ref(model_ref: ModelRef) -> AssessmentModelRef {
    AssessmentModelRef {
        provider: model_ref.provider.as_canonical_str().to_string(),
        model: model_ref.model.as_serialized_str().to_string(),
    }
}

fn finalize_assessment(mut assessment: OperationAssessment) -> OperationAssessment {
    if !assessment.blockers.is_empty() {
        assessment.status = OperationAssessmentStatus::Block;
        assessment.requires_confirmation = false;
        assessment.confirmation_token = None;
        return assessment;
    }

    if !assessment.warnings.is_empty() {
        assessment.status = OperationAssessmentStatus::Warning;
        assessment.requires_confirmation = true;
        assessment.confirmation_token = Some(build_confirmation_token(&assessment));
        return assessment;
    }

    assessment.status = OperationAssessmentStatus::Ok;
    assessment.requires_confirmation = false;
    assessment.confirmation_token = None;
    assessment
}

fn build_confirmation_token(assessment: &OperationAssessment) -> String {
    let payload = serde_json::json!({
        "operation": assessment.operation,
        "intent": assessment.intent,
        "effective_model_ref": assessment.effective_model_ref,
        "warnings": assessment.warnings,
        "blockers": assessment.blockers,
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

async fn load_agent(core: &Arc<AppCore>, id_or_prefix: &str) -> Result<StoredAgent> {
    let trimmed = id_or_prefix.trim();
    let resolved_id = if trimmed.eq_ignore_ascii_case("default") {
        core.storage.agents.resolve_default_agent_id()?
    } else {
        core.storage.agents.resolve_existing_agent_id(trimmed)?
    };
    core.storage
        .agents
        .get_agent(resolved_id.clone())?
        .ok_or_else(|| anyhow!("Agent not found: {resolved_id}"))
}

fn normalize_subagent_request(
    core: &Arc<AppCore>,
    request: ContractSubagentSpawnRequest,
) -> Result<SpawnRequest> {
    let definitions = StorageBackedSubagentLookup::new(core.storage.agents.clone());
    let available_agents = definitions.list_callable();
    spawn_request_from_contract(&available_agents, request)
        .map_err(|error| anyhow!(error.to_string()))
}

async fn assess_agent_node(
    core: &Arc<AppCore>,
    auth_manager: &AuthProfileManager,
    operation: &str,
    intent: OperationAssessmentIntent,
    agent: &AgentNode,
    subagent_parent_fallback: bool,
) -> Result<OperationAssessment> {
    let mut assessment = OperationAssessment::ok(operation.to_string(), intent.clone());

    if let Err(errors) = agent.validate() {
        assessment.blockers.extend(issues_from_validation(errors));
    }
    if let Err(errors) = agent.validate_async(core).await {
        assessment.blockers.extend(issues_from_validation(errors));
    }

    if !assessment.blockers.is_empty() {
        return Ok(finalize_assessment(assessment));
    }

    if let Some(model_ref) = agent.resolved_model_ref() {
        assessment.effective_model_ref = Some(to_assessment_model_ref(model_ref));
        if !provider_available(core, auth_manager, model_ref.provider).await
            && !agent_has_local_credential(core, agent)
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

    if subagent_parent_fallback {
        assessment.warnings.push(issue(
            "inherits_parent_model",
            "No explicit model is configured. This sub-agent will inherit the parent runtime model.",
            Some("model"),
            Some("Set model/model_ref when you need deterministic provider behavior."),
        ));
        return Ok(finalize_assessment(assessment));
    }

    match resolve_model_from_stored_credentials(core, auth_manager).await? {
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
    let auth_manager = build_auth(core).await?;
    let agent = parse_agent_node(request.agent)?;
    assess_agent_node(
        core,
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
    let auth_manager = build_auth(core).await?;
    let Some(agent_value) = request.agent else {
        return Ok(OperationAssessment::ok(
            "update_agent",
            OperationAssessmentIntent::Save,
        ));
    };
    let agent = parse_agent_node(agent_value)?;
    assess_agent_node(
        core,
        &auth_manager,
        "update_agent",
        OperationAssessmentIntent::Save,
        &agent,
        false,
    )
    .await
}

pub async fn assess_background_agent_create(
    core: &Arc<AppCore>,
    request: BackgroundAgentCreateRequest,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(core).await?;
    let stored_agent = load_agent(core, &request.agent_id).await?;
    assess_agent_node(
        core,
        &auth_manager,
        "create_background_agent",
        OperationAssessmentIntent::Save,
        &stored_agent.agent,
        false,
    )
    .await
}

pub async fn assess_background_agent_convert_session(
    core: &Arc<AppCore>,
    request: BackgroundAgentConvertSessionRequest,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(core).await?;
    let session = core
        .storage
        .chat_sessions
        .get(&request.session_id)?
        .ok_or_else(|| anyhow!("Session not found: {}", request.session_id))?;
    let intent = if request.run_now.unwrap_or(false) {
        OperationAssessmentIntent::Run
    } else {
        OperationAssessmentIntent::Save
    };
    if derive_conversion_input(request.input.clone(), &session.messages).is_none() {
        let mut assessment = OperationAssessment::ok("convert_session_to_background_agent", intent);
        assessment.blockers.push(issue(
            "missing_conversion_input",
            "Cannot convert session: no non-empty user message found; please provide input.",
            Some("input"),
            Some("Provide a non-empty input value before converting the session."),
        ));
        return Ok(finalize_assessment(assessment));
    }
    let stored_agent = load_agent(core, &session.agent_id).await?;
    assess_agent_node(
        core,
        &auth_manager,
        "convert_session_to_background_agent",
        intent,
        &stored_agent.agent,
        false,
    )
    .await
}

pub async fn assess_background_agent_update(
    core: &Arc<AppCore>,
    request: BackgroundAgentUpdateRequest,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(core).await?;
    let task_id = core
        .storage
        .background_agents
        .resolve_existing_task_id(&request.id)?;
    let task = core
        .storage
        .background_agents
        .get_task(&task_id)?
        .ok_or_else(|| anyhow!("Background agent not found: {task_id}"))?;
    let next_agent_id = request
        .agent_id
        .as_deref()
        .unwrap_or(task.agent_id.as_str());
    let stored_agent = load_agent(core, next_agent_id).await?;
    assess_agent_node(
        core,
        &auth_manager,
        "update_background_agent",
        OperationAssessmentIntent::Save,
        &stored_agent.agent,
        false,
    )
    .await
}

pub async fn assess_background_agent_control(
    core: &Arc<AppCore>,
    request: BackgroundAgentControlRequest,
) -> Result<OperationAssessment> {
    let action = request.action.trim().to_lowercase();
    if action != "run_now" && action != "run-now" && action != "runnow" {
        return Ok(OperationAssessment::ok(
            "control_background_agent",
            OperationAssessmentIntent::Run,
        ));
    }

    let auth_manager = build_auth(core).await?;
    let task_id = core
        .storage
        .background_agents
        .resolve_existing_task_id(&request.id)?;
    let task = core
        .storage
        .background_agents
        .get_task(&task_id)?
        .ok_or_else(|| anyhow!("Background agent not found: {task_id}"))?;
    let stored_agent = load_agent(core, &task.agent_id).await?;
    assess_agent_node(
        core,
        &auth_manager,
        "run_background_agent",
        OperationAssessmentIntent::Run,
        &stored_agent.agent,
        false,
    )
    .await
}

pub async fn assess_background_agent_template(
    core: &Arc<AppCore>,
    operation: &str,
    intent: OperationAssessmentIntent,
    agent_ids: Vec<String>,
    template_mode: bool,
) -> Result<OperationAssessment> {
    let auth_manager = build_auth(core).await?;
    let mut assessment = OperationAssessment::ok(operation.to_string(), intent.clone());

    for agent_id in agent_ids {
        match load_agent(core, &agent_id).await {
            Ok(agent) => {
                let child = assess_agent_node(
                    core,
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
    request: ContractSubagentSpawnRequest,
    template_mode: bool,
) -> Result<OperationAssessment> {
    let request = normalize_subagent_request(core, request)?;
    let auth_manager = build_auth(core).await?;
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

        if !provider_available(core, &auth_manager, requested_provider).await {
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
        let stored_agent = load_agent(core, agent_id).await?;
        return assess_agent_node(
            core,
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
        "This temporary sub-agent has no explicit model and will inherit the parent runtime model.",
        Some("model"),
        Some("Set model/provider to make this sub-agent deterministic."),
    ));
    Ok(finalize_assessment(assessment))
}

pub async fn assess_subagent_batch(
    core: &Arc<AppCore>,
    operation: &str,
    requests: Vec<ContractSubagentSpawnRequest>,
    template_mode: bool,
) -> Result<OperationAssessment> {
    let intent = if template_mode {
        OperationAssessmentIntent::Save
    } else {
        OperationAssessmentIntent::Run
    };
    let mut assessment = OperationAssessment::ok(operation.to_string(), intent);

    for (index, request) in requests.into_iter().enumerate() {
        let child = assess_subagent_spawn(core, operation, request, template_mode).await?;
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
            ContractSubagentSpawnRequest {
                task: "Summarize the workspace".to_string(),
                model: Some("gpt-5-mini".to_string()),
                model_provider: Some("openai".to_string()),
                ..ContractSubagentSpawnRequest::default()
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
            ContractSubagentSpawnRequest {
                task: "Summarize the workspace".to_string(),
                model: Some("gpt-5-mini".to_string()),
                model_provider: None,
                ..ContractSubagentSpawnRequest::default()
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
            vec![ContractSubagentSpawnRequest {
                task: "Summarize the workspace".to_string(),
                model: Some("gpt-5-mini".to_string()),
                model_provider: None,
                ..ContractSubagentSpawnRequest::default()
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
                confirmation_token: None,
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
                confirmation_token: None,
            },
        )
        .await
        .expect("assessment");

        assert_eq!(assessment.status, OperationAssessmentStatus::Block);
        assert_eq!(assessment.intent, OperationAssessmentIntent::Save);
        assert_eq!(assessment.blockers.len(), 1);
        assert_eq!(assessment.blockers[0].code, "missing_conversion_input");
    }
}
