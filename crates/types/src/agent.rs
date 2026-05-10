//! Shared agent configuration types.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

use crate::error::ValidationError;
use crate::model::ModelRef;
use crate::model_id::ModelId;
use crate::request;

/// Codex CLI execution mode.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct ModelRoutingConfig {
    /// Enable automatic model routing.
    pub enabled: bool,
    /// Model for routine tasks (cheapest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine_model: Option<String>,
    /// Model for moderate tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderate_model: Option<String>,
    /// Model for complex tasks (most capable).
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

/// API key or password configuration (direct value or secret reference).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum ApiKeyConfig {
    /// Direct password/key value.
    Direct(String),
    /// Reference to secret name in secret manager.
    Secret(String),
}

/// Agent configuration for AI-powered execution.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
pub struct AgentNode {
    /// Explicit provider + model reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<ModelRef>,
    /// System prompt for the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Temperature setting for model responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Optional reasoning effort override for Codex CLI models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_cli_reasoning_effort: Option<String>,
    /// Optional execution mode override for Codex CLI models (`safe` | `bypass`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_cli_execution_mode: Option<CodexCliExecutionMode>,
    /// API key configuration (direct or from secret).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_config: Option<ApiKeyConfig>,
    /// List of tool names the agent is allowed to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    /// List of skill IDs to load into the system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    /// Variables available for skill prompt substitution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_variables: Option<HashMap<String, String>>,
    /// Optional skill preflight policy mode (`off` | `warn` | `enforce`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_preflight_policy_mode: Option<SkillPreflightPolicyMode>,
    /// Optional tier-based model routing policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_routing: Option<ModelRoutingConfig>,
}

impl AgentNode {
    /// Create a new agent with default settings (no model specified).
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode a contract agent payload into the canonical core model.
    pub fn try_from_contract_node(value: request::AgentNode) -> Result<Self, Vec<ValidationError>> {
        Self::try_from(value)
    }

    /// Create a new agent with a specific model.
    pub fn with_model(model: ModelId) -> Self {
        Self {
            model_ref: Some(ModelRef::from_model(model)),
            ..Default::default()
        }
    }

    /// Create a new agent with an explicit provider + model reference.
    pub fn with_model_ref(model_ref: ModelRef) -> Self {
        Self {
            model_ref: Some(model_ref),
            ..Default::default()
        }
    }

    /// Set the system prompt.
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Set the temperature.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the reasoning effort for Codex CLI models.
    pub fn with_codex_cli_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        let effort = effort.into();
        let normalized = effort.trim();
        if !normalized.is_empty() {
            self.codex_cli_reasoning_effort = Some(normalized.to_string());
        }
        self
    }

    /// Set the execution mode for Codex CLI models.
    pub fn with_codex_cli_execution_mode(mut self, mode: CodexCliExecutionMode) -> Self {
        self.codex_cli_execution_mode = Some(mode);
        self
    }

    /// Set the API key configuration.
    pub fn with_api_key(mut self, config: ApiKeyConfig) -> Self {
        self.api_key_config = Some(config);
        self
    }

    /// Set the allowed tools.
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the skill IDs to load.
    pub fn with_skills(mut self, skills: Vec<String>) -> Self {
        self.skills = Some(skills);
        self
    }

    /// Set skill variables for prompt substitution.
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
    }

    /// Validate model fields before persistence or execution.
    pub fn normalize_model_fields(&mut self) -> Result<(), ValidationError> {
        if let Some(model_ref) = self.model_ref {
            model_ref.validate()?;
        }
        Ok(())
    }

    /// Get the model, returning an error if not specified.
    pub fn require_model(&self) -> Result<ModelId, &'static str> {
        self.resolved_model_ref()
            .map(|model_ref| model_ref.model)
            .ok_or("Model not specified. Please set a model for this agent.")
    }

    /// Get the model or use a fallback default.
    pub fn get_model_or(&self, default: ModelId) -> ModelId {
        self.resolved_model_ref()
            .map(|model_ref| model_ref.model)
            .unwrap_or(default)
    }

    /// Validate fields that do not depend on storage or runtime state.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        let resolved_model_ref = self.resolved_model_ref();
        if let Some(model_ref) = resolved_model_ref
            && let Err(error) = model_ref.validate()
        {
            errors.push(error);
        }
        let selected_model = resolved_model_ref.map(|model_ref| model_ref.model);

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
            if !(0.0..=2.0).contains(&temperature) {
                errors.push(ValidationError::new(
                    "temperature",
                    "must be between 0.0 and 2.0",
                ));
            }
        }

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
}

/// Agent metadata stored separately from prompt content.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentMeta {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_config: Option<ApiKeyConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_variables: Option<HashMap<String, String>>,
    pub agent_type: AgentType,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum AgentType {
    Main,
    Cron,
    Sub,
    Inline,
}

