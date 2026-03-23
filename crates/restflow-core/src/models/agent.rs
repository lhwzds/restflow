//! Agent-related models
//!
//! These models define the configuration structure for AI agents.

use crate::models::{ModelId, ModelRef};
use crate::{AppCore, models::ValidationError};
use restflow_contracts::request::{
    AgentNode as ContractAgentNode, ApiKeyConfig as ContractApiKeyConfig,
    CodexCliExecutionMode as ContractCodexCliExecutionMode,
    ModelRoutingConfig as ContractModelRoutingConfig,
    SkillPreflightPolicyMode as ContractSkillPreflightPolicyMode,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::sync::Arc;
use ts_rs::TS;

/// Codex CLI execution mode.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum CodexCliExecutionMode {
    /// Safe mode: codex runs with `--full-auto`.
    Safe,
    /// Bypass mode: codex runs with
    /// `--dangerously-bypass-approvals-and-sandbox`.
    Bypass,
}

impl CodexCliExecutionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Bypass => "bypass",
        }
    }
}

/// Skill preflight policy mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Type, PartialEq, Eq, Default)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum SkillPreflightPolicyMode {
    /// Disable skill-related preflight issues.
    Off,
    /// Keep skill-related preflight issues as warnings.
    #[default]
    Warn,
    /// Promote critical skill-related warnings to blockers.
    Enforce,
}

/// Model routing configuration for automatic tier-based model selection.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ModelRoutingConfig {
    /// Enable automatic model routing.
    pub enabled: bool,
    /// Model for routine tasks (cheapest).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine_model: Option<String>,
    /// Model for moderate tasks.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderate_model: Option<String>,
    /// Model for complex tasks (most capable).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complex_model: Option<String>,
    /// Escalate to complex tier after a failed iteration.
    pub escalate_on_failure: bool,
}

impl Default for ModelRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            routine_model: None,
            moderate_model: None,
            complex_model: None,
            escalate_on_failure: true,
        }
    }
}

impl From<&ModelRoutingConfig> for restflow_ai::agent::ModelRoutingConfig {
    fn from(config: &ModelRoutingConfig) -> Self {
        Self {
            enabled: config.enabled,
            routine_model: config.routine_model.clone(),
            moderate_model: config.moderate_model.clone(),
            complex_model: config.complex_model.clone(),
            escalate_on_failure: config.escalate_on_failure,
        }
    }
}

/// API key or password configuration (direct value or secret reference)
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[ts(export)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum ApiKeyConfig {
    /// Direct password/key value
    Direct(String),
    /// Reference to secret name in secret manager
    Secret(String),
}

/// Agent configuration for AI-powered execution
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, Default)]
#[ts(export)]
pub struct AgentNode {
    /// AI model to use for this agent (None = auto-select based on auth profile)
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    /// Explicit provider + model reference (preferred over legacy `model` field).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<ModelRef>,
    /// System prompt for the agent
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Temperature setting for model responses
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Optional reasoning effort override for Codex CLI models
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_cli_reasoning_effort: Option<String>,
    /// Optional execution mode override for Codex CLI models (`safe` | `bypass`)
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_cli_execution_mode: Option<CodexCliExecutionMode>,
    /// API key configuration (direct or from secret)
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_config: Option<ApiKeyConfig>,
    /// List of tool names the agent is allowed to use
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    /// List of skill IDs to load into the system prompt
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    /// Variables available for skill prompt substitution
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_variables: Option<HashMap<String, String>>,
    /// Optional skill preflight policy mode (`off` | `warn` | `enforce`).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_preflight_policy_mode: Option<SkillPreflightPolicyMode>,
    /// Optional tier-based model routing policy.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_routing: Option<ModelRoutingConfig>,
}

fn parse_contract_model(field: &str, value: &str) -> Result<ModelId, ValidationError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(ValidationError::new(field, "must not be empty"));
    }

    ModelId::from_api_name(normalized)
        .or_else(|| ModelId::from_canonical_id(normalized))
        .or_else(|| ModelId::from_serialized_str(normalized))
        .ok_or_else(|| ValidationError::new(field, format!("unknown model '{}'", value)))
}

