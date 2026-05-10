use std::sync::Arc;

use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};
use types::ModelProvider as SharedModelProvider;
use types::request::{AgentNode as ContractAgentNode, RunSpawnRequest as ContractRunSpawnRequest};

use crate::AgentStorage;
use crate::AppCore;
use crate::StoredAgent;
use crate::auth::{
    AuthManagerConfig, AuthProfileManager, provider_available as auth_provider_available,
    resolve_model_from_credentials, secret_exists,
};
use crate::storage::{ConfigStorage, SecretStorage, Storage};
use crate::tools::ToolError;
use types::assessment::{
    AgentOperationAssessor, AssessmentModelRef, OperationAssessment, OperationAssessmentIntent,
    OperationAssessmentIssue, OperationAssessmentStatus,
};
use types::store::{AgentCreateRequest, AgentUpdateRequest};
use types::subagent::spawn_request_from_contract as run_spawn_request_from_contract;
use types::subagent::{SpawnRequest as RunSpawnRequest, SubagentDefSummary};
use types::{AgentNode, ApiKeyConfig, ModelId, ModelRef, Provider, ValidationError};

#[derive(Clone)]
pub struct OperationAssessorAdapter {
    context: AssessmentContext,
}

#[derive(Clone)]
struct AssessmentContext {
    secrets: SecretStorage,
    config: ConfigStorage,
    agents: AgentStorage,
}

impl AssessmentContext {
    fn from_core(core: &Arc<AppCore>) -> Self {
        Self::from_storage(core.storage.as_ref())
    }