impl From<CodexCliExecutionMode> for request::CodexCliExecutionMode {
    fn from(value: CodexCliExecutionMode) -> Self {
        match value {
            CodexCliExecutionMode::Safe => Self::Safe,
            CodexCliExecutionMode::Bypass => Self::Bypass,
        }
    }
}

impl From<SkillPreflightPolicyMode> for request::SkillPreflightPolicyMode {
    fn from(value: SkillPreflightPolicyMode) -> Self {
        match value {
            SkillPreflightPolicyMode::Off => Self::Off,
            SkillPreflightPolicyMode::Warn => Self::Warn,
            SkillPreflightPolicyMode::Enforce => Self::Enforce,
        }
    }
}

impl From<ApiKeyConfig> for request::ApiKeyConfig {
    fn from(value: ApiKeyConfig) -> Self {
        match value {
            ApiKeyConfig::Direct(secret) => Self::Direct(secret),
            ApiKeyConfig::Secret(secret) => Self::Secret(secret),
        }
    }
}

impl From<ModelRoutingConfig> for request::ModelRoutingConfig {
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

impl From<AgentNode> for request::AgentNode {
    fn from(value: AgentNode) -> Self {
        request::AgentNode {
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

impl TryFrom<request::AgentNode> for AgentNode {
    type Error = Vec<ValidationError>;

    fn try_from(value: request::AgentNode) -> Result<Self, Self::Error> {
        let mut errors = Vec::new();

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

        let mut agent = AgentNode {
            model_ref,
            prompt: value.prompt,
            temperature: value.temperature,
            codex_cli_reasoning_effort: value.codex_cli_reasoning_effort,
            codex_cli_execution_mode: match value.codex_cli_execution_mode {
                Some(request::CodexCliExecutionMode::Safe) => Some(CodexCliExecutionMode::Safe),
                Some(request::CodexCliExecutionMode::Bypass) => Some(CodexCliExecutionMode::Bypass),
                Some(request::CodexCliExecutionMode::Unknown) | None => None,
            },
            api_key_config: value.api_key_config.map(|config| match config {
                request::ApiKeyConfig::Direct(secret) => ApiKeyConfig::Direct(secret),
                request::ApiKeyConfig::Secret(secret) => ApiKeyConfig::Secret(secret),
            }),
            tools: value.tools,
            skills: value.skills,
            skill_variables: value.skill_variables,
            skill_preflight_policy_mode: value.skill_preflight_policy_mode.map(|mode| match mode {
                request::SkillPreflightPolicyMode::Off => SkillPreflightPolicyMode::Off,
                request::SkillPreflightPolicyMode::Warn => SkillPreflightPolicyMode::Warn,
                request::SkillPreflightPolicyMode::Enforce => SkillPreflightPolicyMode::Enforce,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Provider;

    #[test]
    fn codex_cli_execution_mode_serializes_to_snake_case() {
        let safe = serde_json::to_string(&CodexCliExecutionMode::Safe).unwrap();
        let bypass = serde_json::to_string(&CodexCliExecutionMode::Bypass).unwrap();
        assert_eq!(safe, "\"safe\"");
        assert_eq!(bypass, "\"bypass\"");
    }

    #[test]
    fn validate_rejects_temperature_on_unsupported_model() {
        let node = AgentNode {
            ..AgentNode::with_model(ModelId::Gpt5).with_temperature(0.5)
        };
        let errors = node.validate().expect_err("expected validation error");
        assert!(errors.iter().any(
            |error| error.field == "temperature" && error.message.contains("does not support")
        ));
    }

    #[test]
    fn validate_rejects_invalid_reasoning_effort() {
        let node = AgentNode {
            ..AgentNode::with_model(ModelId::CodexCli).with_codex_cli_reasoning_effort("ultra")
        };
        let errors = node.validate().expect_err("expected validation error");
        assert!(
            errors
                .iter()
                .any(|error| error.field == "codex_cli_reasoning_effort")
        );
    }

    #[test]
    fn contract_agent_node_round_trips_through_explicit_conversion() {
        let agent = AgentNode::with_model_ref(ModelRef {
            provider: Provider::Codex,
            model: ModelId::Gpt5_4Codex,
        })
        .with_prompt("Base prompt")
        .with_codex_cli_execution_mode(CodexCliExecutionMode::Safe);

        let contract: request::AgentNode = agent.clone().into();
        let decoded = AgentNode::try_from(contract).expect("agent should decode");

        assert_eq!(
            decoded.model_ref,
            Some(ModelRef {
                provider: Provider::Codex,
                model: ModelId::Gpt5_4Codex,
            })
        );
        assert_eq!(decoded.prompt.as_deref(), Some("Base prompt"));
    }
}