impl From<CodexCliExecutionMode> for ContractCodexCliExecutionMode {
    fn from(value: CodexCliExecutionMode) -> Self {
        match value {
            CodexCliExecutionMode::Safe => Self::Safe,
            CodexCliExecutionMode::Bypass => Self::Bypass,
        }
    }
}

impl From<SkillPreflightPolicyMode> for ContractSkillPreflightPolicyMode {
    fn from(value: SkillPreflightPolicyMode) -> Self {
        match value {
            SkillPreflightPolicyMode::Off => Self::Off,
            SkillPreflightPolicyMode::Warn => Self::Warn,
            SkillPreflightPolicyMode::Enforce => Self::Enforce,
        }
    }
}

impl From<ApiKeyConfig> for ContractApiKeyConfig {
    fn from(value: ApiKeyConfig) -> Self {
        match value {
            ApiKeyConfig::Direct(secret) => Self::Direct(secret),
            ApiKeyConfig::Secret(secret) => Self::Secret(secret),
        }
    }
}

impl From<ModelRoutingConfig> for ContractModelRoutingConfig {
    fn from(value: ModelRoutingConfig) -> Self {
        Self {
            enabled: value.enabled,
            routine_model: value.routine_model,
            moderate_model: value.moderate_model,
            complex_model: value.complex_model,
            escalate_on_failure: value.escalate_on_failure,
        }
    }
}

impl From<AgentNode> for ContractAgentNode {
    fn from(value: AgentNode) -> Self {
        Self {
            model: value
                .model
                .map(|model| model.as_serialized_str().to_string()),
            model_ref: value.model_ref.map(Into::into),
            prompt: value.prompt,
            temperature: value.temperature,
            codex_cli_reasoning_effort: value.codex_cli_reasoning_effort,
            codex_cli_execution_mode: value.codex_cli_execution_mode.map(Into::into),
            api_key_config: value.api_key_config.map(Into::into),
            tools: value.tools,
            skills: value.skills,
            skill_variables: value.skill_variables,
            skill_preflight_policy_mode: value.skill_preflight_policy_mode.map(Into::into),
            model_routing: value.model_routing.map(Into::into),
        }
    }
}

impl TryFrom<ContractAgentNode> for AgentNode {
    type Error = Vec<ValidationError>;

    fn try_from(value: ContractAgentNode) -> Result<Self, Self::Error> {
        let mut errors = Vec::new();

        let model = match value.model {
            Some(model) => match parse_contract_model("model", &model) {
                Ok(model) => Some(model),
                Err(error) => {
                    errors.push(error);
                    None
                }
            },
            None => None,
        };

        let model_ref = match value.model_ref {
            Some(model_ref) => match ModelRef::try_from(model_ref) {
                Ok(model_ref) => Some(model_ref),
                Err(error) => {
                    errors.push(error);
                    None
                }
            },
            None => None,
        };

        let mut agent = Self {
            model,
            model_ref,
            prompt: value.prompt,
            temperature: value.temperature,
            codex_cli_reasoning_effort: value.codex_cli_reasoning_effort,
            codex_cli_execution_mode: match value.codex_cli_execution_mode {
                Some(ContractCodexCliExecutionMode::Safe) => Some(CodexCliExecutionMode::Safe),
                Some(ContractCodexCliExecutionMode::Bypass) => Some(CodexCliExecutionMode::Bypass),
                Some(ContractCodexCliExecutionMode::Unknown) | None => None,
            },
            api_key_config: value.api_key_config.map(|config| match config {
                ContractApiKeyConfig::Direct(secret) => ApiKeyConfig::Direct(secret),
                ContractApiKeyConfig::Secret(secret) => ApiKeyConfig::Secret(secret),
            }),
            tools: value.tools,
            skills: value.skills,
            skill_variables: value.skill_variables,
            skill_preflight_policy_mode: value.skill_preflight_policy_mode.map(|mode| match mode {
                ContractSkillPreflightPolicyMode::Off => SkillPreflightPolicyMode::Off,
                ContractSkillPreflightPolicyMode::Warn => SkillPreflightPolicyMode::Warn,
                ContractSkillPreflightPolicyMode::Enforce => SkillPreflightPolicyMode::Enforce,
            }),
            model_routing: value.model_routing.map(|routing| ModelRoutingConfig {
                enabled: routing.enabled,
                routine_model: routing.routine_model,
                moderate_model: routing.moderate_model,
                complex_model: routing.complex_model,
                escalate_on_failure: routing.escalate_on_failure,
            }),
        };

        if errors.is_empty()
            && let Err(error) = agent.normalize_model_fields()
        {
            errors.push(error);
        }

        if errors.is_empty() {
            Ok(agent)
        } else {
            Err(errors)
        }
    }
}