    fn from_storage(storage: &Storage) -> Self {
        Self {
            secrets: storage.secrets.clone(),
            config: storage.config.clone(),
            agents: storage.agents.clone(),
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
    let secrets = Arc::new(context.secrets.clone());
    let manager = AuthProfileManager::with_config(AuthManagerConfig::default(), secrets);
    manager.initialize().await?;
    Ok(manager)
}

fn agent_has_local_credential(context: &AssessmentContext, agent: &AgentNode) -> bool {
    match agent.api_key_config.as_ref() {
        Some(ApiKeyConfig::Direct(value)) => !value.trim().is_empty(),
        Some(ApiKeyConfig::Secret(secret_name)) => secret_exists(&context.secrets, secret_name),
        None => false,
    }
}

async fn provider_available(
    context: &AssessmentContext,
    auth_manager: &AuthProfileManager,
    provider: Provider,
) -> bool {
    auth_provider_available(auth_manager, provider, |key| {
        secret_exists(&context.secrets, key)
    })
    .await
}

async fn resolve_model_from_stored_credentials(
    context: &AssessmentContext,
    auth_manager: &AuthProfileManager,
) -> Result<Option<ModelId>> {
    Ok(
        resolve_model_from_credentials(auth_manager, |key| secret_exists(&context.secrets, key))
            .await,
    )
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
        .map_err(|errors| anyhow!(types::encode_validation_error(errors)))
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
    let available_agents = context
        .agents
        .list_agents()?
        .into_iter()
        .map(|agent| SubagentDefSummary {
            id: agent.id,
            name: agent.name,
            description: "File-backed agent".to_string(),
            tags: Vec::new(),
        })
        .collect::<Vec<_>>();
    run_spawn_request_from_contract(&available_agents, request)
        .map_err(|error| anyhow!(error.to_string()))
}

async fn validate_agent_async(
    context: &AssessmentContext,
    agent: &AgentNode,
) -> std::result::Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    let tool_registry = match crate::services::tool_registry::create_tool_registry(
        context.config.clone(),
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
            if !tool_registry.has(normalized) && !is_subagent_tool_name(normalized) {
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
            match crate::services::skills::skill_exists_in_catalog(normalized) {
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
                    format!("secret not found in storage: {}", normalized),
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

fn is_subagent_tool_name(name: &str) -> bool {
    matches!(
        name,
        "spawn_subagent" | "spawn_subagent_batch" | "wait_subagents" | "list_subagents"
    )
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
        if matches!(intent, OperationAssessmentIntent::Save) {
            assessment.warnings.push(issue(
                "inherits_parent_model",
                "No explicit model is configured. This child run will inherit the parent runtime model.",
                Some("model_ref"),
                Some("Set model_ref when you need deterministic provider behavior."),
            ));
        }
        return Ok(finalize_assessment(assessment));
    }

    if matches!(intent, OperationAssessmentIntent::Save) {
        return Ok(finalize_assessment(assessment));
    }

    match resolve_model_from_stored_credentials(context, auth_manager).await? {
        Some(model) => {
            let model_ref = ModelRef::from_model(model);
            assessment.effective_model_ref = Some(to_assessment_model_ref(model_ref));
        }
        None => {
            let current_issue = issue(
                "auto_model_unresolved",
                "No explicit model is configured and no compatible credential is currently available.",
                Some("model_ref"),
                Some("Set model_ref or configure a compatible API key/auth profile."),
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
    if matches!(assessment.intent, OperationAssessmentIntent::Save) {
        assessment.warnings.push(issue(
            "inherits_parent_model",
            "This temporary child run has no explicit model and will inherit the parent runtime model.",
            Some("model_ref"),
            Some("Set model_ref to make this child run deterministic."),
        ));
    }
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
    use crate::test_support::RestflowTestEnv;
    use types::request::{ApiKeyConfig as ContractApiKeyConfig, WireModelRef};

    async fn create_test_core_isolated() -> (Arc<AppCore>, RestflowTestEnv) {
        let env = RestflowTestEnv::new();
        let db_path = env.db_path("test.db");
        let core = Arc::new(
            AppCore::new(db_path.to_str().expect("db path"))
                .await
                .unwrap(),
        );
        (core, env)
    }

    #[tokio::test]
    async fn assess_agent_create_accepts_valid_contract_agent_node() {
        let (core, _env) = create_test_core_isolated().await;
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
    async fn assess_agent_create_accepts_subagent_tools() {
        let (core, _env) = create_test_core_isolated().await;
        let assessment = assess_agent_create(
            &core,
            AgentCreateRequest {
                name: "Subagent Coordinator".to_string(),
                agent: ContractAgentNode {
                    model_ref: Some(WireModelRef {
                        provider: "openai".to_string(),
                        model: "gpt-5-mini".to_string(),
                    }),
                    api_key_config: Some(ContractApiKeyConfig::Direct("test-key".to_string())),
                    tools: Some(vec![
                        "bash".to_string(),
                        "spawn_subagent_batch".to_string(),
                        "wait_subagents".to_string(),
                        "list_subagents".to_string(),
                    ]),
                    prompt: Some("coordinate subagents".to_string()),
                    ..ContractAgentNode::default()
                },
            },
        )
        .await
        .expect("subagent tools should be accepted");

        assert_eq!(assessment.status, OperationAssessmentStatus::Ok);
    }

    #[tokio::test]
    async fn assess_agent_create_rejects_invalid_model_ref() {
        let (core, _env) = create_test_core_isolated().await;
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
    async fn assess_agent_update_rejects_invalid_model_ref() {
        let (core, _env) = create_test_core_isolated().await;
        let error = assess_agent_update(
            &core,
            AgentUpdateRequest {
                id: "agent-1".to_string(),
                name: None,
                agent: Some(ContractAgentNode {
                    model_ref: Some(WireModelRef {
                        provider: "anthropic".to_string(),
                        model: "gpt-5-mini".to_string(),
                    }),
                    ..ContractAgentNode::default()
                }),
            },
        )
        .await
        .expect_err("invalid model_ref should fail");

        let message = error.to_string();
        assert!(message.contains("validation_error"));
        assert!(message.contains("model_ref"));
    }

    #[tokio::test]
    async fn assess_subagent_spawn_accepts_contract_request_and_sets_effective_model_ref() {
        let (core, _env) = create_test_core_isolated().await;
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
        let (core, _env) = create_test_core_isolated().await;
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
        let (core, _env) = create_test_core_isolated().await;
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
    async fn assess_subagent_batch_allows_runtime_parent_model_inheritance() {
        let (core, _env) = create_test_core_isolated().await;
        let assessment = assess_subagent_batch(
            &core,
            "spawn_subagent_batch",
            vec![ContractRunSpawnRequest {
                task: "Return A_OK".to_string(),
                ..ContractRunSpawnRequest::default()
            }],
            false,
        )
        .await
        .expect("runtime inheritance should be allowed");

        assert_eq!(assessment.status, OperationAssessmentStatus::Ok);
        assert!(!assessment.requires_confirmation);
        assert_eq!(assessment.approval_id, None);
    }
}