impl AgentNode {
    /// Create a new agent with default settings (no model specified)
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode a contract agent payload into the canonical core model.
    pub fn try_from_contract_node(value: ContractAgentNode) -> Result<Self, Vec<ValidationError>> {
        Self::try_from(value)
    }

    /// Create a new agent with a specific model
    pub fn with_model(model: ModelId) -> Self {
        Self {
            model: Some(model),
            model_ref: Some(ModelRef::from_model(model)),
            ..Default::default()
        }
    }

    /// Create a new agent with an explicit provider + model reference.
    pub fn with_model_ref(model_ref: ModelRef) -> Self {
        Self {
            model: Some(model_ref.model),
            model_ref: Some(model_ref),
            ..Default::default()
        }
    }

    /// Set the system prompt
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the reasoning effort for Codex CLI models
    pub fn with_codex_cli_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        let effort = effort.into();
        let normalized = effort.trim();
        if !normalized.is_empty() {
            self.codex_cli_reasoning_effort = Some(normalized.to_string());
        }
        self
    }

    /// Set the execution mode for Codex CLI models
    pub fn with_codex_cli_execution_mode(mut self, mode: CodexCliExecutionMode) -> Self {
        self.codex_cli_execution_mode = Some(mode);
        self
    }

    /// Set the API key configuration
    pub fn with_api_key(mut self, config: ApiKeyConfig) -> Self {
        self.api_key_config = Some(config);
        self
    }

    /// Set the allowed tools
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the skill IDs to load
    pub fn with_skills(mut self, skills: Vec<String>) -> Self {
        self.skills = Some(skills);
        self
    }

    /// Set skill variables for prompt substitution
    pub fn with_skill_variables(mut self, variables: HashMap<String, String>) -> Self {
        self.skill_variables = Some(variables);
        self
    }

    /// Set skill preflight policy mode.
    pub fn with_skill_preflight_policy_mode(mut self, mode: SkillPreflightPolicyMode) -> Self {
        self.skill_preflight_policy_mode = Some(mode);
        self
    }

    /// Get effective skill preflight policy mode, defaulting to `warn`.
    pub fn effective_skill_preflight_policy_mode(&self) -> SkillPreflightPolicyMode {
        self.skill_preflight_policy_mode.unwrap_or_default()
    }

    /// Set model routing policy.
    pub fn with_model_routing(mut self, routing: ModelRoutingConfig) -> Self {
        self.model_routing = Some(routing);
        self
    }

    /// Resolve effective provider + model, preferring `model_ref`.
    pub fn resolved_model_ref(&self) -> Option<ModelRef> {
        self.model_ref
            .or_else(|| self.model.map(ModelRef::from_model))
    }

    /// Normalize legacy/new model fields into a consistent representation.
    pub fn normalize_model_fields(&mut self) -> Result<(), ValidationError> {
        if let Some(model_ref) = self.model_ref {
            model_ref.validate()?;
            if let Some(model) = self.model
                && model != model_ref.model
            {
                return Err(ValidationError::new(
                    "model_ref",
                    "model_ref.model must match legacy model field when both are set",
                ));
            }
            self.model = Some(model_ref.model);
            return Ok(());
        }

        if let Some(model) = self.model {
            self.model_ref = Some(ModelRef::from_model(model));
        }

        Ok(())
    }

    /// Get the model, returning an error if not specified
    pub fn require_model(&self) -> Result<ModelId, &'static str> {
        self.resolved_model_ref()
            .map(|model_ref| model_ref.model)
            .ok_or("Model not specified. Please set a model for this agent.")
    }

    /// Get the model or use a fallback default
    pub fn get_model_or(&self, default: ModelId) -> ModelId {
        self.resolved_model_ref()
            .map(|model_ref| model_ref.model)
            .unwrap_or(default)
    }

    /// Validate fields that do not depend on storage or runtime state.
    ///
    /// Cross-field validation notes:
    /// - Temperature: validated against model's `supports_temperature()` when model is set.
    ///   Range 0.0..=2.0 is the outer bound across all providers (OpenAI uses 0.0-2.0,
    ///   Anthropic 0.0-1.0, others vary). We use the widest range here to avoid
    ///   false rejections; providers clamp or ignore out-of-range values.
    /// - codex_cli_reasoning_effort / codex_cli_execution_mode: only meaningful for
    ///   Codex CLI models. Setting them on other models is rejected since the values
    ///   would be silently ignored at runtime.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        let resolved_model_ref = self.resolved_model_ref();
        if let (Some(model), Some(model_ref)) = (self.model, self.model_ref)
            && model != model_ref.model
        {
            errors.push(ValidationError::new(
                "model_ref",
                "model_ref.model must match legacy model field when both are set",
            ));
        }
        if let Some(model_ref) = resolved_model_ref
            && let Err(error) = model_ref.validate()
        {
            errors.push(error);
        }
        let selected_model = resolved_model_ref.map(|model_ref| model_ref.model);

        // Temperature: reject if model explicitly doesn't support it (GPT-5, Codex CLI, etc.)
        if let Some(temperature) = self.temperature {
            if let Some(model) = selected_model
                && !model.supports_temperature()
            {
                errors.push(ValidationError::new(
                    "temperature",
                    format!(
                        "model {} does not support temperature parameter",
                        model.metadata().name
                    ),
                ));
            }
            // Outer bound across all providers (OpenAI 0-2, Anthropic 0-1, etc.)
            if !(0.0..=2.0).contains(&temperature) {
                errors.push(ValidationError::new(
                    "temperature",
                    "must be between 0.0 and 2.0",
                ));
            }
        }

        // codex_cli_reasoning_effort: only applies to Codex CLI models
        if let Some(effort) = &self.codex_cli_reasoning_effort {
            if let Some(model) = selected_model
                && !model.is_codex_cli()
            {
                errors.push(ValidationError::new(
                    "codex_cli_reasoning_effort",
                    format!(
                        "only applies to Codex CLI models, not {}",
                        model.metadata().name
                    ),
                ));
            }
            let normalized = effort.trim().to_lowercase();
            if !matches!(normalized.as_str(), "low" | "medium" | "high" | "xhigh") {
                errors.push(ValidationError::new(
                    "codex_cli_reasoning_effort",
                    "must be one of: low, medium, high, xhigh",
                ));
            }
        }

        // codex_cli_execution_mode: only applies to Codex CLI models
        if self.codex_cli_execution_mode.is_some()
            && let Some(model) = selected_model
            && !model.is_codex_cli()
        {
            errors.push(ValidationError::new(
                "codex_cli_execution_mode",
                format!(
                    "only applies to Codex CLI models, not {}",
                    model.metadata().name
                ),
            ));
        }

        if let Some(routing) = &self.model_routing {
            for (field, model) in [
                (
                    "model_routing.routine_model",
                    routing.routine_model.as_deref(),
                ),
                (
                    "model_routing.moderate_model",
                    routing.moderate_model.as_deref(),
                ),
                (
                    "model_routing.complex_model",
                    routing.complex_model.as_deref(),
                ),
            ] {
                if let Some(model) = model {
                    let normalized = model.trim();
                    if normalized.is_empty() {
                        errors.push(ValidationError::new(field, "must not be empty"));
                    } else if ModelId::from_api_name(normalized).is_none() {
                        errors.push(ValidationError::new(
                            field,
                            format!("unsupported model '{}'", normalized),
                        ));
                    }
                }
            }
        }

        if let Some(prompt) = &self.prompt
            && prompt.trim().is_empty()
        {
            errors.push(ValidationError::new(
                "prompt",
                "must not be empty or whitespace-only",
            ));
        }

        if let Some(api_key_config) = &self.api_key_config {
            match api_key_config {
                ApiKeyConfig::Direct(value) => {
                    if value.trim().is_empty() {
                        errors.push(ValidationError::new(
                            "api_key_config",
                            "direct key must not be empty",
                        ));
                    }
                }
                ApiKeyConfig::Secret(secret_name) => {
                    if secret_name.trim().is_empty() {
                        errors.push(ValidationError::new(
                            "api_key_config",
                            "secret reference must not be empty",
                        ));
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate fields that require runtime/storage lookups.
    pub async fn validate_async(&self, core: &Arc<AppCore>) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        let tool_registry = match crate::services::tool_registry::create_tool_registry(
            core.storage.skills.clone(),
            core.storage.memory.clone(),
            core.storage.chat_sessions.clone(),
            core.storage.channel_session_bindings.clone(),
            core.storage.execution_traces.clone(),
            core.storage.kv_store.clone(),
            core.storage.work_items.clone(),
            core.storage.secrets.clone(),
            core.storage.config.clone(),
            core.storage.agents.clone(),
            core.storage.background_agents.clone(),
            core.storage.triggers.clone(),
            core.storage.terminal_sessions.clone(),
            core.storage.deliverables.clone(),
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

        if let Some(tools) = &self.tools {
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

        if let Some(skills) = &self.skills {
            for skill_id in skills {
                let normalized = skill_id.trim();
                if normalized.is_empty() {
                    errors.push(ValidationError::new("skills", "skill ID must not be empty"));
                    continue;
                }
                match core.storage.skills.exists(normalized) {
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

        if let Some(ApiKeyConfig::Secret(secret_name)) = &self.api_key_config {
            let normalized = secret_name.trim();
            if !normalized.is_empty() {
                match core.storage.secrets.has_available_secret(normalized) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Provider;
    use restflow_contracts::request::{AgentNode as ContractAgentNode, WireModelRef};

    #[test]
    fn with_codex_cli_reasoning_effort_sets_trimmed_value() {
        let node = AgentNode::new().with_codex_cli_reasoning_effort("  xhigh  ");
        assert_eq!(node.codex_cli_reasoning_effort.as_deref(), Some("xhigh"));
    }

    #[test]
    fn with_codex_cli_reasoning_effort_ignores_empty_input() {
        let node = AgentNode::new().with_codex_cli_reasoning_effort("   ");
        assert!(node.codex_cli_reasoning_effort.is_none());
    }

    #[test]
    fn codex_cli_execution_mode_serializes_to_snake_case() {
        let safe = serde_json::to_string(&CodexCliExecutionMode::Safe).unwrap();
        let bypass = serde_json::to_string(&CodexCliExecutionMode::Bypass).unwrap();
        assert_eq!(safe, "\"safe\"");
        assert_eq!(bypass, "\"bypass\"");
    }

    #[test]
    fn with_codex_cli_execution_mode_sets_value() {
        let node = AgentNode::new().with_codex_cli_execution_mode(CodexCliExecutionMode::Bypass);
        assert_eq!(
            node.codex_cli_execution_mode,
            Some(CodexCliExecutionMode::Bypass)
        );
    }

    #[test]
    fn skill_preflight_policy_mode_serializes_to_snake_case() {
        let off = serde_json::to_string(&SkillPreflightPolicyMode::Off).unwrap();
        let warn = serde_json::to_string(&SkillPreflightPolicyMode::Warn).unwrap();
        let enforce = serde_json::to_string(&SkillPreflightPolicyMode::Enforce).unwrap();
        assert_eq!(off, "\"off\"");
        assert_eq!(warn, "\"warn\"");
        assert_eq!(enforce, "\"enforce\"");
    }

    #[test]
    fn effective_skill_preflight_policy_mode_defaults_to_warn() {
        let node: AgentNode = serde_json::from_str(r#"{"prompt":"hello"}"#).unwrap();
        assert!(node.skill_preflight_policy_mode.is_none());
        assert_eq!(
            node.effective_skill_preflight_policy_mode(),
            SkillPreflightPolicyMode::Warn
        );
    }

    #[test]
    fn with_skill_preflight_policy_mode_sets_value() {
        let node = AgentNode::new().with_skill_preflight_policy_mode(SkillPreflightPolicyMode::Off);
        assert_eq!(
            node.skill_preflight_policy_mode,
            Some(SkillPreflightPolicyMode::Off)
        );
    }

    #[test]
    fn validate_accepts_valid_codex_config() {
        let node = AgentNode {
            model: Some(ModelId::CodexCli),
            ..AgentNode::new()
                .with_prompt("You are helpful")
                .with_codex_cli_reasoning_effort("HIGH")
                .with_api_key(ApiKeyConfig::Direct("test-key".to_string()))
        };
        assert!(node.validate().is_ok());
    }

    #[test]
    fn validate_accepts_temperature_on_supported_model() {
        let node = AgentNode {
            model: Some(ModelId::ClaudeSonnet4_5),
            ..AgentNode::new().with_temperature(0.7)
        };
        assert!(node.validate().is_ok());
    }

    #[test]
    fn validate_accepts_temperature_without_model() {
        // When model is None (auto-select), allow temperature since we cannot
        // know which model will be resolved at runtime.
        let node = AgentNode::new().with_temperature(1.0);
        assert!(node.validate().is_ok());
    }

    #[test]
    fn validate_rejects_temperature_on_unsupported_model() {
        let node = AgentNode {
            model: Some(ModelId::Gpt5),
            ..AgentNode::new().with_temperature(0.5)
        };
        let errors = node.validate().expect_err("expected validation error");
        assert!(
            errors
                .iter()
                .any(|e| e.field == "temperature" && e.message.contains("does not support"))
        );
    }

    #[test]
    fn validate_rejects_out_of_range_temperature() {
        let node = AgentNode::new().with_temperature(2.1);
        let errors = node.validate().expect_err("expected validation error");
        assert!(
            errors
                .iter()
                .any(|error| error.field == "temperature" && error.message.contains("0.0 and 2.0"))
        );
    }

    #[test]
    fn validate_rejects_empty_prompt() {
        let node = AgentNode::new().with_prompt("   ");
        let errors = node.validate().expect_err("expected validation error");
        assert!(
            errors
                .iter()
                .any(|error| error.field == "prompt" && error.message.contains("must not be empty"))
        );
    }

    #[test]
    fn validate_rejects_invalid_reasoning_effort() {
        let node = AgentNode {
            model: Some(ModelId::CodexCli),
            ..AgentNode::new().with_codex_cli_reasoning_effort("ultra")
        };
        let errors = node.validate().expect_err("expected validation error");
        assert!(
            errors
                .iter()
                .any(|error| error.field == "codex_cli_reasoning_effort")
        );
    }

    #[test]
    fn validate_rejects_reasoning_effort_on_non_codex_model() {
        let node = AgentNode {
            model: Some(ModelId::ClaudeSonnet4_5),
            ..AgentNode::new().with_codex_cli_reasoning_effort("high")
        };
        let errors = node.validate().expect_err("expected validation error");
        assert!(
            errors
                .iter()
                .any(|e| e.field == "codex_cli_reasoning_effort"
                    && e.message.contains("only applies to Codex CLI"))
        );
    }

    #[test]
    fn validate_rejects_execution_mode_on_non_codex_model() {
        let node = AgentNode {
            model: Some(ModelId::DeepseekChat),
            ..AgentNode::new().with_codex_cli_execution_mode(CodexCliExecutionMode::Bypass)
        };
        let errors = node.validate().expect_err("expected validation error");
        assert!(errors.iter().any(|e| e.field == "codex_cli_execution_mode"
            && e.message.contains("only applies to Codex CLI")));
    }

    #[test]
    fn validate_accepts_execution_mode_on_codex_model() {
        let node = AgentNode {
            model: Some(ModelId::Gpt5Codex),
            ..AgentNode::new().with_codex_cli_execution_mode(CodexCliExecutionMode::Safe)
        };
        assert!(node.validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_direct_api_key() {
        let node = AgentNode::new().with_api_key(ApiKeyConfig::Direct("  ".to_string()));
        let errors = node.validate().expect_err("expected validation error");
        assert!(errors.iter().any(|error| error.field == "api_key_config"));
    }

    #[test]
    fn validate_accepts_model_routing_with_known_models() {
        let node = AgentNode::new().with_model_routing(ModelRoutingConfig {
            enabled: true,
            routine_model: Some("gpt-5-nano".to_string()),
            moderate_model: Some("claude-sonnet-4-5".to_string()),
            complex_model: Some("claude-opus-4-6".to_string()),
            escalate_on_failure: true,
        });

        assert!(node.validate().is_ok());
    }

    #[test]
    fn validate_rejects_model_routing_with_unknown_model() {
        let node = AgentNode::new().with_model_routing(ModelRoutingConfig {
            enabled: true,
            routine_model: Some("not-a-real-model".to_string()),
            moderate_model: None,
            complex_model: None,
            escalate_on_failure: true,
        });

        let errors = node.validate().expect_err("expected validation error");
        assert!(
            errors
                .iter()
                .any(|error| error.field == "model_routing.routine_model")
        );
    }

    #[test]
    fn normalize_model_fields_backfills_model_ref_from_legacy_model() {
        let mut node = AgentNode {
            model: Some(ModelId::Gpt5),
            model_ref: None,
            ..AgentNode::new()
        };
        node.normalize_model_fields()
            .expect("normalization should pass");
        let model_ref = node.model_ref.expect("model_ref should be backfilled");
        assert_eq!(model_ref.provider, Provider::OpenAI);
        assert_eq!(model_ref.model, ModelId::Gpt5);
    }

    #[test]
    fn validate_rejects_mismatched_model_and_model_ref() {
        let node = AgentNode {
            model: Some(ModelId::Gpt5),
            model_ref: Some(ModelRef {
                provider: Provider::Anthropic,
                model: ModelId::ClaudeSonnet4_5,
            }),
            ..AgentNode::new()
        };
        let errors = node.validate().expect_err("expected validation error");
        assert!(errors.iter().any(|error| error.field == "model_ref"));
    }

    #[test]
    fn contract_agent_node_round_trips_through_explicit_conversion() {
        let agent = AgentNode::with_model_ref(ModelRef {
            provider: Provider::Anthropic,
            model: ModelId::ClaudeCodeSonnet,
        })
        .with_prompt("Base prompt")
        .with_codex_cli_execution_mode(CodexCliExecutionMode::Safe);

        let contract: ContractAgentNode = agent.clone().into();
        let decoded = AgentNode::try_from(contract).expect("agent should decode");

        assert_eq!(decoded.model, Some(ModelId::ClaudeCodeSonnet));
        assert_eq!(
            decoded.model_ref,
            Some(ModelRef {
                provider: Provider::ClaudeCode,
                model: ModelId::ClaudeCodeSonnet,
            })
        );
        assert_eq!(decoded.prompt.as_deref(), Some("Base prompt"));
    }

    #[test]
    fn contract_agent_node_rejects_unknown_model() {
        let errors = AgentNode::try_from(ContractAgentNode {
            model: Some("missing-model".to_string()),
            ..ContractAgentNode::default()
        })
        .expect_err("unknown model should fail");

        assert!(errors.iter().any(|error| error.field == "model"));
    }

    #[test]
    fn contract_agent_node_rejects_conflicting_model_and_model_ref() {
        let errors = AgentNode::try_from(ContractAgentNode {
            model: Some("gpt-5".to_string()),
            model_ref: Some(WireModelRef {
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4-5".to_string(),
            }),
            ..ContractAgentNode::default()
        })
        .expect_err("conflicting model fields should fail");

        assert!(errors.iter().any(|error| error.field == "model_ref"));
    }
}
