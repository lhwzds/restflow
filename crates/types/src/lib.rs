//! Shared RestFlow types, contracts, traits, and model catalog.
//!
//! This crate provides the shared interfaces used across the RestFlow workspace:
//! - Tool trait, ToolError, ToolRegistry, Toolset
//! - SecurityGate, SecurityDecision, ToolAction
//! - SkillProvider and skill data types
//! - store traits (AgentStore, SessionStore, SecretStore, etc.)
//! - Sub-agent data types and lookup traits
//! - Provider and model catalog types

pub mod agent {
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
        pub fn try_from_contract_node(
            value: request::AgentNode,
        ) -> Result<Self, Vec<ValidationError>> {
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
                    Some(request::CodexCliExecutionMode::Bypass) => {
                        Some(CodexCliExecutionMode::Bypass)
                    }
                    Some(request::CodexCliExecutionMode::Unknown) | None => None,
                },
                api_key_config: value.api_key_config.map(|config| match config {
                    request::ApiKeyConfig::Direct(secret) => ApiKeyConfig::Direct(secret),
                    request::ApiKeyConfig::Secret(secret) => ApiKeyConfig::Secret(secret),
                }),
                tools: value.tools,
                skills: value.skills,
                skill_variables: value.skill_variables,
                skill_preflight_policy_mode: value.skill_preflight_policy_mode.map(
                    |mode| match mode {
                        request::SkillPreflightPolicyMode::Off => SkillPreflightPolicyMode::Off,
                        request::SkillPreflightPolicyMode::Warn => SkillPreflightPolicyMode::Warn,
                        request::SkillPreflightPolicyMode::Enforce => {
                            SkillPreflightPolicyMode::Enforce
                        }
                    },
                ),
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
            assert!(
                errors.iter().any(|error| error.field == "temperature"
                    && error.message.contains("does not support"))
            );
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
}

pub mod catalog {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    use crate::{ClientKind, ModelId, ModelMetadata, ModelMetadataDTO, Provider};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ModelDescriptor {
        pub id: ModelId,
        pub provider: Provider,
        pub api_name: &'static str,
        pub display_name: &'static str,
        pub supports_temperature: bool,
        pub aliases: &'static [&'static str],
        pub prefix_aliases: &'static [&'static str],
        pub canonical_family: Option<&'static str>,
        pub same_provider_fallback: Option<ModelId>,
        pub openrouter_equivalent: Option<ModelId>,
        pub client_kind: ClientKind,
        pub base_url_override: Option<&'static str>,
    }

    impl ModelDescriptor {
        pub const fn new(
            id: ModelId,
            provider: Provider,
            api_name: &'static str,
            display_name: &'static str,
            supports_temperature: bool,
        ) -> Self {
            Self {
                id,
                provider,
                api_name,
                display_name,
                supports_temperature,
                aliases: &[],
                prefix_aliases: &[],
                canonical_family: None,
                same_provider_fallback: None,
                openrouter_equivalent: None,
                client_kind: ClientKind::Http,
                base_url_override: None,
            }
        }

        pub const fn with_aliases(mut self, aliases: &'static [&'static str]) -> Self {
            self.aliases = aliases;
            self
        }

        pub const fn with_prefix_aliases(mut self, aliases: &'static [&'static str]) -> Self {
            self.prefix_aliases = aliases;
            self
        }

        pub const fn with_canonical_family(mut self, family: &'static str) -> Self {
            self.canonical_family = Some(family);
            self
        }

        pub const fn with_same_provider_fallback(mut self, model: ModelId) -> Self {
            self.same_provider_fallback = Some(model);
            self
        }

        pub const fn with_openrouter_equivalent(mut self, model: ModelId) -> Self {
            self.openrouter_equivalent = Some(model);
            self
        }

        pub const fn with_client_kind(mut self, client_kind: ClientKind) -> Self {
            self.client_kind = client_kind;
            self
        }

        pub const fn with_base_url_override(mut self, base_url: &'static str) -> Self {
            self.base_url_override = Some(base_url);
            self
        }

        pub const fn metadata(&self) -> ModelMetadata {
            ModelMetadata {
                provider: self.provider,
                supports_temperature: self.supports_temperature,
                name: self.display_name,
            }
        }

        pub fn metadata_dto(&self) -> ModelMetadataDTO {
            ModelMetadataDTO {
                model: self.id,
                provider: self.provider,
                supports_temperature: self.supports_temperature,
                name: self.display_name.to_string(),
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProviderCatalog {
        pub provider: Provider,
        pub flagship: ModelId,
        pub models: &'static [ModelDescriptor],
    }

    impl ProviderCatalog {
        pub const fn new(
            provider: Provider,
            flagship: ModelId,
            models: &'static [ModelDescriptor],
        ) -> Self {
            Self {
                provider,
                flagship,
                models,
            }
        }
    }

    pub const PROVIDER_CATALOGS: &[ProviderCatalog] = &[
        openai::CATALOG,
        anthropic::CATALOG,
        codex::CATALOG,
        deepseek::CATALOG,
        google::CATALOG,
        groq::CATALOG,
        openrouter::CATALOG,
        xai::CATALOG,
        qwen::CATALOG,
        zai::CATALOG,
        zai_coding_plan::CATALOG,
        moonshot::CATALOG,
        doubao::CATALOG,
        yi::CATALOG,
        siliconflow::CATALOG,
        minimax::CATALOG,
        minimax_coding_plan::CATALOG,
    ];

    static DESCRIPTOR_BY_ID: OnceLock<HashMap<&'static str, &'static ModelDescriptor>> =
        OnceLock::new();
    static MODEL_IDS: OnceLock<Vec<ModelId>> = OnceLock::new();
    static NAME_LOOKUP: OnceLock<HashMap<String, ModelId>> = OnceLock::new();

    pub fn provider_catalog(provider: Provider) -> Option<&'static ProviderCatalog> {
        PROVIDER_CATALOGS
            .iter()
            .find(|catalog| catalog.provider == provider)
    }

    pub fn all_descriptors() -> impl Iterator<Item = &'static ModelDescriptor> {
        PROVIDER_CATALOGS
            .iter()
            .flat_map(|catalog| catalog.models.iter())
    }

    pub fn all_model_ids() -> &'static [ModelId] {
        MODEL_IDS
            .get_or_init(|| all_descriptors().map(|descriptor| descriptor.id).collect())
            .as_slice()
    }

    pub fn descriptor(model: ModelId) -> Option<&'static ModelDescriptor> {
        DESCRIPTOR_BY_ID
            .get_or_init(|| {
                all_descriptors()
                    .map(|descriptor| (descriptor.id.as_serialized_str(), descriptor))
                    .collect()
            })
            .get(model.as_serialized_str())
            .copied()
    }

    pub fn lookup_by_name(name: &str) -> Option<ModelId> {
        let key = normalize_lookup_key(name)?;
        if let Some(model) = NAME_LOOKUP
            .get_or_init(|| {
                let mut lookup = HashMap::new();
                for descriptor in all_descriptors() {
                    register_lookup_key(
                        &mut lookup,
                        descriptor.id.as_serialized_str(),
                        descriptor.id,
                    );
                }
                for descriptor in all_descriptors() {
                    register_lookup_key(&mut lookup, descriptor.api_name, descriptor.id);
                    for alias in descriptor.aliases {
                        register_lookup_key(&mut lookup, alias, descriptor.id);
                    }
                }
                lookup
            })
            .get(&key)
            .copied()
        {
            return Some(model);
        }

        all_descriptors().find_map(|descriptor| {
            descriptor_matches_prefix_alias(descriptor, &key).then_some(descriptor.id)
        })
    }

    pub fn lookup_for_provider(provider: Provider, model: &str) -> Option<ModelId> {
        let key = normalize_lookup_key(model)?;
        provider_catalog(provider)?
            .models
            .iter()
            .find_map(|descriptor| {
                descriptor_matches_lookup_key(descriptor, &key).then_some(descriptor.id)
            })
    }

    pub(crate) fn descriptor_matches_lookup_key(descriptor: &ModelDescriptor, key: &str) -> bool {
        descriptor.id.as_serialized_str().eq_ignore_ascii_case(key)
            || descriptor.api_name.eq_ignore_ascii_case(key)
            || descriptor
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(key))
            || descriptor_matches_prefix_alias(descriptor, key)
    }

    fn descriptor_matches_prefix_alias(descriptor: &ModelDescriptor, key: &str) -> bool {
        descriptor
            .prefix_aliases
            .iter()
            .any(|alias| key.starts_with(alias))
    }

    fn register_lookup_key(lookup: &mut HashMap<String, ModelId>, raw: &str, model: ModelId) {
        if let Some(key) = normalize_lookup_key(raw) {
            lookup.entry(key).or_insert(model);
        }
    }

    pub(crate) fn normalize_lookup_key(value: &str) -> Option<String> {
        let normalized = value.trim();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_ascii_lowercase())
        }
    }

    mod anthropic {
        use super::*;

        const OPUS_ALIASES: &[&str] = &["claude-opus-4-6-20260205", "claude-opus-4-6-20250514"];
        const SONNET_ALIASES: &[&str] = &["claude-sonnet-4-5-20250514", "claude-sonnet-4-20250514"];
        const HAIKU_ALIASES: &[&str] = &["claude-haiku-4-5-20250514", "claude-haiku-4-20250514"];
        const OPUS_PREFIX_ALIASES: &[&str] = &["claude-opus-4-6", "claude-opus-4"];
        const SONNET_PREFIX_ALIASES: &[&str] = &["claude-sonnet-4"];
        const HAIKU_PREFIX_ALIASES: &[&str] = &["claude-haiku-4"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::ClaudeOpus4_6,
                Provider::Anthropic,
                "claude-opus-4-6",
                "Claude Opus 4.6",
                true,
            )
            .with_aliases(OPUS_ALIASES)
            .with_prefix_aliases(OPUS_PREFIX_ALIASES)
            .with_same_provider_fallback(ModelId::ClaudeSonnet4_5)
            .with_openrouter_equivalent(ModelId::OrClaudeOpus4_6),
            ModelDescriptor::new(
                ModelId::ClaudeSonnet4_5,
                Provider::Anthropic,
                "claude-sonnet-4-5",
                "Claude Sonnet 4.5",
                true,
            )
            .with_aliases(SONNET_ALIASES)
            .with_prefix_aliases(SONNET_PREFIX_ALIASES)
            .with_same_provider_fallback(ModelId::ClaudeHaiku4_5)
            .with_openrouter_equivalent(ModelId::OrClaudeOpus4_6),
            ModelDescriptor::new(
                ModelId::ClaudeHaiku4_5,
                Provider::Anthropic,
                "claude-haiku-4-5",
                "Claude Haiku 4.5",
                true,
            )
            .with_aliases(HAIKU_ALIASES)
            .with_prefix_aliases(HAIKU_PREFIX_ALIASES),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::Anthropic, ModelId::ClaudeSonnet4_5, MODELS);
    }

    mod codex {
        use super::*;

        const GPT_5_4_ALIASES: &[&str] = &["gpt-5.4-codex"];
        const GPT_5_4_MINI_ALIASES: &[&str] = &["gpt-5.4-mini-codex"];
        const GPT_5_5_ALIASES: &[&str] = &["gpt-5.5-codex"];
        const GPT_5_5_PRO_ALIASES: &[&str] = &["gpt-5.5-pro-codex"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::Gpt5_4Codex,
                Provider::Codex,
                "gpt-5.4",
                "GPT-5.4",
                false,
            )
            .with_aliases(GPT_5_4_ALIASES)
            .with_client_kind(ClientKind::CodexCli)
            .with_same_provider_fallback(ModelId::Gpt5_4MiniCodex),
            ModelDescriptor::new(
                ModelId::Gpt5_4MiniCodex,
                Provider::Codex,
                "gpt-5.4-mini",
                "GPT-5.4 Mini",
                false,
            )
            .with_aliases(GPT_5_4_MINI_ALIASES)
            .with_client_kind(ClientKind::CodexCli),
            ModelDescriptor::new(
                ModelId::Gpt5_5Codex,
                Provider::Codex,
                "gpt-5.5",
                "Codex GPT-5.5",
                false,
            )
            .with_aliases(GPT_5_5_ALIASES)
            .with_client_kind(ClientKind::CodexCli)
            .with_same_provider_fallback(ModelId::Gpt5_4Codex),
            ModelDescriptor::new(
                ModelId::Gpt5_5ProCodex,
                Provider::Codex,
                "gpt-5.5-pro",
                "Codex GPT-5.5 Pro",
                false,
            )
            .with_aliases(GPT_5_5_PRO_ALIASES)
            .with_client_kind(ClientKind::CodexCli)
            .with_same_provider_fallback(ModelId::Gpt5_5Codex),
            ModelDescriptor::new(
                ModelId::Gpt5Codex,
                Provider::Codex,
                "gpt-5-codex",
                "Codex GPT-5",
                false,
            )
            .with_client_kind(ClientKind::CodexCli),
            ModelDescriptor::new(
                ModelId::Gpt5_1Codex,
                Provider::Codex,
                "gpt-5.1-codex",
                "Codex GPT-5.1",
                false,
            )
            .with_client_kind(ClientKind::CodexCli),
            ModelDescriptor::new(
                ModelId::Gpt5_2Codex,
                Provider::Codex,
                "gpt-5.2-codex",
                "Codex GPT-5.2",
                false,
            )
            .with_client_kind(ClientKind::CodexCli),
            ModelDescriptor::new(
                ModelId::CodexCli,
                Provider::Codex,
                "gpt-5.3-codex",
                "Codex GPT-5.3",
                false,
            )
            .with_client_kind(ClientKind::CodexCli),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::Codex, ModelId::Gpt5_5Codex, MODELS);
    }

    mod deepseek {
        use super::*;

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::DeepseekChat,
                Provider::DeepSeek,
                "deepseek-chat",
                "DeepSeek Chat",
                true,
            )
            .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
            ModelDescriptor::new(
                ModelId::DeepseekReasoner,
                Provider::DeepSeek,
                "deepseek-reasoner",
                "DeepSeek Reasoner",
                true,
            )
            .with_same_provider_fallback(ModelId::DeepseekChat)
            .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
            ModelDescriptor::new(
                ModelId::DeepseekV4Pro,
                Provider::DeepSeek,
                "deepseek-v4-pro",
                "DeepSeek V4 Pro",
                true,
            )
            .with_same_provider_fallback(ModelId::DeepseekV4Flash)
            .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
            ModelDescriptor::new(
                ModelId::DeepseekV4Flash,
                Provider::DeepSeek,
                "deepseek-v4-flash",
                "DeepSeek V4 Flash",
                true,
            )
            .with_same_provider_fallback(ModelId::DeepseekChat)
            .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::DeepSeek, ModelId::DeepseekV4Pro, MODELS);
    }

    mod doubao {
        use super::*;

        const DOUBAO_ALIASES: &[&str] = &["doubao-pro", "doubao"];

        pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
            ModelId::DoubaoPro,
            Provider::Doubao,
            "doubao-pro-256k",
            "Doubao Pro",
            true,
        )
        .with_aliases(DOUBAO_ALIASES)];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::Doubao, ModelId::DoubaoPro, MODELS);
    }

    mod google {
        use super::*;

        const GEMINI_25_PRO_ALIASES: &[&str] = &["gemini-pro"];
        const GEMINI_25_FLASH_ALIASES: &[&str] = &["gemini-flash"];
        const GEMINI_3_PRO_ALIASES: &[&str] = &["gemini-3-pro"];
        const GEMINI_3_FLASH_ALIASES: &[&str] = &["gemini-3-flash"];
        const GEMINI_CLI_ALIASES: &[&str] = &["gemini-cli"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::Gemini25Pro,
                Provider::Google,
                "gemini-2.5-pro",
                "Gemini 2.5 Pro",
                true,
            )
            .with_aliases(GEMINI_25_PRO_ALIASES)
            .with_same_provider_fallback(ModelId::Gemini25Flash)
            .with_openrouter_equivalent(ModelId::OrGemini3Pro),
            ModelDescriptor::new(
                ModelId::Gemini25Flash,
                Provider::Google,
                "gemini-2.5-flash",
                "Gemini 2.5 Flash",
                true,
            )
            .with_aliases(GEMINI_25_FLASH_ALIASES)
            .with_same_provider_fallback(ModelId::Gemini3Flash)
            .with_openrouter_equivalent(ModelId::OrGemini3Pro),
            ModelDescriptor::new(
                ModelId::Gemini3Pro,
                Provider::Google,
                "gemini-3-pro-preview",
                "Gemini 3 Pro Preview",
                true,
            )
            .with_aliases(GEMINI_3_PRO_ALIASES),
            ModelDescriptor::new(
                ModelId::Gemini3Flash,
                Provider::Google,
                "gemini-3-flash-preview",
                "Gemini 3 Flash Preview",
                true,
            )
            .with_aliases(GEMINI_3_FLASH_ALIASES),
            ModelDescriptor::new(
                ModelId::GeminiCli,
                Provider::Google,
                "gemini-2.5-pro",
                "Gemini CLI",
                false,
            )
            .with_aliases(GEMINI_CLI_ALIASES)
            .with_client_kind(ClientKind::GeminiCli),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::Google, ModelId::Gemini3Pro, MODELS);
    }

    mod groq {
        use super::*;

        const SCOUT_ALIASES: &[&str] = &["groq-scout", "llama-4-scout"];
        const MAVERICK_ALIASES: &[&str] = &["groq-maverick", "llama-4-maverick"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::GroqLlama4Scout,
                Provider::Groq,
                "meta-llama/llama-4-scout-17b-16e-instruct",
                "Llama 4 Scout",
                true,
            )
            .with_aliases(SCOUT_ALIASES),
            ModelDescriptor::new(
                ModelId::GroqLlama4Maverick,
                Provider::Groq,
                "meta-llama/llama-4-maverick-17b-128e-instruct",
                "Llama 4 Maverick",
                true,
            )
            .with_aliases(MAVERICK_ALIASES),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::Groq, ModelId::GroqLlama4Maverick, MODELS);
    }

    mod minimax {
        use super::*;

        const M21_ALIASES: &[&str] = &["minimax-m2.1"];
        const M25_ALIASES: &[&str] = &["minimax-m2.5"];
        const M27_ALIASES: &[&str] = &["minimax-m2.7"];
        const M27_HIGHSPEED_ALIASES: &[&str] = &["minimax-m2.7-highspeed"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::MiniMaxM21,
                Provider::MiniMax,
                "MiniMax-M2.1",
                "MiniMax M2.1",
                true,
            )
            .with_aliases(M21_ALIASES)
            .with_canonical_family("minimax-m2-1")
            .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
            ModelDescriptor::new(
                ModelId::MiniMaxM25,
                Provider::MiniMax,
                "MiniMax-M2.5",
                "MiniMax M2.5",
                true,
            )
            .with_aliases(M25_ALIASES)
            .with_canonical_family("minimax-m2-5")
            .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
            ModelDescriptor::new(
                ModelId::MiniMaxM27,
                Provider::MiniMax,
                "MiniMax-M2.7",
                "MiniMax M2.7",
                true,
            )
            .with_aliases(M27_ALIASES)
            .with_canonical_family("minimax-m2-7")
            .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
            ModelDescriptor::new(
                ModelId::MiniMaxM27Highspeed,
                Provider::MiniMax,
                "MiniMax-M2.7-highspeed",
                "MiniMax M2.7 Highspeed",
                true,
            )
            .with_aliases(M27_HIGHSPEED_ALIASES)
            .with_canonical_family("minimax-m2-7-highspeed")
            .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::MiniMax, ModelId::MiniMaxM27, MODELS);
    }

    mod minimax_coding_plan {
        use super::*;

        const M21_ALIASES: &[&str] = &["minimax-coding-plan-m2.1", "minimax-m2-1", "minimax-m2.1"];
        const M25_ALIASES: &[&str] = &[
            "minimax-coding-plan-m2.5",
            "minimax-m2-5",
            "minimax-m2.5",
            "minimax-coding-plan",
            "minimax-coding",
            "coding-plan-minimax",
            "minimax/coding-plan",
        ];
        const M25_HIGHSPEED_ALIASES: &[&str] = &[
            "minimax-coding-plan-m2.5-highspeed",
            "minimax-m2-5-highspeed",
            "minimax-m2.5-highspeed",
        ];
        const M27_ALIASES: &[&str] = &["minimax-coding-plan-m2.7", "minimax-m2-7", "minimax-m2.7"];
        const M27_HIGHSPEED_ALIASES: &[&str] = &[
            "minimax-coding-plan-m2.7-highspeed",
            "minimax-m2-7-highspeed",
            "minimax-m2.7-highspeed",
        ];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::MiniMaxM21CodingPlan,
                Provider::MiniMaxCodingPlan,
                "MiniMax-M2.1",
                "MiniMax M2.1 (Coding Plan)",
                true,
            )
            .with_aliases(M21_ALIASES)
            .with_canonical_family("minimax-m2-1")
            .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
            ModelDescriptor::new(
                ModelId::MiniMaxM25CodingPlan,
                Provider::MiniMaxCodingPlan,
                "MiniMax-M2.5",
                "MiniMax M2.5 (Coding Plan)",
                true,
            )
            .with_aliases(M25_ALIASES)
            .with_canonical_family("minimax-m2-5")
            .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
            ModelDescriptor::new(
                ModelId::MiniMaxM25CodingPlanHighspeed,
                Provider::MiniMaxCodingPlan,
                "MiniMax-M2.5-highspeed",
                "MiniMax M2.5 Highspeed (Coding Plan)",
                true,
            )
            .with_aliases(M25_HIGHSPEED_ALIASES)
            .with_canonical_family("minimax-m2-5-highspeed")
            .with_same_provider_fallback(ModelId::MiniMaxM25CodingPlan)
            .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
            ModelDescriptor::new(
                ModelId::MiniMaxM27CodingPlan,
                Provider::MiniMaxCodingPlan,
                "MiniMax-M2.7",
                "MiniMax M2.7 (Coding Plan)",
                true,
            )
            .with_aliases(M27_ALIASES)
            .with_canonical_family("minimax-m2-7")
            .with_same_provider_fallback(ModelId::MiniMaxM25CodingPlan)
            .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
            ModelDescriptor::new(
                ModelId::MiniMaxM27CodingPlanHighspeed,
                Provider::MiniMaxCodingPlan,
                "MiniMax-M2.7-highspeed",
                "MiniMax M2.7 Highspeed (Coding Plan)",
                true,
            )
            .with_aliases(M27_HIGHSPEED_ALIASES)
            .with_canonical_family("minimax-m2-7-highspeed")
            .with_same_provider_fallback(ModelId::MiniMaxM27CodingPlan)
            .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
        ];

        // Keep the best-quality flagship distinct from the conservative default model.
        // Provider metadata owns the default selection (M2.5), while the catalog
        // flagship remains the recommended top-end coding-plan model (M2.7).
        pub const CATALOG: ProviderCatalog = ProviderCatalog::new(
            Provider::MiniMaxCodingPlan,
            ModelId::MiniMaxM27CodingPlan,
            MODELS,
        );

        #[cfg(test)]
        mod tests {
            use super::CATALOG;
            use crate::{ModelId, ModelProvider, provider_meta};

            #[test]
            fn default_model_is_intentionally_distinct_from_flagship() {
                let provider_meta = provider_meta(ModelProvider::MiniMaxCodingPlan);

                assert_eq!(
                    provider_meta.default_model_id,
                    ModelId::MiniMaxM25CodingPlan
                );
                assert_eq!(CATALOG.flagship, ModelId::MiniMaxM27CodingPlan);
                assert_ne!(provider_meta.default_model_id, CATALOG.flagship);
            }

            #[test]
            fn catalog_contains_default_and_flagship_models() {
                assert!(
                    CATALOG
                        .models
                        .iter()
                        .any(|descriptor| descriptor.id == ModelId::MiniMaxM25CodingPlan)
                );
                assert!(
                    CATALOG
                        .models
                        .iter()
                        .any(|descriptor| descriptor.id == ModelId::MiniMaxM27CodingPlan)
                );
            }
        }
    }

    mod moonshot {
        use super::*;

        const KIMI_ALIASES: &[&str] = &["kimi-k2-5", "kimi", "moonshot"];

        pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
            ModelId::KimiK2_5,
            Provider::Moonshot,
            "kimi-k2.5",
            "Kimi K2.5",
            true,
        )
        .with_aliases(KIMI_ALIASES)
        .with_openrouter_equivalent(ModelId::OrKimiK2_5)];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::Moonshot, ModelId::KimiK2_5, MODELS);
    }

    mod openai {
        use super::*;

        const OPENCODE_ALIASES: &[&str] = &["opencode-cli"];
        const GPT_5_1_ALIASES: &[&str] = &["gpt-5-1"];
        const GPT_5_2_ALIASES: &[&str] = &["gpt-5-2"];
        const GPT_5_4_ALIASES: &[&str] = &["gpt-5-4"];
        const GPT_5_4_MINI_ALIASES: &[&str] = &["gpt-5-4-mini"];
        const GPT_5_4_NANO_ALIASES: &[&str] = &["gpt-5-4-nano"];
        const GPT_5_5_ALIASES: &[&str] = &["gpt-5-5"];
        const GPT_5_5_PRO_ALIASES: &[&str] = &["gpt-5-5-pro"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::Gpt5_4,
                Provider::OpenAI,
                "gpt-5.4",
                "GPT-5.4",
                false,
            )
            .with_aliases(GPT_5_4_ALIASES)
            .with_same_provider_fallback(ModelId::Gpt5_4Mini)
            .with_openrouter_equivalent(ModelId::OrGpt5),
            ModelDescriptor::new(
                ModelId::Gpt5_4Mini,
                Provider::OpenAI,
                "gpt-5.4-mini",
                "GPT-5.4 Mini",
                false,
            )
            .with_aliases(GPT_5_4_MINI_ALIASES)
            .with_same_provider_fallback(ModelId::Gpt5_4Nano)
            .with_openrouter_equivalent(ModelId::OrGpt5),
            ModelDescriptor::new(
                ModelId::Gpt5_4Nano,
                Provider::OpenAI,
                "gpt-5.4-nano",
                "GPT-5.4 Nano",
                false,
            )
            .with_aliases(GPT_5_4_NANO_ALIASES),
            ModelDescriptor::new(ModelId::Gpt5, Provider::OpenAI, "gpt-5", "GPT-5", false)
                .with_same_provider_fallback(ModelId::Gpt5Mini)
                .with_openrouter_equivalent(ModelId::OrGpt5),
            ModelDescriptor::new(
                ModelId::Gpt5Mini,
                Provider::OpenAI,
                "gpt-5-mini",
                "GPT-5 Mini",
                false,
            )
            .with_same_provider_fallback(ModelId::Gpt5Nano)
            .with_openrouter_equivalent(ModelId::OrGpt5),
            ModelDescriptor::new(
                ModelId::Gpt5Nano,
                Provider::OpenAI,
                "gpt-5-nano",
                "GPT-5 Nano",
                false,
            ),
            ModelDescriptor::new(
                ModelId::Gpt5Pro,
                Provider::OpenAI,
                "gpt-5-pro",
                "GPT-5 Pro",
                false,
            )
            .with_same_provider_fallback(ModelId::Gpt5)
            .with_openrouter_equivalent(ModelId::OrGpt5),
            ModelDescriptor::new(
                ModelId::Gpt5_1,
                Provider::OpenAI,
                "gpt-5.1",
                "GPT-5.1",
                false,
            )
            .with_aliases(GPT_5_1_ALIASES),
            ModelDescriptor::new(
                ModelId::Gpt5_2,
                Provider::OpenAI,
                "gpt-5.2",
                "GPT-5.2",
                false,
            )
            .with_aliases(GPT_5_2_ALIASES),
            ModelDescriptor::new(
                ModelId::Gpt5_5,
                Provider::OpenAI,
                "gpt-5.5",
                "GPT-5.5",
                false,
            )
            .with_aliases(GPT_5_5_ALIASES)
            .with_same_provider_fallback(ModelId::Gpt5_4)
            .with_openrouter_equivalent(ModelId::OrGpt5),
            ModelDescriptor::new(
                ModelId::Gpt5_5Pro,
                Provider::OpenAI,
                "gpt-5.5-pro",
                "GPT-5.5 Pro",
                false,
            )
            .with_aliases(GPT_5_5_PRO_ALIASES)
            .with_same_provider_fallback(ModelId::Gpt5_5),
            ModelDescriptor::new(
                ModelId::OpenCodeCli,
                Provider::OpenAI,
                "opencode",
                "OpenCode CLI",
                false,
            )
            .with_aliases(OPENCODE_ALIASES)
            .with_client_kind(ClientKind::OpenCodeCli),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::OpenAI, ModelId::Gpt5_5, MODELS);
    }

    mod openrouter {
        use super::*;

        const OPENROUTER_AUTO_ALIASES: &[&str] = &["openrouter"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::OpenRouterAuto,
                Provider::OpenRouter,
                "openrouter/auto",
                "OpenRouter Auto",
                true,
            )
            .with_aliases(OPENROUTER_AUTO_ALIASES),
            ModelDescriptor::new(
                ModelId::OrClaudeOpus4_6,
                Provider::OpenRouter,
                "anthropic/claude-opus-4.6",
                "OR Claude Opus 4.6",
                true,
            ),
            ModelDescriptor::new(
                ModelId::OrGpt5,
                Provider::OpenRouter,
                "openai/gpt-5",
                "OR GPT-5",
                false,
            ),
            ModelDescriptor::new(
                ModelId::OrGemini3Pro,
                Provider::OpenRouter,
                "google/gemini-3-pro-preview",
                "OR Gemini 3 Pro",
                true,
            ),
            ModelDescriptor::new(
                ModelId::OrDeepseekV3_2,
                Provider::OpenRouter,
                "deepseek/deepseek-v3.2",
                "OR DeepSeek V3.2",
                true,
            ),
            ModelDescriptor::new(
                ModelId::OrGrok4,
                Provider::OpenRouter,
                "x-ai/grok-4",
                "OR Grok 4",
                true,
            ),
            ModelDescriptor::new(
                ModelId::OrLlama4Maverick,
                Provider::OpenRouter,
                "meta-llama/llama-4-maverick",
                "OR Llama 4 Maverick",
                true,
            ),
            ModelDescriptor::new(
                ModelId::OrQwen3Coder,
                Provider::OpenRouter,
                "qwen/qwen3-coder",
                "OR Qwen3 Coder",
                true,
            ),
            ModelDescriptor::new(
                ModelId::OrDevstral2,
                Provider::OpenRouter,
                "mistralai/devstral-2-2512",
                "OR Devstral 2",
                true,
            ),
            ModelDescriptor::new(
                ModelId::OrGlm4_7,
                Provider::OpenRouter,
                "z-ai/glm-4.7",
                "OR GLM-4.7",
                true,
            ),
            ModelDescriptor::new(
                ModelId::OrKimiK2_5,
                Provider::OpenRouter,
                "moonshotai/kimi-k2.5",
                "OR Kimi K2.5",
                true,
            ),
            ModelDescriptor::new(
                ModelId::OrMinimaxM2_1,
                Provider::OpenRouter,
                "minimax/minimax-m2.1",
                "OR MiniMax M2.1",
                true,
            ),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::OpenRouter, ModelId::OrClaudeOpus4_6, MODELS);
    }

    mod qwen {
        use super::*;

        const QWEN3_MAX_ALIASES: &[&str] = &["qwen-max", "qwen"];
        const QWEN3_PLUS_ALIASES: &[&str] = &["qwen-plus"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::Qwen3Max,
                Provider::Qwen,
                "qwen3-max",
                "Qwen3 Max",
                true,
            )
            .with_aliases(QWEN3_MAX_ALIASES)
            .with_openrouter_equivalent(ModelId::OrQwen3Coder),
            ModelDescriptor::new(
                ModelId::Qwen3Plus,
                Provider::Qwen,
                "qwen3-plus",
                "Qwen3 Plus",
                true,
            )
            .with_aliases(QWEN3_PLUS_ALIASES)
            .with_openrouter_equivalent(ModelId::OrQwen3Coder),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::Qwen, ModelId::Qwen3Max, MODELS);
    }

    mod siliconflow {
        use super::*;

        const SILICONFLOW_AUTO_ALIASES: &[&str] = &["siliconflow"];

        pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
            ModelId::SiliconFlowAuto,
            Provider::SiliconFlow,
            "deepseek-ai/DeepSeek-V3",
            "SiliconFlow Auto",
            true,
        )
        .with_aliases(SILICONFLOW_AUTO_ALIASES)];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::SiliconFlow, ModelId::SiliconFlowAuto, MODELS);
    }

    mod xai {
        use super::*;

        const GROK4_ALIASES: &[&str] = &["grok4"];
        const GROK3_MINI_ALIASES: &[&str] = &["grok3-mini"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(ModelId::Grok4, Provider::XAI, "grok-4", "Grok 4", true)
                .with_aliases(GROK4_ALIASES)
                .with_same_provider_fallback(ModelId::Grok3Mini)
                .with_openrouter_equivalent(ModelId::OrGrok4),
            ModelDescriptor::new(
                ModelId::Grok3Mini,
                Provider::XAI,
                "grok-3-mini",
                "Grok 3 Mini",
                true,
            )
            .with_aliases(GROK3_MINI_ALIASES)
            .with_openrouter_equivalent(ModelId::OrGrok4),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::XAI, ModelId::Grok4, MODELS);
    }

    mod yi {
        use super::*;

        const YI_ALIASES: &[&str] = &["yi"];

        pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
            ModelId::YiLightning,
            Provider::Yi,
            "yi-lightning",
            "Yi Lightning",
            true,
        )
        .with_aliases(YI_ALIASES)];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::Yi, ModelId::YiLightning, MODELS);
    }

    mod zai {
        use super::*;

        const ZAI_CODING_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
        const GLM5_ALIASES: &[&str] = &["glm5"];
        const GLM5_TURBO_ALIASES: &[&str] = &["glm5-turbo"];
        const GLM5_CODE_ALIASES: &[&str] = &["glm5-code"];
        const GLM47_ALIASES: &[&str] = &["glm-4.7", "glm-4-7", "glm"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(ModelId::Glm5, Provider::Zai, "glm-5", "GLM-5", true)
                .with_aliases(GLM5_ALIASES)
                .with_canonical_family("glm-5")
                .with_same_provider_fallback(ModelId::Glm5Turbo)
                .with_openrouter_equivalent(ModelId::OrGlm4_7),
            ModelDescriptor::new(
                ModelId::Glm5Turbo,
                Provider::Zai,
                "glm-5-turbo",
                "GLM-5 Turbo",
                true,
            )
            .with_aliases(GLM5_TURBO_ALIASES)
            .with_canonical_family("glm-5-turbo")
            .with_same_provider_fallback(ModelId::Glm5Code)
            .with_openrouter_equivalent(ModelId::OrGlm4_7),
            ModelDescriptor::new(
                ModelId::Glm5Code,
                Provider::Zai,
                "glm-5",
                "GLM-5 Code",
                true,
            )
            .with_aliases(GLM5_CODE_ALIASES)
            .with_base_url_override(ZAI_CODING_BASE_URL)
            .with_canonical_family("glm-5-code")
            .with_same_provider_fallback(ModelId::Glm4_7)
            .with_openrouter_equivalent(ModelId::OrGlm4_7),
            ModelDescriptor::new(ModelId::Glm4_7, Provider::Zai, "glm-4.7", "GLM-4.7", true)
                .with_aliases(GLM47_ALIASES)
                .with_canonical_family("glm-4-7")
                .with_openrouter_equivalent(ModelId::OrGlm4_7),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::Zai, ModelId::Glm5, MODELS);
    }

    mod zai_coding_plan {
        use super::*;

        const ZAI_CODING_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
        const GLM5_1_ALIASES: &[&str] = &[
            "glm5.1",
            "glm-5.1",
            "glm5.1 coding plan",
            "glm-5.1 coding plan",
            "zai-coding-plan-glm5.1",
            "zai-coding-plan-glm-5.1",
        ];
        const GLM5_ALIASES: &[&str] = &[
            "glm5",
            "glm5 coding plan",
            "glm-5 coding plan",
            "zai-coding-plan",
            "zai-coding-plan-glm5",
            "zai-coding-plan-glm-5",
        ];
        const GLM5_TURBO_ALIASES: &[&str] = &[
            "glm5-turbo",
            "glm5 turbo coding plan",
            "glm-5 turbo coding plan",
            "glm-5-turbo coding plan",
            "zai-coding-plan-glm5-turbo",
            "zai-coding-plan-glm-5-turbo",
        ];
        const GLM5_CODE_ALIASES: &[&str] = &[
            "glm5-code",
            "glm5 coding plan code",
            "glm-5 coding plan code",
            "glm-5 coding-plan code",
            "zai-coding-plan-glm-5-code",
        ];
        const GLM47_ALIASES: &[&str] = &["glm-4.7", "zai-coding-plan-glm-4-7"];

        pub const MODELS: &[ModelDescriptor] = &[
            ModelDescriptor::new(
                ModelId::Glm5_1CodingPlan,
                Provider::ZaiCodingPlan,
                "glm-5.1",
                "GLM-5.1 (Coding Plan)",
                true,
            )
            .with_aliases(GLM5_1_ALIASES)
            .with_base_url_override(ZAI_CODING_BASE_URL)
            .with_canonical_family("glm-5-1")
            .with_same_provider_fallback(ModelId::Glm5CodingPlan),
            ModelDescriptor::new(
                ModelId::Glm5CodingPlan,
                Provider::ZaiCodingPlan,
                "glm-5",
                "GLM-5 (Coding Plan)",
                true,
            )
            .with_aliases(GLM5_ALIASES)
            .with_base_url_override(ZAI_CODING_BASE_URL)
            .with_canonical_family("glm-5")
            .with_same_provider_fallback(ModelId::Glm5TurboCodingPlan)
            .with_openrouter_equivalent(ModelId::OrGlm4_7),
            ModelDescriptor::new(
                ModelId::Glm5TurboCodingPlan,
                Provider::ZaiCodingPlan,
                "glm-5-turbo",
                "GLM-5 Turbo (Coding Plan)",
                true,
            )
            .with_aliases(GLM5_TURBO_ALIASES)
            .with_base_url_override(ZAI_CODING_BASE_URL)
            .with_canonical_family("glm-5-turbo")
            .with_same_provider_fallback(ModelId::Glm5CodeCodingPlan)
            .with_openrouter_equivalent(ModelId::OrGlm4_7),
            ModelDescriptor::new(
                ModelId::Glm5CodeCodingPlan,
                Provider::ZaiCodingPlan,
                "glm-5",
                "GLM-5 Code (Coding Plan)",
                true,
            )
            .with_aliases(GLM5_CODE_ALIASES)
            .with_base_url_override(ZAI_CODING_BASE_URL)
            .with_canonical_family("glm-5-code")
            .with_same_provider_fallback(ModelId::Glm4_7CodingPlan)
            .with_openrouter_equivalent(ModelId::OrGlm4_7),
            ModelDescriptor::new(
                ModelId::Glm4_7CodingPlan,
                Provider::ZaiCodingPlan,
                "glm-4.7",
                "GLM-4.7 (Coding Plan)",
                true,
            )
            .with_aliases(GLM47_ALIASES)
            .with_base_url_override(ZAI_CODING_BASE_URL)
            .with_canonical_family("glm-4-7")
            .with_openrouter_equivalent(ModelId::OrGlm4_7),
        ];

        pub const CATALOG: ProviderCatalog =
            ProviderCatalog::new(Provider::ZaiCodingPlan, ModelId::Glm5_1CodingPlan, MODELS);
    }
}

pub mod config_types {
    //! Configuration data types shared across crates.
    //!
    //! Pure data structures with no database or file I/O dependencies.
    //! Validation logic and TOML persistence live in `runtime`.

    use serde::{Deserialize, Serialize};

    use crate::defaults::*;

    // ── Local constants ──────────────────────────────────────────────────

    const DEFAULT_WORKER_COUNT: usize = 4;
    const DEFAULT_STALL_TIMEOUT_SECONDS: u64 = 600;
    const DEFAULT_MAX_RETRIES: u32 = 3;
    const DEFAULT_CHAT_SESSION_RETENTION_DAYS: u32 = 30;
    const DEFAULT_LOG_FILE_RETENTION_DAYS: u32 = 30;
    const DEFAULT_SESSION_LIST_LIMIT: u32 = 20;

    // ── CLI types ────────────────────────────────────────────────────────

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[cfg_attr(feature = "specta", derive(specta::Type))]
    #[serde(default)]
    pub struct CliConfig {
        pub version: u32,
        pub agent: Option<String>,
        pub model: Option<String>,
    }

    impl Default for CliConfig {
        fn default() -> Self {
            Self {
                version: 1,
                agent: None,
                model: None,
            }
        }
    }

    // ── SystemSection ────────────────────────────────────────────────────

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[cfg_attr(feature = "specta", derive(specta::Type))]
    #[serde(default)]
    pub struct SystemSection {
        pub worker_count: usize,
        pub stall_timeout_seconds: u64,
        #[serde(default)]
        pub chat_response_timeout_seconds: Option<u64>,
        pub max_retries: u32,
        pub chat_session_retention_days: u32,
        pub log_file_retention_days: u32,
        pub experimental_features: Vec<String>,
    }

    impl Default for SystemSection {
        fn default() -> Self {
            Self {
                worker_count: DEFAULT_WORKER_COUNT,
                stall_timeout_seconds: DEFAULT_STALL_TIMEOUT_SECONDS,
                chat_response_timeout_seconds: None,
                max_retries: DEFAULT_MAX_RETRIES,
                chat_session_retention_days: DEFAULT_CHAT_SESSION_RETENTION_DAYS,
                log_file_retention_days: DEFAULT_LOG_FILE_RETENTION_DAYS,
                experimental_features: Vec::new(),
            }
        }
    }

    impl From<&SystemConfig> for SystemSection {
        fn from(config: &SystemConfig) -> Self {
            Self {
                worker_count: config.worker_count,
                stall_timeout_seconds: config.stall_timeout_seconds,
                chat_response_timeout_seconds: config.chat_response_timeout_seconds,
                max_retries: config.max_retries,
                chat_session_retention_days: config.chat_session_retention_days,
                log_file_retention_days: config.log_file_retention_days,
                experimental_features: config.experimental_features.clone(),
            }
        }
    }

    // ── AgentDefaults ────────────────────────────────────────────────────

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[cfg_attr(feature = "specta", derive(specta::Type))]
    #[serde(default)]
    pub struct AgentDefaults {
        pub tool_timeout_secs: u64,
        pub llm_timeout_secs: Option<u64>,
        pub bash_timeout_secs: u64,
        pub python_timeout_secs: u64,
        pub browser_timeout_secs: u64,
        pub approval_timeout_secs: u64,
        pub max_iterations: usize,
        pub max_depth: usize,
        pub subagent_timeout_secs: u64,
        pub max_parallel_subagents: usize,
        pub max_tool_calls: usize,
        pub max_tool_concurrency: usize,
        pub max_tool_result_length: usize,
        pub prune_tool_max_chars: usize,
        pub compact_preserve_tokens: usize,
        pub max_wall_clock_secs: Option<u64>,
        #[serde(default)]
        pub fallback_models: Option<Vec<String>>,
    }

    pub type AgentSettings = AgentDefaults;

    impl Default for AgentDefaults {
        fn default() -> Self {
            Self {
                tool_timeout_secs: DEFAULT_AGENT_TOOL_TIMEOUT_SECS,
                llm_timeout_secs: Some(DEFAULT_AGENT_LLM_TIMEOUT_SECS),
                bash_timeout_secs: DEFAULT_AGENT_BASH_TIMEOUT_SECS,
                python_timeout_secs: DEFAULT_AGENT_PYTHON_TIMEOUT_SECS,
                browser_timeout_secs: DEFAULT_AGENT_BROWSER_TIMEOUT_SECS,
                approval_timeout_secs: DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS,
                max_iterations: DEFAULT_AGENT_MAX_ITERATIONS,
                max_depth: DEFAULT_SUBAGENT_MAX_DEPTH,
                subagent_timeout_secs: DEFAULT_SUBAGENT_TIMEOUT_SECS,
                max_parallel_subagents: DEFAULT_MAX_PARALLEL_SUBAGENTS,
                max_tool_calls: DEFAULT_AGENT_MAX_TOOL_CALLS,
                max_tool_concurrency: DEFAULT_AGENT_MAX_TOOL_CONCURRENCY,
                max_tool_result_length: DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH,
                prune_tool_max_chars: DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS,
                compact_preserve_tokens: DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS,
                max_wall_clock_secs: None,
                fallback_models: None,
            }
        }
    }

    // ── ApiDefaults ──────────────────────────────────────────────────────

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[cfg_attr(feature = "specta", derive(specta::Type))]
    #[serde(default)]
    pub struct ApiDefaults {
        pub session_list_limit: u32,
        pub web_search_num_results: usize,
    }

    pub type ApiSettings = ApiDefaults;

    impl Default for ApiDefaults {
        fn default() -> Self {
            Self {
                session_list_limit: DEFAULT_SESSION_LIST_LIMIT,
                web_search_num_results: DEFAULT_API_WEB_SEARCH_RESULTS,
            }
        }
    }

    // ── RuntimeDefaults ──────────────────────────────────────────────────

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[cfg_attr(feature = "specta", derive(specta::Type))]
    #[serde(default)]
    pub struct RuntimeDefaults {
        pub chat_max_session_history: usize,
    }

    pub type RuntimeSettings = RuntimeDefaults;

    impl Default for RuntimeDefaults {
        fn default() -> Self {
            Self {
                chat_max_session_history: DEFAULT_CHAT_MAX_SESSION_HISTORY,
            }
        }
    }

    // ── RegistryDefaults ─────────────────────────────────────────────────

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[cfg_attr(feature = "specta", derive(specta::Type))]
    #[serde(default)]
    pub struct RegistryDefaults {
        pub github_cache_ttl_secs: u64,
        pub marketplace_cache_ttl_secs: u64,
    }

    pub type RegistrySettings = RegistryDefaults;

    impl Default for RegistryDefaults {
        fn default() -> Self {
            Self {
                github_cache_ttl_secs: DEFAULT_GITHUB_CACHE_TTL_SECS,
                marketplace_cache_ttl_secs: DEFAULT_MARKETPLACE_CACHE_TTL_SECS,
            }
        }
    }

    // ── SystemConfig ─────────────────────────────────────────────────────

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[cfg_attr(feature = "specta", derive(specta::Type))]
    #[serde(default)]
    pub struct SystemConfig {
        pub worker_count: usize,
        pub stall_timeout_seconds: u64,
        #[serde(default)]
        pub chat_response_timeout_seconds: Option<u64>,
        pub max_retries: u32,
        pub chat_session_retention_days: u32,
        pub log_file_retention_days: u32,
        pub experimental_features: Vec<String>,
        #[serde(default)]
        pub agent: AgentSettings,
        #[serde(default)]
        pub api_defaults: ApiSettings,
        #[serde(default)]
        pub runtime_defaults: RuntimeSettings,
        #[serde(default)]
        pub registry_defaults: RegistrySettings,
    }

    impl Default for SystemConfig {
        fn default() -> Self {
            Self {
                worker_count: DEFAULT_WORKER_COUNT,
                stall_timeout_seconds: DEFAULT_STALL_TIMEOUT_SECONDS,
                chat_response_timeout_seconds: None,
                max_retries: DEFAULT_MAX_RETRIES,
                chat_session_retention_days: DEFAULT_CHAT_SESSION_RETENTION_DAYS,
                log_file_retention_days: DEFAULT_LOG_FILE_RETENTION_DAYS,
                experimental_features: Vec::new(),
                agent: AgentSettings::default(),
                api_defaults: ApiSettings::default(),
                runtime_defaults: RuntimeSettings::default(),
                registry_defaults: RegistrySettings::default(),
            }
        }
    }

    // ── ConfigDocument ───────────────────────────────────────────────────

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    #[cfg_attr(feature = "specta", derive(specta::Type))]
    #[serde(default, deny_unknown_fields)]
    pub struct ConfigDocument {
        pub system: SystemSection,
        pub agent: AgentSettings,
        pub api: ApiSettings,
        pub runtime: RuntimeSettings,
        pub registry: RegistrySettings,
        #[serde(default)]
        pub cli: CliConfig,
    }

    impl ConfigDocument {
        pub fn from_system_config(system: SystemConfig, cli: CliConfig) -> Self {
            Self {
                system: SystemSection::from(&system),
                agent: system.agent,
                api: system.api_defaults,
                runtime: system.runtime_defaults,
                registry: system.registry_defaults,
                cli,
            }
        }

        pub fn system_config(&self) -> SystemConfig {
            SystemConfig {
                worker_count: self.system.worker_count,
                stall_timeout_seconds: self.system.stall_timeout_seconds,
                chat_response_timeout_seconds: self.system.chat_response_timeout_seconds,
                max_retries: self.system.max_retries,
                chat_session_retention_days: self.system.chat_session_retention_days,
                log_file_retention_days: self.system.log_file_retention_days,
                experimental_features: self.system.experimental_features.clone(),
                agent: self.agent.clone(),
                api_defaults: self.api.clone(),
                runtime_defaults: self.runtime.clone(),
                registry_defaults: self.registry.clone(),
            }
        }

        pub fn replace_system_config(&mut self, system: SystemConfig) {
            self.system = SystemSection::from(&system);
            self.agent = system.agent;
            self.api = system.api_defaults;
            self.runtime = system.runtime_defaults;
            self.registry = system.registry_defaults;
        }
    }
}

pub mod defaults {
    //! Shared runtime default constants used across crates.

    /// Default maximum ReAct iterations for agent/sub-agent execution.
    pub const DEFAULT_AGENT_MAX_ITERATIONS: usize = 1000;

    /// Default maximum tool calls per agent run.
    pub const DEFAULT_AGENT_MAX_TOOL_CALLS: usize = 200;

    /// Default timeout (seconds) for the executor wrapper around tool calls.
    pub const DEFAULT_AGENT_TOOL_TIMEOUT_SECS: u64 = 300;

    /// Default timeout (seconds) for a single LLM request.
    pub const DEFAULT_AGENT_LLM_TIMEOUT_SECS: u64 = 1800;

    /// Default timeout (seconds) for bash tool execution.
    pub const DEFAULT_AGENT_BASH_TIMEOUT_SECS: u64 = 300;

    /// Default timeout (seconds) for Python tool execution.
    pub const DEFAULT_AGENT_PYTHON_TIMEOUT_SECS: u64 = 120;

    /// Default timeout (seconds) for browser tool execution.
    pub const DEFAULT_AGENT_BROWSER_TIMEOUT_SECS: u64 = 120;

    /// Default timeout (seconds) for approval requests.
    pub const DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS: u64 = 300;

    /// Default timeout (seconds) for sub-agent execution.
    pub const DEFAULT_SUBAGENT_TIMEOUT_SECS: u64 = 3600;

    /// Default cap for maximum parallel sub-agents.
    pub const DEFAULT_MAX_PARALLEL_SUBAGENTS: usize = 200;

    /// Default sub-agent nesting depth.
    pub const DEFAULT_SUBAGENT_MAX_DEPTH: usize = 1;

    /// Default maximum length of tool results kept in agent context.
    pub const DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH: usize = 4_000;

    /// Default maximum number of tool calls allowed to run concurrently.
    pub const DEFAULT_AGENT_MAX_TOOL_CONCURRENCY: usize = 100;

    /// Default fallback context window used when model metadata is unavailable.
    pub const DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS: usize = 128_000;

    /// Default max characters to keep from a pruned tool result.
    pub const DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS: usize = 2_048;

    /// Default number of recent tokens to preserve during context compaction.
    pub const DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS: usize = 20_000;

    /// Default maximum total bytes loaded from workspace instruction files.
    pub const DEFAULT_WORKSPACE_CONTEXT_MAX_TOTAL_BYTES: usize = 100_000;

    /// Default maximum bytes loaded from a single workspace instruction file.
    pub const DEFAULT_WORKSPACE_CONTEXT_MAX_FILE_BYTES: usize = 50_000;

    /// Default maximum chat session history preserved for channel conversations.
    pub const DEFAULT_CHAT_MAX_SESSION_HISTORY: usize = 20;

    /// Default number of results returned by web search when no limit is specified.
    pub const DEFAULT_API_WEB_SEARCH_RESULTS: usize = 5;

    /// Hard cap for web search results per request.
    pub const MAX_API_WEB_SEARCH_RESULTS: usize = 10;

    /// Default cache TTL (seconds) for GitHub registry results.
    pub const DEFAULT_GITHUB_CACHE_TTL_SECS: u64 = 600;

    /// Default cache TTL (seconds) for marketplace registry results.
    pub const DEFAULT_MARKETPLACE_CACHE_TTL_SECS: u64 = 300;
}

pub mod error {
    //! Shared error types.

    use serde::{Deserialize, Serialize};
    use specta::Type;
    use thiserror::Error;

    /// Structured validation error for model and API validation.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    pub struct ValidationError {
        pub field: String,
        pub message: String,
    }

    impl ValidationError {
        pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
            Self {
                field: field.into(),
                message: message.into(),
            }
        }
    }

    /// Payload returned to clients when validation fails.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    pub struct ValidationErrorResponse {
        #[serde(rename = "type")]
        pub error_type: String,
        pub errors: Vec<ValidationError>,
    }

    impl ValidationErrorResponse {
        pub fn new(errors: Vec<ValidationError>) -> Self {
            Self {
                error_type: "validation_error".to_string(),
                errors,
            }
        }
    }

    pub fn encode_validation_error(errors: Vec<ValidationError>) -> String {
        let response = ValidationErrorResponse::new(errors);
        serde_json::to_string(&response).unwrap_or_else(|_| {
        "{\"type\":\"validation_error\",\"errors\":[{\"field\":\"_global\",\"message\":\"Validation failed\"}]}".to_string()
    })
    }

    pub const SESSION_NOT_FOUND: &str = "Session not found";

    pub fn session_not_found_message(session_id: &str) -> String {
        format!("{SESSION_NOT_FOUND}: {session_id}")
    }

    /// Tool-specific error types.
    #[derive(Error, Debug)]
    pub enum ToolError {
        #[error("tool error: {0}")]
        Tool(String),

        #[error("execution failed: {0}")]
        Execution(#[from] std::io::Error),

        #[error("invalid input: {0}")]
        InvalidInput(String),

        #[error("security blocked: {0}")]
        SecurityBlocked(String),

        #[error("tool not found: {0}")]
        NotFound(String),

        #[error("JSON error: {0}")]
        Json(#[from] serde_json::Error),

        #[error("{0}")]
        Other(#[from] anyhow::Error),
    }

    /// Result type alias for tool operations.
    pub type Result<T> = std::result::Result<T, ToolError>;
}

pub mod llm {
    //! LLM switching abstractions.
    //!
    //! Defines the [`LlmSwitcher`] trait for runtime model switching without
    //! coupling consumers to concrete LLM client implementations.

    use crate::error::ToolError;

    /// Concrete execution path used to satisfy an LLM request.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ClientKind {
        Http,
        CodexCli,
        OpenCodeCli,
        GeminiCli,
        ClaudeCodeCli,
    }

    impl ClientKind {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Http => "http",
                Self::CodexCli => "codex-cli",
                Self::OpenCodeCli => "opencode-cli",
                Self::GeminiCli => "gemini-cli",
                Self::ClaudeCodeCli => "claude-code-cli",
            }
        }

        pub fn is_cli(self) -> bool {
            !matches!(self, Self::Http)
        }
    }

    macro_rules! define_llm_provider_enum {
    ($($variant:ident => { name: $name:literal, base_url: $base_url:literal }),+ $(,)?) => {
        /// Runtime provider bucket used by the LLM factory layer.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum LlmProvider {
            $(
                $variant,
            )+
        }

        impl LlmProvider {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            pub fn base_url(self) -> &'static str {
                match self {
                    $(Self::$variant => $base_url,)+
                }
            }
        }
    };
}

    define_llm_provider_enum! {
        OpenAI => { name: "openai", base_url: "https://api.openai.com/v1" },
        Anthropic => { name: "anthropic", base_url: "" },
        DeepSeek => { name: "deepseek", base_url: "https://api.deepseek.com/v1" },
        Google => { name: "google", base_url: "https://generativelanguage.googleapis.com/v1beta/openai" },
        Groq => { name: "groq", base_url: "https://api.groq.com/openai/v1" },
        OpenRouter => { name: "openrouter", base_url: "https://openrouter.ai/api/v1" },
        XAI => { name: "xai", base_url: "https://api.x.ai/v1" },
        Qwen => { name: "qwen", base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1" },
        Zai => { name: "zai", base_url: "https://api.z.ai/api/paas/v4" },
        ZaiCodingPlan => { name: "zai-coding-plan", base_url: "https://api.z.ai/api/coding/paas/v4" },
        Moonshot => { name: "moonshot", base_url: "https://api.moonshot.cn/v1" },
        Doubao => { name: "doubao", base_url: "https://ark.cn-beijing.volces.com/api/v3" },
        Yi => { name: "yi", base_url: "https://api.lingyiwanwu.com/v1" },
        SiliconFlow => { name: "siliconflow", base_url: "https://api.siliconflow.cn/v1" },
        MiniMax => { name: "minimax", base_url: "https://api.minimax.io" },
        MiniMaxCodingPlan => { name: "minimax-coding-plan", base_url: "https://api.minimax.io" },
    }

    /// Result of a successful model swap.
    #[derive(Debug, Clone)]
    pub struct SwapResult {
        /// Previous provider label reported by the active client.
        pub previous_provider: String,
        /// Previous model name.
        pub previous_model: String,
        /// Previous runtime provider bucket, when known.
        pub previous_runtime_provider: Option<LlmProvider>,
        /// New provider label reported by the active client.
        pub new_provider: String,
        /// New model name.
        pub new_model: String,
        /// New runtime provider bucket.
        pub new_runtime_provider: LlmProvider,
    }

    /// Runtime LLM model switching.
    ///
    /// Abstracts `SwappableLlm` + `LlmClientFactory` so that tool implementations
    /// can switch models without depending on the concrete AI framework.
    pub trait LlmSwitcher: Send + Sync {
        /// Current model name.
        fn current_model(&self) -> String;

        /// Current provider label reported by the active client.
        fn current_provider(&self) -> String;

        /// Current runtime provider bucket, when it can be derived from the active model.
        fn current_runtime_provider(&self) -> Option<LlmProvider> {
            let current_model = self.current_model();
            self.provider_for_model(&current_model)
        }

        /// List all available model names.
        fn available_models(&self) -> Vec<String>;

        /// Return the runtime provider bucket for a given model, if known.
        fn provider_for_model(&self, model: &str) -> Option<LlmProvider>;

        /// Resolve the API key for a runtime provider bucket.
        fn resolve_api_key(&self, provider: LlmProvider) -> Option<String>;

        /// Return the concrete client kind for a known model.
        fn client_kind_for_model(&self, model: &str) -> Option<ClientKind>;

        /// Create a new LLM client for the given model and swap the active client.
        ///
        /// Returns the previous and new provider/model information.
        fn create_and_swap(
            &self,
            model: &str,
            api_key: Option<&str>,
        ) -> std::result::Result<SwapResult, ToolError>;

        /// Switch to a new model using the switcher's built-in provider/api-key
        /// resolution semantics.
        fn switch_model(&self, model: &str) -> std::result::Result<SwapResult, ToolError> {
            let provider = self
                .provider_for_model(model)
                .ok_or_else(|| ToolError::Tool(format!("Unknown model: {model}")))?;
            let client_kind = self
                .client_kind_for_model(model)
                .unwrap_or(ClientKind::Http);
            let api_key = if client_kind.is_cli() {
                self.resolve_api_key(provider)
            } else {
                Some(self.resolve_api_key(provider).ok_or_else(|| {
                ToolError::Tool(format!(
                    "No API key for provider '{}'. Set the key via manage_secrets tool (e.g., ANTHROPIC_API_KEY, OPENAI_API_KEY).",
                    provider.as_str(),
                ))
            })?)
            };

            self.create_and_swap(model, api_key.as_deref())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{ClientKind, LlmProvider, LlmSwitcher, SwapResult};
        use crate::error::ToolError;
        use std::sync::Mutex;

        struct MockSwitcher {
            current_model: Mutex<String>,
            key: Option<String>,
            kind: ClientKind,
        }

        impl MockSwitcher {
            fn new(kind: ClientKind, key: Option<&str>) -> Self {
                Self {
                    current_model: Mutex::new("initial".to_string()),
                    key: key.map(str::to_string),
                    kind,
                }
            }
        }

        impl LlmSwitcher for MockSwitcher {
            fn current_model(&self) -> String {
                self.current_model.lock().unwrap().clone()
            }

            fn current_provider(&self) -> String {
                "openai".to_string()
            }

            fn available_models(&self) -> Vec<String> {
                vec!["gpt-5".to_string()]
            }

            fn provider_for_model(&self, _model: &str) -> Option<LlmProvider> {
                Some(LlmProvider::OpenAI)
            }

            fn resolve_api_key(&self, _provider: LlmProvider) -> Option<String> {
                self.key.clone()
            }

            fn client_kind_for_model(&self, _model: &str) -> Option<ClientKind> {
                Some(self.kind)
            }

            fn create_and_swap(
                &self,
                model: &str,
                _api_key: Option<&str>,
            ) -> std::result::Result<SwapResult, ToolError> {
                let previous_model = self.current_model();
                *self.current_model.lock().unwrap() = model.to_string();
                Ok(SwapResult {
                    previous_provider: "openai".to_string(),
                    previous_model,
                    previous_runtime_provider: Some(LlmProvider::OpenAI),
                    new_provider: "openai".to_string(),
                    new_model: model.to_string(),
                    new_runtime_provider: LlmProvider::OpenAI,
                })
            }
        }

        #[test]
        fn default_switch_model_requires_api_key_for_http_models() {
            let switcher = MockSwitcher::new(ClientKind::Http, None);
            let error = switcher.switch_model("gpt-5").unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("No API key for provider 'openai'")
            );
        }

        #[test]
        fn default_switch_model_skips_api_key_for_cli_models() {
            let switcher = MockSwitcher::new(ClientKind::CodexCli, None);
            let result = switcher.switch_model("gpt-5").unwrap();
            assert_eq!(result.new_model, "gpt-5");
        }

        #[test]
        fn current_runtime_provider_defaults_to_model_lookup() {
            let switcher = MockSwitcher::new(ClientKind::Http, Some("test-key"));
            assert_eq!(
                switcher.current_runtime_provider(),
                Some(LlmProvider::OpenAI)
            );
        }
    }
}

pub mod model {
    //! Shared model/provider primitives for cross-crate normalization.

    use serde::{Deserialize, Deserializer, Serialize};
    use specta::Type;

    use crate::request::WireModelRef;
    use crate::{ModelId, Provider, ValidationError};

    /// Model metadata containing provider, temperature support, and display name.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ModelMetadata {
        pub provider: Provider,
        pub supports_temperature: bool,
        pub name: &'static str,
    }

    /// Serializable model metadata for runtime clients.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
    pub struct ModelMetadataDTO {
        pub model: ModelId,
        pub provider: Provider,
        pub supports_temperature: bool,
        pub name: String,
    }

    /// Provider + model pair used by API and persistence layers.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
    pub struct ModelRef {
        pub provider: Provider,
        pub model: ModelId,
    }

    impl ModelRef {
        /// Build a consistent model reference from a model enum.
        pub fn from_model(model: ModelId) -> Self {
            Self {
                provider: model.provider(),
                model,
            }
        }

        /// Validate that provider and model provider metadata are consistent.
        pub fn validate(&self) -> Result<(), ValidationError> {
            let expected_provider = self.model.provider();
            if self.provider != expected_provider {
                return Err(ValidationError::new(
                    "model_ref",
                    format!(
                        "provider '{}' does not match model provider '{}'",
                        self.provider.as_canonical_str(),
                        expected_provider.as_canonical_str()
                    ),
                ));
            }
            Ok(())
        }

        /// Return canonical ID in `provider:model` format.
        pub fn canonical_id(&self) -> String {
            format!(
                "{}:{}",
                self.provider.as_canonical_str(),
                self.model.as_serialized_str()
            )
        }

        /// Resolve user input into a provider/model pair without silently
        /// choosing one provider for colliding unqualified names.
        pub fn from_unambiguous_user_input(input: &str) -> Result<Self, String> {
            let model = ModelId::from_unambiguous_user_input(input)?;
            Ok(Self::from_model(model))
        }

        /// Resolve legacy provider-less config/session input.
        ///
        /// Older RestFlow config and JSONL session metadata stored only an API
        /// model string. If that string now collides with a CLI provider model
        /// ID, keep the old API-model behavior by preferring OpenAI rather than
        /// silently falling back to an agent default.
        pub fn from_legacy_providerless_user_input(input: &str) -> Result<Self, String> {
            match Self::from_unambiguous_user_input(input) {
                Ok(model_ref) => Ok(model_ref),
                Err(error) => {
                    if let Some(model) = ModelId::from_provider_user_input(
                        Provider::OpenAI.as_canonical_str(),
                        input,
                    ) {
                        return Ok(Self::from_model(model));
                    }
                    Err(error)
                }
            }
        }
    }

    impl TryFrom<WireModelRef> for ModelRef {
        type Error = ValidationError;

        fn try_from(value: WireModelRef) -> Result<Self, Self::Error> {
            let provider = Provider::from_canonical_str(&value.provider).ok_or_else(|| {
                ValidationError::new(
                    "model_ref.provider",
                    format!("unknown provider '{}'", value.provider),
                )
            })?;
            let model =
                ModelId::for_provider_and_model(provider, &value.model).ok_or_else(|| {
                    ValidationError::new(
                        "model_ref.model",
                        format!("unknown model '{}'", value.model),
                    )
                })?;

            let model_ref = Self { provider, model };
            model_ref.validate()?;
            Ok(model_ref)
        }
    }

    impl From<ModelRef> for WireModelRef {
        fn from(value: ModelRef) -> Self {
            Self {
                provider: value.provider.as_canonical_str().to_string(),
                model: value.model.as_serialized_str().to_string(),
            }
        }
    }

    macro_rules! define_model_provider {
    ($($variant:ident => { canonical: $canonical:literal, key: $key:literal, aliases: [$($alias:literal),* $(,)?] }),+ $(,)?) => {
        /// Canonical model provider identity shared by runtime and tooling layers.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Type)]
        pub enum ModelProvider {
            $(
                #[serde(rename = $canonical)]
                $variant,
            )+
        }

        impl ModelProvider {
            /// Return all canonical providers in a stable order.
            pub fn all() -> &'static [Self] {
                &[
                    $(Self::$variant,)+
                ]
            }

            /// Return canonical provider string used by config and API payloads.
            pub fn canonical_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $canonical,)+
                }
            }

            /// Parse user input/provider aliases into canonical provider identity.
            pub fn parse_alias(value: &str) -> Option<Self> {
                let normalized: String = value
                    .trim()
                    .chars()
                    .filter(|ch| ch.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();

                match normalized.as_str() {
                    $(
                        $key => Some(Self::$variant),
                        $($alias => Some(Self::$variant),)*
                    )+
                    _ => None,
                }
            }
        }
    };
}

    define_model_provider! {
        OpenAI => { canonical: "openai", key: "openai", aliases: ["gpt"] },
        Anthropic => { canonical: "anthropic", key: "anthropic", aliases: ["claude"] },
        ClaudeCode => { canonical: "claude-code", key: "claudecode", aliases: ["claudecodecli"] },
        Codex => { canonical: "codex", key: "codex", aliases: ["openaicodex", "openaicodexcli"] },
        DeepSeek => { canonical: "deepseek", key: "deepseek", aliases: [] },
        Google => { canonical: "google", key: "google", aliases: ["gemini"] },
        Groq => { canonical: "groq", key: "groq", aliases: [] },
        OpenRouter => { canonical: "openrouter", key: "openrouter", aliases: [] },
        XAI => { canonical: "xai", key: "xai", aliases: ["xaiapi", "grok"] },
        Qwen => { canonical: "qwen", key: "qwen", aliases: [] },
        Zai => { canonical: "zai", key: "zai", aliases: ["zhipu"] },
        ZaiCodingPlan => { canonical: "zai-coding-plan", key: "zaicodingplan", aliases: ["zaicoding", "zhipucodingplan"] },
        Moonshot => { canonical: "moonshot", key: "moonshot", aliases: ["kimi"] },
        Doubao => { canonical: "doubao", key: "doubao", aliases: ["ark"] },
        Yi => { canonical: "yi", key: "yi", aliases: [] },
        SiliconFlow => { canonical: "siliconflow", key: "siliconflow", aliases: [] },
        MiniMax => { canonical: "minimax", key: "minimax", aliases: [] },
        MiniMaxCodingPlan => { canonical: "minimax-coding-plan", key: "minimaxcodingplan", aliases: ["minimaxcoding"] },
    }

    impl<'de> Deserialize<'de> for ModelProvider {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let raw = String::deserialize(deserializer)?;
            Self::parse_alias(&raw)
                .ok_or_else(|| serde::de::Error::custom(format!("unknown provider: {raw}")))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::ModelProvider;

        #[test]
        fn parse_alias_supports_common_shortcuts() {
            assert_eq!(
                ModelProvider::parse_alias("gpt"),
                Some(ModelProvider::OpenAI)
            );
            assert_eq!(
                ModelProvider::parse_alias("gemini"),
                Some(ModelProvider::Google)
            );
            assert_eq!(
                ModelProvider::parse_alias("claude-code"),
                Some(ModelProvider::ClaudeCode)
            );
            assert_eq!(
                ModelProvider::parse_alias("openai-codex"),
                Some(ModelProvider::Codex)
            );
            assert_eq!(
                ModelProvider::parse_alias("zai-coding"),
                Some(ModelProvider::ZaiCodingPlan)
            );
            assert_eq!(
                ModelProvider::parse_alias("minimax_coding"),
                Some(ModelProvider::MiniMaxCodingPlan)
            );
        }

        #[test]
        fn canonical_str_is_stable() {
            assert_eq!(ModelProvider::OpenAI.canonical_str(), "openai");
            assert_eq!(ModelProvider::ClaudeCode.canonical_str(), "claude-code");
            assert_eq!(ModelProvider::Codex.canonical_str(), "codex");
            assert_eq!(
                ModelProvider::ZaiCodingPlan.canonical_str(),
                "zai-coding-plan"
            );
            assert_eq!(
                ModelProvider::MiniMaxCodingPlan.canonical_str(),
                "minimax-coding-plan"
            );
        }

        #[test]
        fn deserialize_accepts_aliases() {
            let parsed: ModelProvider = serde_json::from_str("\"gpt\"").unwrap();
            assert_eq!(parsed, ModelProvider::OpenAI);

            let parsed: ModelProvider = serde_json::from_str("\"openai-codex\"").unwrap();
            assert_eq!(parsed, ModelProvider::Codex);
        }
    }
}

mod model_id {
    use crate::{ClientKind, ModelMetadata, ModelMetadataDTO, ModelSpec, Provider, catalog};
    use serde::{Deserialize, Deserializer, Serialize};
    use specta::Type;

    /// Canonical model identifier.
    ///
    /// This replaces the old large enum with a lightweight value object backed by
    /// the provider/model catalog.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type)]
    pub struct ModelId(&'static str);

    #[allow(non_upper_case_globals)]
    impl ModelId {
        pub const Gpt5: Self = Self("gpt-5");
        pub const Gpt5Mini: Self = Self("gpt-5-mini");
        pub const Gpt5Nano: Self = Self("gpt-5-nano");
        pub const Gpt5Pro: Self = Self("gpt-5-pro");
        pub const Gpt5_1: Self = Self("gpt-5-1");
        pub const Gpt5_2: Self = Self("gpt-5-2");
        pub const Gpt5_4: Self = Self("gpt-5-4");
        pub const Gpt5_4Mini: Self = Self("gpt-5-4-mini");
        pub const Gpt5_4Nano: Self = Self("gpt-5-4-nano");
        pub const Gpt5_5: Self = Self("gpt-5-5");
        pub const Gpt5_5Pro: Self = Self("gpt-5-5-pro");
        pub const ClaudeOpus4_6: Self = Self("claude-opus-4-6");
        pub const ClaudeSonnet4_5: Self = Self("claude-sonnet-4-5");
        pub const ClaudeHaiku4_5: Self = Self("claude-haiku-4-5");
        pub const ClaudeCodeOpus: Self = Self("claude-code-opus");
        pub const ClaudeCodeSonnet: Self = Self("claude-code-sonnet");
        pub const ClaudeCodeHaiku: Self = Self("claude-code-haiku");
        pub const DeepseekChat: Self = Self("deepseek-chat");
        pub const DeepseekReasoner: Self = Self("deepseek-reasoner");
        pub const DeepseekV4Pro: Self = Self("deepseek-v4-pro");
        pub const DeepseekV4Flash: Self = Self("deepseek-v4-flash");
        pub const Gemini25Pro: Self = Self("gemini-2-5-pro");
        pub const Gemini25Flash: Self = Self("gemini-2-5-flash");
        pub const Gemini3Pro: Self = Self("gemini-3-pro");
        pub const Gemini3Flash: Self = Self("gemini-3-flash");
        pub const GroqLlama4Scout: Self = Self("groq-llama4-scout");
        pub const GroqLlama4Maverick: Self = Self("groq-llama4-maverick");
        pub const Grok4: Self = Self("grok-4");
        pub const Grok3Mini: Self = Self("grok-3-mini");
        pub const OpenRouterAuto: Self = Self("openrouter");
        pub const OrClaudeOpus4_6: Self = Self("or-claude-opus-4-6");
        pub const OrGpt5: Self = Self("or-gpt-5");
        pub const OrGemini3Pro: Self = Self("or-gemini-3-pro");
        pub const OrDeepseekV3_2: Self = Self("or-deepseek-v3-2");
        pub const OrGrok4: Self = Self("or-grok-4");
        pub const OrLlama4Maverick: Self = Self("or-llama-4-maverick");
        pub const OrQwen3Coder: Self = Self("or-qwen3-coder");
        pub const OrDevstral2: Self = Self("or-devstral-2");
        pub const OrGlm4_7: Self = Self("or-glm-4-7");
        pub const OrKimiK2_5: Self = Self("or-kimi-k2-5");
        pub const OrMinimaxM2_1: Self = Self("or-minimax-m2-1");
        pub const Qwen3Max: Self = Self("qwen3-max");
        pub const Qwen3Plus: Self = Self("qwen3-plus");
        pub const Glm5: Self = Self("glm-5");
        pub const Glm5Turbo: Self = Self("glm-5-turbo");
        pub const Glm5Code: Self = Self("glm-5-code");
        pub const Glm4_7: Self = Self("glm-4-7");
        pub const Glm5_1CodingPlan: Self = Self("zai-coding-plan-glm-5-1");
        pub const Glm5CodingPlan: Self = Self("zai-coding-plan-glm-5");
        pub const Glm5TurboCodingPlan: Self = Self("zai-coding-plan-glm-5-turbo");
        pub const Glm5CodeCodingPlan: Self = Self("zai-coding-plan-glm-5-code");
        pub const Glm4_7CodingPlan: Self = Self("zai-coding-plan-glm-4-7");
        pub const KimiK2_5: Self = Self("kimi-k2-5");
        pub const DoubaoPro: Self = Self("doubao-pro");
        pub const YiLightning: Self = Self("yi-lightning");
        pub const SiliconFlowAuto: Self = Self("siliconflow");
        pub const MiniMaxM21: Self = Self("minimax-m2-1");
        pub const MiniMaxM25: Self = Self("minimax-m2-5");
        pub const MiniMaxM27: Self = Self("minimax-m2-7");
        pub const MiniMaxM27Highspeed: Self = Self("minimax-m2-7-highspeed");
        pub const MiniMaxM21CodingPlan: Self = Self("minimax-coding-plan-m2-1");
        pub const MiniMaxM25CodingPlan: Self = Self("minimax-coding-plan-m2-5");
        pub const MiniMaxM25CodingPlanHighspeed: Self = Self("minimax-coding-plan-m2-5-highspeed");
        pub const MiniMaxM27CodingPlan: Self = Self("minimax-coding-plan-m2-7");
        pub const MiniMaxM27CodingPlanHighspeed: Self = Self("minimax-coding-plan-m2-7-highspeed");
        pub const Gpt5_4Codex: Self = Self("gpt-5.4");
        pub const Gpt5_4MiniCodex: Self = Self("gpt-5.4-mini");
        pub const Gpt5_5Codex: Self = Self("gpt-5.5");
        pub const Gpt5_5ProCodex: Self = Self("gpt-5.5-pro");
        pub const Gpt5Codex: Self = Self("gpt-5-codex");
        pub const Gpt5_1Codex: Self = Self("gpt-5.1-codex");
        pub const Gpt5_2Codex: Self = Self("gpt-5.2-codex");
        pub const CodexCli: Self = Self("gpt-5.3-codex");
        pub const OpenCodeCli: Self = Self("opencode-cli");
        pub const GeminiCli: Self = Self("gemini-cli");

        pub const fn as_serialized_str(&self) -> &'static str {
            self.0
        }

        pub fn from_serialized_str(value: &str) -> Option<Self> {
            let normalized = value.trim();
            if normalized.is_empty() {
                return None;
            }

            catalog::lookup_by_name(normalized)
        }

        pub fn all() -> &'static [Self] {
            catalog::all_model_ids()
        }
    }

    impl Serialize for ModelId {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_str(self.0)
        }
    }

    impl<'de> Deserialize<'de> for ModelId {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let raw = String::deserialize(deserializer)?;
            Self::from_api_name(&raw)
                .or_else(|| Self::from_canonical_id(&raw))
                .or_else(|| Self::from_serialized_str(&raw))
                .ok_or_else(|| serde::de::Error::custom(format!("unknown model: {raw}")))
        }
    }

    impl ModelId {
        fn model_spec_named(&self, name: &str) -> ModelSpec {
            let descriptor = self.descriptor();
            let provider = self.provider().as_llm_provider();
            let mut spec = match descriptor.client_kind {
                ClientKind::Http => ModelSpec::new(name, provider, self.as_str()),
                ClientKind::CodexCli => ModelSpec::codex(name, self.as_str()),
                ClientKind::OpenCodeCli => ModelSpec::opencode(name, self.as_str()),
                ClientKind::GeminiCli => ModelSpec::gemini_cli(name, self.as_str()),
                ClientKind::ClaudeCodeCli => ModelSpec::claude_code(name, self.as_str()),
            };

            if let Some(base_url) = descriptor.base_url_override {
                spec = spec.with_base_url(base_url);
            }

            spec
        }

        fn descriptor(&self) -> &'static catalog::ModelDescriptor {
            catalog::descriptor(*self).unwrap_or_else(|| {
                panic!(
                    "missing model catalog entry for {}",
                    self.as_serialized_str()
                )
            })
        }

        /// Convert ModelId to ModelSpec used by runtime LLM factory.
        pub fn as_model_spec(&self) -> ModelSpec {
            self.model_spec_named(self.as_serialized_str())
        }

        /// Build the shared model catalog for dynamic model switching.
        pub fn build_model_specs() -> Vec<ModelSpec> {
            let mut specs = Vec::new();
            for model in Self::all() {
                specs.push(model.as_model_spec());

                // OpenAI 5.4 API models use serialized IDs to avoid colliding with
                // Codex CLI IDs, but runtime switching should accept the public API
                // names for OpenAI provider usage.
                if matches!(model.provider(), Provider::OpenAI)
                    && model.as_str() != model.as_serialized_str()
                {
                    specs.push(model.model_spec_named(model.as_str()));
                }

                // Claude Code aliases are matched by `as_str()` at runtime as well.
                if model.is_claude_code() {
                    specs.push(model.model_spec_named(model.as_str()));
                }
            }

            specs
        }

        /// Get comprehensive metadata for this model
        pub fn metadata(&self) -> ModelMetadata {
            self.descriptor().metadata()
        }

        /// Get the provider for this model
        pub fn provider(&self) -> Provider {
            self.metadata().provider
        }

        /// Get the concrete execution path for this model.
        pub fn client_kind(&self) -> ClientKind {
            self.descriptor().client_kind
        }

        /// Check whether the provider matches the model.
        pub fn provider_matches(&self, provider: Provider) -> bool {
            provider == self.provider()
        }

        /// Get the canonical model identity in "provider:model" format.
        /// This is the single source of truth for model identification across
        /// routing, events, pricing lookup, and logs.
        ///
        /// Format: lowercase provider:model (e.g., "openai:gpt-5", "anthropic:claude-opus-4-6")
        pub fn canonical_id(&self) -> String {
            format!(
                "{}:{}",
                self.provider().as_canonical_str(),
                self.as_serialized_str()
            )
        }

        /// Parse a canonical model ID back to ModelId.
        /// Accepts only "provider:model" format.
        ///
        /// Returns None if the model string is not recognized.
        pub fn from_canonical_id(canonical_id: &str) -> Option<Self> {
            let normalized = canonical_id.trim().to_lowercase();

            if let Some((provider_str, model_str)) = normalized.split_once(':')
                && let Some(provider) = Provider::from_canonical_str(provider_str)
            {
                return Self::for_provider_and_model(provider, model_str);
            }

            None
        }

        /// Check if this model supports temperature parameter
        pub fn supports_temperature(&self) -> bool {
            self.metadata().supports_temperature
        }

        /// Normalize accepted model identifiers into serialized enum form.
        ///
        /// Examples:
        /// - "MiniMax-M2.5" -> "minimax-m2-5"
        /// - "gpt-5.1" -> "gpt-5-1"
        /// - "openai:gpt-5" -> "gpt-5"
        pub fn normalize_model_id(input: &str) -> Option<String> {
            Self::from_user_input(input).map(|model| model.as_serialized_str().to_string())
        }

        /// Normalize model-only user input without silently selecting one provider
        /// when multiple providers expose the same public model identifier.
        pub fn normalize_unambiguous_model_id(input: &str) -> Result<String, String> {
            Self::from_unambiguous_user_input(input)
                .map(|model| model.as_serialized_str().to_string())
        }

        /// Resolve a user- or wire-facing model string into a concrete catalog model.
        ///
        /// Accepts provider-qualified identifiers (`openai:gpt-5.5`), API names,
        /// canonical IDs, aliases, and serialized enum names.
        pub fn from_user_input(input: &str) -> Option<Self> {
            let normalized = input.trim();
            if normalized.is_empty() {
                return None;
            }

            Self::from_api_name(normalized)
                .or_else(|| Self::from_canonical_id(normalized))
                .or_else(|| {
                    Self::from_serialized_str(
                        &normalized.replace([' ', '.', '/'], "-").replace('_', "-"),
                    )
                })
        }

        /// Resolve model input, rejecting unqualified names that match multiple
        /// providers. Provider-qualified values such as `codex:gpt-5.5` remain
        /// accepted.
        pub fn from_unambiguous_user_input(input: &str) -> Result<Self, String> {
            let normalized = input.trim();
            if normalized.is_empty() {
                return Err("Unsupported model identifier: empty".to_string());
            }
            if normalized.contains(':') {
                return Self::from_user_input(normalized)
                    .ok_or_else(|| format!("Unsupported model identifier: {normalized}"));
            }

            let mut providers = Vec::new();
            let Some(key) = catalog::normalize_lookup_key(normalized) else {
                return Err(format!("Unsupported model identifier: {normalized}"));
            };
            for descriptor in catalog::all_descriptors() {
                if catalog::descriptor_matches_lookup_key(descriptor, &key)
                    && !providers.contains(&descriptor.provider)
                {
                    providers.push(descriptor.provider);
                }
            }
            if providers.len() > 1 {
                providers.sort_by_key(|provider| provider.as_canonical_str());
                let matches = providers
                    .iter()
                    .map(|provider| provider.as_canonical_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "Ambiguous model identifier '{normalized}' matches multiple providers ({matches}); specify provider"
                ));
            }

            Self::from_user_input(normalized)
                .ok_or_else(|| format!("Unsupported model identifier: {normalized}"))
        }

        /// Resolve a model string for a specific provider.
        pub fn from_provider_user_input(provider: &str, model: &str) -> Option<Self> {
            let provider = Provider::from_canonical_str(provider.trim())?;
            Self::for_provider_and_model(provider, model.trim())
        }

        /// Get the string representation used for API calls
        pub fn as_str(&self) -> &'static str {
            self.descriptor().api_name
        }

        /// Convert an API model name into an ModelId.
        pub fn from_api_name(name: &str) -> Option<Self> {
            let normalized = name.trim();
            if normalized.is_empty() {
                return None;
            }

            catalog::lookup_by_name(normalized)
        }

        /// Resolve a concrete model for a specific provider/model pair.
        pub fn for_provider_and_model(provider: Provider, model: &str) -> Option<Self> {
            let normalized = model.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                return None;
            }

            catalog::lookup_for_provider(provider, &normalized).or_else(|| {
                let parsed = Self::from_api_name(&normalized)?;
                parsed.provider_matches(provider).then_some(parsed)
            })
        }

        /// Get the display name for UI
        pub fn display_name(&self) -> &'static str {
            self.metadata().name
        }

        /// Check if this model uses the Codex CLI
        pub fn is_codex_cli(&self) -> bool {
            self.client_kind() == ClientKind::CodexCli
        }

        /// Check if this model uses the Claude Code CLI
        pub fn is_claude_code(&self) -> bool {
            self.client_kind() == ClientKind::ClaudeCodeCli
        }

        /// Check if this model uses the OpenCode CLI
        pub fn is_opencode_cli(&self) -> bool {
            self.client_kind() == ClientKind::OpenCodeCli
        }

        /// Check if this model uses the Gemini CLI
        pub fn is_gemini_cli(&self) -> bool {
            self.client_kind() == ClientKind::GeminiCli
        }

        /// Check if this model is any CLI-based model (manages its own auth)
        pub fn is_cli_model(&self) -> bool {
            self.client_kind().is_cli()
        }

        /// Get a same-provider fallback model (cheaper tier).
        /// Returns None if this is already the cheapest or no fallback exists.
        pub fn same_provider_fallback(&self) -> Option<Self> {
            self.descriptor().same_provider_fallback
        }

        /// Get the OpenRouter equivalent of this model (if one exists).
        pub fn openrouter_equivalent(&self) -> Option<Self> {
            self.descriptor().openrouter_equivalent
        }

        /// Get all models with their metadata as DTOs
        pub fn all_with_metadata() -> Vec<ModelMetadataDTO> {
            catalog::all_descriptors()
                .map(catalog::ModelDescriptor::metadata_dto)
                .collect()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::ModelId;

        #[test]
        fn unambiguous_model_input_rejects_cross_provider_collision() {
            let error = ModelId::normalize_unambiguous_model_id("gpt-5.5")
                .expect_err("openai/codex collision should require provider");

            assert!(error.contains("Ambiguous model identifier"));
            assert!(error.contains("codex"));
            assert!(error.contains("openai"));
        }

        #[test]
        fn unambiguous_model_input_accepts_provider_qualified_collision() {
            assert_eq!(
                ModelId::normalize_unambiguous_model_id("codex:gpt-5.5").unwrap(),
                "gpt-5.5"
            );
            assert_eq!(
                ModelId::normalize_unambiguous_model_id("openai:gpt-5.5").unwrap(),
                "gpt-5-5"
            );
        }
    }
}

pub mod orchestrator {
    //! Shared orchestration contracts for agent execution surfaces.

    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    use crate::error::ToolError;
    use crate::subagent::InlineRunConfig;

    /// Lifecycle mode for one agent execution plan.
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum ExecutionMode {
        Interactive,
        Subagent,
    }

    /// Shared execution plan consumed by orchestrators.
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct ExecutionPlan {
        /// Lifecycle mode that should handle this execution.
        pub mode: Option<ExecutionMode>,
        /// Optional stored agent identifier.
        #[serde(default)]
        pub agent_id: Option<String>,
        /// Optional inline temporary sub-agent configuration.
        #[serde(default)]
        pub inline_subagent: Option<InlineRunConfig>,
        /// Runtime input for the execution.
        #[serde(default)]
        pub input: Option<String>,
        /// Optional chat session ID for interactive mode.
        #[serde(default)]
        pub chat_session_id: Option<String>,
        /// Optional timeout override in seconds.
        #[serde(default)]
        pub timeout_secs: Option<u64>,
        /// Optional model override.
        #[serde(default)]
        pub model: Option<String>,
        /// Optional provider paired with model override.
        #[serde(default)]
        pub provider: Option<String>,
        /// Optional max iterations override.
        #[serde(default)]
        pub max_iterations: Option<u32>,
        /// Optional parent run ID.
        #[serde(default)]
        pub parent_run_id: Option<String>,
        /// Optional authoritative run ID.
        ///
        /// For sub-agent executions this identifies the canonical sub-agent run. When
        /// supplied by a caller that already owns lifecycle emission, executors
        /// must reuse this run ID without emitting a second top-level lifecycle.
        #[serde(default)]
        pub run_id: Option<String>,
        /// Mode-specific metadata payload.
        #[serde(default)]
        pub metadata: Option<Value>,
    }

    impl ExecutionPlan {
        /// Returns the canonical parent run identifier for this execution plan.
        pub fn parent_run_id(&self) -> Option<&str> {
            self.parent_run_id.as_deref()
        }

        /// Sets the canonical parent run identifier.
        pub fn set_parent_run_id(&mut self, parent_run_id: Option<String>) {
            self.parent_run_id = parent_run_id;
        }

        /// Validate that the plan contains the minimum fields required for its mode.
        pub fn validate(&self) -> Result<(), ToolError> {
            let mode = self
                .mode
                .as_ref()
                .ok_or_else(|| ToolError::Tool("Execution plan requires 'mode'.".to_string()))?;

            let has_model = self
                .model
                .as_ref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            let has_provider = self
                .provider
                .as_ref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            if has_model != has_provider {
                return Err(ToolError::Tool(
                    "Execution plan requires both 'model' and 'provider' when either is set."
                        .to_string(),
                ));
            }

            match mode {
                ExecutionMode::Interactive => {
                    if self
                        .chat_session_id
                        .as_ref()
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                    {
                        return Err(ToolError::Tool(
                            "Interactive execution requires 'chat_session_id'.".to_string(),
                        ));
                    }
                    if self
                        .input
                        .as_ref()
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                    {
                        return Err(ToolError::Tool(
                            "Interactive execution requires non-empty 'input'.".to_string(),
                        ));
                    }
                }
                ExecutionMode::Subagent => {
                    let has_selector = self
                        .agent_id
                        .as_ref()
                        .map(|value| !value.trim().is_empty())
                        .unwrap_or(false)
                        || self.inline_subagent.is_some()
                        || (has_model && has_provider);
                    if !has_selector {
                        return Err(ToolError::Tool(
                        "Subagent execution requires 'agent_id', 'inline_subagent', or paired 'model' and 'provider'.".to_string(),
                    ));
                    }
                    if self
                        .input
                        .as_ref()
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                    {
                        return Err(ToolError::Tool(
                            "Subagent execution requires non-empty 'input'.".to_string(),
                        ));
                    }
                }
            }

            Ok(())
        }
    }

    /// Normalized outcome returned by an orchestrator.
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct ExecutionOutcome {
        /// Whether execution succeeded.
        pub success: bool,
        /// Main textual output.
        #[serde(default)]
        pub text: Option<String>,
        /// Optional structured result payload.
        #[serde(default)]
        pub result: Option<Value>,
        /// Optional metadata payload.
        #[serde(default)]
        pub metadata: Option<Value>,
        /// Optional error message.
        #[serde(default)]
        pub error: Option<String>,
        /// Optional iteration count.
        #[serde(default)]
        pub iterations: Option<u32>,
        /// Optional resolved model identifier.
        #[serde(default)]
        pub model: Option<String>,
        /// Optional duration in milliseconds.
        #[serde(default)]
        pub duration_ms: Option<u64>,
    }

    /// Shared orchestrator abstraction used by higher-level lifecycle adapters.
    #[async_trait]
    pub trait AgentOrchestrator: Send + Sync {
        async fn run(&self, plan: ExecutionPlan) -> Result<ExecutionOutcome, ToolError>;
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_execution_plan_requires_paired_model_and_provider() {
            let plan = ExecutionPlan {
                mode: Some(ExecutionMode::Subagent),
                input: Some("task".to_string()),
                inline_subagent: Some(InlineRunConfig::default()),
                model: Some("gpt-5.3-codex".to_string()),
                ..ExecutionPlan::default()
            };

            let error = plan.validate().unwrap_err();
            assert!(error.to_string().contains("both 'model' and 'provider'"));
        }

        #[test]
        fn test_execution_plan_validates_subagent_mode() {
            let invalid = ExecutionPlan {
                mode: Some(ExecutionMode::Subagent),
                input: Some("task".to_string()),
                ..ExecutionPlan::default()
            };
            assert!(invalid.validate().is_err());

            let valid = ExecutionPlan {
                mode: Some(ExecutionMode::Subagent),
                input: Some("task".to_string()),
                inline_subagent: Some(InlineRunConfig::default()),
                ..ExecutionPlan::default()
            };
            assert!(valid.validate().is_ok());
        }

        #[test]
        fn test_execution_plan_parent_run_id_accessors_round_trip() {
            let mut plan = ExecutionPlan::default();
            assert_eq!(plan.parent_run_id(), None);

            plan.set_parent_run_id(Some("parent-1".to_string()));
            assert_eq!(plan.parent_run_id(), Some("parent-1"));
        }

        #[test]
        fn test_execution_plan_serializes_parent_run_id_canonically() {
            let mut plan = ExecutionPlan::default();
            plan.set_parent_run_id(Some("parent-1".to_string()));

            let serialized = serde_json::to_value(plan).expect("serialize execution plan");
            assert_eq!(serialized["parent_run_id"], "parent-1");
        }

        #[test]
        fn test_execution_plan_accepts_model_provider_only_subagent_mode() {
            let valid = ExecutionPlan {
                mode: Some(ExecutionMode::Subagent),
                input: Some("task".to_string()),
                model: Some("gpt-5.3-codex".to_string()),
                provider: Some("openai".to_string()),
                ..ExecutionPlan::default()
            };

            assert!(valid.validate().is_ok());
        }

        #[test]
        fn test_execution_plan_interactive_only_requires_session_and_input() {
            let valid = ExecutionPlan {
                mode: Some(ExecutionMode::Interactive),
                chat_session_id: Some("session-1".to_string()),
                input: Some("hello".to_string()),
                ..ExecutionPlan::default()
            };

            assert!(valid.validate().is_ok());
        }

        #[test]
        fn test_execution_plan_rejects_whitespace_only_fields() {
            let invalid = ExecutionPlan {
                mode: Some(ExecutionMode::Subagent),
                agent_id: Some("   ".to_string()),
                input: Some("   ".to_string()),
                ..ExecutionPlan::default()
            };

            assert!(invalid.validate().is_err());
        }

        #[test]
        fn test_execution_plan_accepts_optional_run_id_for_subagent_mode() {
            let valid = ExecutionPlan {
                mode: Some(ExecutionMode::Subagent),
                agent_id: Some("child".to_string()),
                input: Some("task".to_string()),
                run_id: Some("subagent-run-1".to_string()),
                ..ExecutionPlan::default()
            };

            assert!(valid.validate().is_ok());
            assert_eq!(valid.run_id.as_deref(), Some("subagent-run-1"));
        }
    }
}

mod provider {
    use crate::{ClientKind, LlmProvider, ModelId, ModelProvider};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use specta::Type;

    /// Shared metadata for a canonical provider identity.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProviderMeta {
        pub provider: ModelProvider,
        pub runtime_provider: LlmProvider,
        pub api_key_env: Option<&'static str>,
        pub default_model_id: ModelId,
        pub models_dev_provider_ids: &'static [&'static str],
    }

    impl ProviderMeta {
        pub fn canonical_name(self) -> &'static str {
            self.provider.canonical_str()
        }
    }

    pub const ALL_PROVIDER_META: &[ProviderMeta] = &[
        ProviderMeta {
            provider: ModelProvider::OpenAI,
            runtime_provider: LlmProvider::OpenAI,
            api_key_env: Some("OPENAI_API_KEY"),
            default_model_id: ModelId::Gpt5_5,
            models_dev_provider_ids: &["openai"],
        },
        ProviderMeta {
            provider: ModelProvider::Anthropic,
            runtime_provider: LlmProvider::Anthropic,
            api_key_env: Some("ANTHROPIC_API_KEY"),
            default_model_id: ModelId::ClaudeOpus4_6,
            models_dev_provider_ids: &["anthropic"],
        },
        ProviderMeta {
            provider: ModelProvider::ClaudeCode,
            runtime_provider: LlmProvider::Anthropic,
            api_key_env: None,
            default_model_id: ModelId::ClaudeCodeOpus,
            models_dev_provider_ids: &["claude-code", "anthropic"],
        },
        ProviderMeta {
            provider: ModelProvider::Codex,
            runtime_provider: LlmProvider::OpenAI,
            api_key_env: None,
            default_model_id: ModelId::Gpt5_5Codex,
            models_dev_provider_ids: &["codex", "openai-codex", "openai"],
        },
        ProviderMeta {
            provider: ModelProvider::DeepSeek,
            runtime_provider: LlmProvider::DeepSeek,
            api_key_env: Some("DEEPSEEK_API_KEY"),
            default_model_id: ModelId::DeepseekChat,
            models_dev_provider_ids: &["deepseek"],
        },
        ProviderMeta {
            provider: ModelProvider::Google,
            runtime_provider: LlmProvider::Google,
            api_key_env: Some("GEMINI_API_KEY"),
            default_model_id: ModelId::Gemini25Pro,
            models_dev_provider_ids: &["google"],
        },
        ProviderMeta {
            provider: ModelProvider::Groq,
            runtime_provider: LlmProvider::Groq,
            api_key_env: Some("GROQ_API_KEY"),
            default_model_id: ModelId::GroqLlama4Maverick,
            models_dev_provider_ids: &["groq"],
        },
        ProviderMeta {
            provider: ModelProvider::OpenRouter,
            runtime_provider: LlmProvider::OpenRouter,
            api_key_env: Some("OPENROUTER_API_KEY"),
            default_model_id: ModelId::OpenRouterAuto,
            models_dev_provider_ids: &["openrouter"],
        },
        ProviderMeta {
            provider: ModelProvider::XAI,
            runtime_provider: LlmProvider::XAI,
            api_key_env: Some("XAI_API_KEY"),
            default_model_id: ModelId::Grok4,
            models_dev_provider_ids: &["xai"],
        },
        ProviderMeta {
            provider: ModelProvider::Qwen,
            runtime_provider: LlmProvider::Qwen,
            api_key_env: Some("DASHSCOPE_API_KEY"),
            default_model_id: ModelId::Qwen3Max,
            models_dev_provider_ids: &["alibaba-cn", "alibaba"],
        },
        ProviderMeta {
            provider: ModelProvider::Zai,
            runtime_provider: LlmProvider::Zai,
            api_key_env: Some("ZAI_API_KEY"),
            default_model_id: ModelId::Glm5,
            models_dev_provider_ids: &["zai", "zhipuai"],
        },
        ProviderMeta {
            provider: ModelProvider::ZaiCodingPlan,
            runtime_provider: LlmProvider::ZaiCodingPlan,
            api_key_env: Some("ZAI_CODING_PLAN_API_KEY"),
            default_model_id: ModelId::Glm5_1CodingPlan,
            models_dev_provider_ids: &["zai-coding-plan", "zhipuai-coding-plan"],
        },
        ProviderMeta {
            provider: ModelProvider::Moonshot,
            runtime_provider: LlmProvider::Moonshot,
            api_key_env: Some("MOONSHOT_API_KEY"),
            default_model_id: ModelId::KimiK2_5,
            models_dev_provider_ids: &["moonshotai", "moonshotai-cn", "kimi-for-coding"],
        },
        ProviderMeta {
            provider: ModelProvider::Doubao,
            runtime_provider: LlmProvider::Doubao,
            api_key_env: Some("ARK_API_KEY"),
            default_model_id: ModelId::DoubaoPro,
            models_dev_provider_ids: &["doubao", "doubao-cn", "ark"],
        },
        ProviderMeta {
            provider: ModelProvider::Yi,
            runtime_provider: LlmProvider::Yi,
            api_key_env: Some("YI_API_KEY"),
            default_model_id: ModelId::YiLightning,
            models_dev_provider_ids: &["yi"],
        },
        ProviderMeta {
            provider: ModelProvider::SiliconFlow,
            runtime_provider: LlmProvider::SiliconFlow,
            api_key_env: Some("SILICONFLOW_API_KEY"),
            default_model_id: ModelId::SiliconFlowAuto,
            models_dev_provider_ids: &["siliconflow", "siliconflow-cn"],
        },
        ProviderMeta {
            provider: ModelProvider::MiniMax,
            runtime_provider: LlmProvider::MiniMax,
            api_key_env: Some("MINIMAX_API_KEY"),
            default_model_id: ModelId::MiniMaxM27,
            models_dev_provider_ids: &["minimax", "minimax-cn"],
        },
        ProviderMeta {
            provider: ModelProvider::MiniMaxCodingPlan,
            runtime_provider: LlmProvider::MiniMaxCodingPlan,
            api_key_env: Some("MINIMAX_CODING_PLAN_API_KEY"),
            default_model_id: ModelId::MiniMaxM25CodingPlan,
            models_dev_provider_ids: &["minimax-coding-plan", "minimax-cn-coding-plan"],
        },
    ];

    pub fn provider_meta(provider: ModelProvider) -> &'static ProviderMeta {
        ALL_PROVIDER_META
            .iter()
            .find(|meta| meta.provider == provider)
            .unwrap_or_else(|| panic!("missing provider metadata for {provider:?}"))
    }

    /// API-facing provider wrapper backed by the shared canonical provider identity.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type)]
    #[repr(transparent)]
    #[specta(transparent)]
    pub struct Provider(ModelProvider);

    #[allow(non_upper_case_globals)]
    impl Provider {
        pub const OpenAI: Self = Self(ModelProvider::OpenAI);
        pub const Anthropic: Self = Self(ModelProvider::Anthropic);
        pub const ClaudeCode: Self = Self(ModelProvider::ClaudeCode);
        pub const Codex: Self = Self(ModelProvider::Codex);
        pub const DeepSeek: Self = Self(ModelProvider::DeepSeek);
        pub const Google: Self = Self(ModelProvider::Google);
        pub const Groq: Self = Self(ModelProvider::Groq);
        pub const OpenRouter: Self = Self(ModelProvider::OpenRouter);
        pub const XAI: Self = Self(ModelProvider::XAI);
        pub const Qwen: Self = Self(ModelProvider::Qwen);
        pub const Zai: Self = Self(ModelProvider::Zai);
        pub const ZaiCodingPlan: Self = Self(ModelProvider::ZaiCodingPlan);
        pub const Moonshot: Self = Self(ModelProvider::Moonshot);
        pub const Doubao: Self = Self(ModelProvider::Doubao);
        pub const Yi: Self = Self(ModelProvider::Yi);
        pub const SiliconFlow: Self = Self(ModelProvider::SiliconFlow);
        pub const MiniMax: Self = Self(ModelProvider::MiniMax);
        pub const MiniMaxCodingPlan: Self = Self(ModelProvider::MiniMaxCodingPlan);

        pub fn all() -> &'static [Provider] {
            &ALL_PROVIDERS
        }

        /// Convert to shared provider identity used by cross-crate parsers.
        pub const fn as_model_provider(self) -> ModelProvider {
            self.0
        }

        /// Convert from shared provider identity.
        pub const fn from_model_provider(provider: ModelProvider) -> Self {
            Self(provider)
        }

        pub fn api_key_env(self) -> Option<&'static str> {
            provider_meta(self.0).api_key_env
        }

        pub fn api_key_env_candidates(self) -> impl Iterator<Item = &'static str> {
            provider_meta(self.0).api_key_env.into_iter()
        }

        /// Convert Provider to LLM provider used by runtime factory.
        pub fn as_llm_provider(self) -> LlmProvider {
            provider_meta(self.0).runtime_provider
        }

        /// Get the canonical provider identifier for use in canonical model IDs.
        /// Returns lowercase provider name (e.g., "openai", "anthropic").
        pub fn as_canonical_str(self) -> &'static str {
            provider_meta(self.0).canonical_name()
        }

        /// Parse a canonical provider string back to Provider.
        /// Returns None if the string is not recognized.
        pub fn from_canonical_str(s: &str) -> Option<Self> {
            ModelProvider::parse_alias(s).map(Self)
        }
    }

    impl From<ModelProvider> for Provider {
        fn from(value: ModelProvider) -> Self {
            Self(value)
        }
    }

    impl From<Provider> for ModelProvider {
        fn from(value: Provider) -> Self {
            value.0
        }
    }

    impl Serialize for Provider {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_str(self.as_canonical_str())
        }
    }

    impl<'de> Deserialize<'de> for Provider {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let raw = String::deserialize(deserializer)?;
            Self::from_canonical_str(&raw)
                .ok_or_else(|| serde::de::Error::custom(format!("unknown provider: {raw}")))
        }
    }

    const ALL_PROVIDERS: [Provider; 18] = [
        Provider::OpenAI,
        Provider::Anthropic,
        Provider::ClaudeCode,
        Provider::Codex,
        Provider::DeepSeek,
        Provider::Google,
        Provider::Groq,
        Provider::OpenRouter,
        Provider::XAI,
        Provider::Qwen,
        Provider::Zai,
        Provider::ZaiCodingPlan,
        Provider::Moonshot,
        Provider::Doubao,
        Provider::Yi,
        Provider::SiliconFlow,
        Provider::MiniMax,
        Provider::MiniMaxCodingPlan,
    ];

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ProviderSelector {
        Provider(Provider),
        ClientKind(ClientKind),
    }

    impl ProviderSelector {
        pub fn label(self) -> &'static str {
            match self {
                Self::Provider(Provider::ClaudeCode)
                | Self::ClientKind(ClientKind::ClaudeCodeCli) => "claude-code",
                Self::Provider(Provider::Codex) | Self::ClientKind(ClientKind::CodexCli) => {
                    "openai-codex"
                }
                Self::Provider(provider) => provider.as_canonical_str(),
                Self::ClientKind(ClientKind::Http) => "http",
                Self::ClientKind(ClientKind::OpenCodeCli) => "opencode-cli",
                Self::ClientKind(ClientKind::GeminiCli) => "gemini-cli",
            }
        }

        pub fn matches_model(self, model: ModelId) -> bool {
            match self {
                Self::Provider(provider) => model.provider() == provider,
                Self::ClientKind(client_kind) => model.client_kind() == client_kind,
            }
        }

        pub fn runtime_provider(self) -> Option<LlmProvider> {
            match self {
                Self::Provider(provider) => Some(provider.as_llm_provider()),
                Self::ClientKind(ClientKind::CodexCli | ClientKind::OpenCodeCli) => {
                    Some(LlmProvider::OpenAI)
                }
                Self::ClientKind(ClientKind::GeminiCli) => Some(LlmProvider::Google),
                Self::ClientKind(ClientKind::ClaudeCodeCli) => Some(LlmProvider::Anthropic),
                Self::ClientKind(ClientKind::Http) => None,
            }
        }
    }

    pub fn parse_provider_selector(value: &str) -> Option<ProviderSelector> {
        let normalized = normalize_identifier(value);
        let special = match normalized.as_str() {
            "claude-code" | "claudecode" => Some(ProviderSelector::Provider(Provider::ClaudeCode)),
            "codex" | "codex-cli" | "codexcli" | "openai-codex" | "openaicodex" => {
                Some(ProviderSelector::Provider(Provider::Codex))
            }
            "opencode" | "opencode-cli" | "opencodecli" => {
                Some(ProviderSelector::ClientKind(ClientKind::OpenCodeCli))
            }
            "gemini-cli" | "geminicli" => Some(ProviderSelector::ClientKind(ClientKind::GeminiCli)),
            _ => None,
        };
        special.or_else(|| {
            ModelProvider::parse_alias(value)
                .map(Provider::from_model_provider)
                .map(ProviderSelector::Provider)
        })
    }

    pub fn split_provider_qualified_model(value: &str) -> Option<(ProviderSelector, &str)> {
        for separator in [':', '/'] {
            let Some((provider_raw, model_raw)) = value.split_once(separator) else {
                continue;
            };
            let model_raw = model_raw.trim();
            if model_raw.is_empty() {
                continue;
            }
            if let Some(provider) = parse_provider_selector(provider_raw) {
                return Some((provider, model_raw));
            }
        }

        None
    }

    pub fn parse_model_reference(value: &str) -> Option<ModelId> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        parse_plain_model_reference(trimmed).or_else(|| {
            let (selector, model_raw) = split_provider_qualified_model(trimmed)?;
            match selector {
                ProviderSelector::Provider(provider) => {
                    ModelId::for_provider_and_model(provider, model_raw)
                }
                ProviderSelector::ClientKind(client_kind) => parse_plain_model_reference(model_raw)
                    .filter(|model| model.client_kind() == client_kind),
            }
        })
    }

    pub fn resolve_available_model_name(requested: &str, available: &[String]) -> Option<String> {
        let trimmed = requested.trim();
        if trimmed.is_empty() {
            return None;
        }

        if let Some(exact) = find_case_insensitive_match(available, trimmed) {
            return Some(exact);
        }

        if let Some(model) = parse_model_reference(trimmed)
            && let Some(resolved) = find_available_model_for_id(available, model)
        {
            return Some(resolved);
        }

        let normalized = normalize_identifier(trimmed);
        if normalized.is_empty() {
            return None;
        }

        let normalized_matches = available
            .iter()
            .filter(|candidate| normalize_identifier(candidate) == normalized)
            .collect::<Vec<_>>();
        if normalized_matches.len() == 1 {
            return Some(normalized_matches[0].clone());
        }

        let prefix_matches = available
            .iter()
            .filter(|candidate| normalize_identifier(candidate).starts_with(&normalized))
            .collect::<Vec<_>>();
        if prefix_matches.len() == 1 {
            return Some(prefix_matches[0].clone());
        }
        None
    }

    fn parse_plain_model_reference(value: &str) -> Option<ModelId> {
        ModelId::from_api_name(value)
            .or_else(|| ModelId::from_canonical_id(value))
            .or_else(|| ModelId::from_serialized_str(value))
    }

    fn find_available_model_for_id(available: &[String], model: ModelId) -> Option<String> {
        find_case_insensitive_match(available, model.as_serialized_str())
            .or_else(|| find_case_insensitive_match(available, model.as_str()))
            .or_else(|| {
                available.iter().find_map(|candidate| {
                    (parse_plain_model_reference(candidate) == Some(model))
                        .then(|| candidate.clone())
                })
            })
    }

    fn find_case_insensitive_match(available: &[String], requested: &str) -> Option<String> {
        available
            .iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(requested))
            .cloned()
    }

    fn normalize_identifier(value: &str) -> String {
        let mut normalized = String::with_capacity(value.len());
        let mut previous_dash = false;

        for ch in value.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                normalized.push(ch.to_ascii_lowercase());
                previous_dash = false;
                continue;
            }
            if !previous_dash {
                normalized.push('-');
                previous_dash = true;
            }
        }

        normalized.trim_matches('-').to_string()
    }

    #[cfg(test)]
    mod tests {
        use super::resolve_available_model_name;

        #[test]
        fn available_model_prefix_resolution_requires_unique_match() {
            let available = vec!["gpt-5.5".to_string(), "gpt-5.5-pro".to_string()];

            assert_eq!(
                resolve_available_model_name("gpt-5.5", &available).as_deref(),
                Some("gpt-5.5")
            );
            assert_eq!(
                resolve_available_model_name("gpt-5.5-p", &available).as_deref(),
                Some("gpt-5.5-pro")
            );
            assert_eq!(resolve_available_model_name("gpt", &available), None);
        }
    }
}

pub mod run {
    use serde::{Deserialize, Serialize};
    use specta::Type;

    #[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum ExecutionContainerKind {
        Workspace,
    }

    #[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum RunKind {
        WorkspaceRun,
        SubagentRun,
    }

    #[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum RunStatus {
        Running,
        Completed,
        Interrupted,
        Failed,
    }

    impl RunStatus {
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::Running => "running",
                Self::Completed => "completed",
                Self::Interrupted => "interrupted",
                Self::Failed => "failed",
            }
        }
    }

    impl std::fmt::Display for RunStatus {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.as_str())
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    pub struct ExecutionContainerSummary {
        pub id: String,
        pub kind: ExecutionContainerKind,
        pub title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub subtitle: Option<String>,
        pub updated_at: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub status: Option<String>,
        #[serde(default)]
        pub session_count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub latest_session_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub latest_run_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub agent_id: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    pub struct ExecutionContainerRef {
        pub kind: ExecutionContainerKind,
        pub id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    pub struct RunSummary {
        pub id: String,
        pub kind: RunKind,
        pub container_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub root_run_id: Option<String>,
        pub title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub subtitle: Option<String>,
        pub status: RunStatus,
        pub updated_at: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub started_at: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub ended_at: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub session_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub run_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub parent_run_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub agent_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub effective_model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub provider: Option<String>,
        #[serde(default)]
        pub event_count: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    pub struct RunListQuery {
        pub container: ExecutionContainerRef,
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn run_types_expose_canonical_surface() {
            let summary = RunSummary {
                id: "run-1".to_string(),
                kind: RunKind::WorkspaceRun,
                container_id: "workspace".to_string(),
                root_run_id: Some("run-1".to_string()),
                title: "Example Run".to_string(),
                subtitle: None,
                status: RunStatus::Completed,
                updated_at: 1,
                started_at: Some(1),
                ended_at: Some(2),
                session_id: Some("session-1".to_string()),
                run_id: Some("run-1".to_string()),
                parent_run_id: None,
                agent_id: Some("agent-1".to_string()),
                effective_model: Some("gpt-5.4".to_string()),
                provider: Some("openai".to_string()),
                event_count: 3,
            };
            let query = RunListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::Workspace,
                    id: "workspace".to_string(),
                },
            };
            assert_eq!(summary.run_id.as_deref(), Some("run-1"));
            assert_eq!(query.container.id, "workspace");
        }
    }
}

pub mod session {
    //! Chat session models for workspace conversation persistence.
    //!
    //! This module defines data structures for storing and managing chat sessions
    //! within the SkillWorkspace, enabling persistent conversations with agents.
    //!
    //! # Architecture
    //!
    //! ```text
    //! ┌──────────────────────────────────────────────────────────────┐
    //! │                    Chat Session Storage                       │
    //! │                                                               │
    //! │  ChatSession                                                  │
    //! │  ├── id: "session-abc123"                                    │
    //! │  ├── agent_id: "research-agent"                              │
    //! │  ├── model: "claude-sonnet-4-20250514"                       │
    //! │  ├── messages: [ChatMessage, ChatMessage, ...]               │
    //! │  └── metadata: { total_tokens: 1500, message_count: 5 }      │
    //! │                                                               │
    //! │  ChatMessage                                                  │
    //! │  ├── role: User | Assistant | System                         │
    //! │  ├── content: "Hello, can you help me..."                    │
    //! │  ├── timestamp: 1706567890000                                │
    //! │  └── execution: Option<MessageExecution>                     │
    //! └──────────────────────────────────────────────────────────────┘
    //! ```

    use crate::model::ModelRef;
    use crate::model_id::ModelId;
    use serde::{Deserialize, Serialize};
    use specta::Type;
    use std::collections::HashSet;

    /// Role of a message sender in a chat session.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
    #[serde(rename_all = "snake_case")]
    pub enum ChatRole {
        /// Message from the user
        #[default]
        User,
        /// Message from the AI assistant
        Assistant,
        /// System message (instructions, context)
        System,
    }

    /// Status of message execution (distinct from workflow ExecutionStatus).
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
    #[serde(rename_all = "snake_case")]
    pub enum ChatExecutionStatus {
        /// Execution is in progress
        #[default]
        Running,
        /// Execution completed successfully
        Completed,
        /// Execution failed with error
        Failed,
    }

    /// Structured media type for a chat message.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum ChatMediaType {
        /// Voice audio message.
        Voice,
    }

    /// Structured media payload for a chat message.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    pub struct ChatMessageMedia {
        /// Media kind.
        pub media_type: ChatMediaType,
        /// Local file path for this media asset.
        pub file_path: String,
        /// Optional media duration in seconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub duration_sec: Option<u32>,
    }

    impl ChatMessageMedia {
        /// Create a voice media descriptor.
        pub fn voice(file_path: impl Into<String>, duration_sec: Option<u32>) -> Self {
            Self {
                media_type: ChatMediaType::Voice,
                file_path: file_path.into(),
                duration_sec,
            }
        }
    }

    /// Structured transcript payload for a chat message.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    pub struct ChatMessageTranscript {
        /// Final transcript text.
        pub text: String,
        /// Optional model identifier used for transcription.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub model: Option<String>,
        /// Optional update timestamp in Unix milliseconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub updated_at: Option<i64>,
    }

    impl ChatMessageTranscript {
        /// Create transcript payload with optional model metadata.
        pub fn new(text: impl Into<String>, model: Option<String>) -> Self {
            Self {
                text: text.into(),
                model,
                updated_at: Some(chrono::Utc::now().timestamp_millis()),
            }
        }
    }

    /// Information about a single execution step.
    ///
    /// Tracks individual steps taken during agent execution, such as
    /// tool calls, API requests, or thinking processes.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
    pub struct ExecutionStepInfo {
        /// Type of step (e.g., "tool_call", "api_request", "thinking")
        pub step_type: String,
        /// Human-readable name of the step
        pub name: String,
        /// Current status of this step
        pub status: String,
        /// Duration of this step in milliseconds (if completed)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub duration_ms: Option<u64>,
    }

    impl ExecutionStepInfo {
        /// Create a new execution step info.
        pub fn new(step_type: impl Into<String>, name: impl Into<String>) -> Self {
            Self {
                step_type: step_type.into(),
                name: name.into(),
                status: "running".to_string(),
                duration_ms: None,
            }
        }

        /// Set the status of this step.
        pub fn with_status(mut self, status: impl Into<String>) -> Self {
            self.status = status.into();
            self
        }

        /// Set the duration of this step.
        pub fn with_duration(mut self, duration_ms: u64) -> Self {
            self.duration_ms = Some(duration_ms);
            self
        }
    }

    /// Execution details for an assistant message.
    ///
    /// Contains information about what the agent did to generate the response,
    /// including tool calls, duration, and token usage.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
    pub struct MessageExecution {
        /// Individual steps taken during execution
        pub steps: Vec<ExecutionStepInfo>,
        /// Total execution duration in milliseconds
        pub duration_ms: u64,
        /// Number of tokens used for this response
        pub tokens_used: u32,
        /// Cost in USD for this response
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cost_usd: Option<f64>,
        /// Input tokens for this response
        #[serde(skip_serializing_if = "Option::is_none")]
        pub input_tokens: Option<u32>,
        /// Output tokens for this response
        #[serde(skip_serializing_if = "Option::is_none")]
        pub output_tokens: Option<u32>,
        /// Overall execution status
        pub status: ChatExecutionStatus,
    }

    impl Default for MessageExecution {
        fn default() -> Self {
            Self {
                steps: Vec::new(),
                duration_ms: 0,
                tokens_used: 0,
                cost_usd: None,
                input_tokens: None,
                output_tokens: None,
                status: ChatExecutionStatus::Running,
            }
        }
    }

    impl MessageExecution {
        /// Create a new message execution tracker.
        pub fn new() -> Self {
            Self::default()
        }

        /// Add an execution step.
        pub fn add_step(&mut self, step: ExecutionStepInfo) {
            self.steps.push(step);
        }

        /// Mark execution as completed.
        pub fn complete(mut self, duration_ms: u64, tokens_used: u32) -> Self {
            self.duration_ms = duration_ms;
            self.tokens_used = tokens_used;
            self.status = ChatExecutionStatus::Completed;
            self
        }

        /// Mark execution as failed.
        pub fn fail(mut self, duration_ms: u64) -> Self {
            self.duration_ms = duration_ms;
            self.status = ChatExecutionStatus::Failed;
            self
        }
    }

    /// A single message in a chat session.
    ///
    /// Represents either a user message, assistant response, or system instruction.
    /// Assistant messages may include execution details showing what the agent did.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
    pub struct ChatMessage {
        /// Unique identifier for this message
        #[serde(default = "new_message_id")]
        pub id: String,
        /// Role of the message sender
        pub role: ChatRole,
        /// Message content (text)
        pub content: String,
        /// Unix timestamp in milliseconds when the message was created
        pub timestamp: i64,
        /// Execution details for assistant messages
        #[serde(skip_serializing_if = "Option::is_none")]
        pub execution: Option<MessageExecution>,
        /// Optional structured media metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub media: Option<ChatMessageMedia>,
        /// Optional structured transcript metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub transcript: Option<ChatMessageTranscript>,
    }

    /// Lifecycle state for a user-visible conversation turn.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
    #[serde(rename_all = "snake_case")]
    pub enum ChatTurnStatus {
        /// The turn is currently executing.
        #[default]
        Running,
        /// The turn finished with an assistant response.
        Completed,
        /// The turn was interrupted or canceled by the user.
        Canceled,
        /// The turn failed before producing a final response.
        Failed,
    }

    impl ChatTurnStatus {
        pub fn is_terminal(self) -> bool {
            matches!(self, Self::Completed | Self::Canceled | Self::Failed)
        }
    }

    /// User-visible event inside a chat turn.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum ChatTurnEventKind {
        /// User input that started the turn.
        UserMessage { content: String },
        /// Final or partial assistant text captured for this turn.
        AssistantMessage { content: String },
        /// A tool call started during the turn.
        ToolCall {
            call_id: String,
            name: String,
            arguments: String,
        },
        /// A tool call completed during the turn.
        ToolResult {
            call_id: String,
            success: bool,
            result: String,
        },
        /// Runtime progress inside the turn.
        Progress { message: String },
        /// The runtime reported an error for this turn.
        Error { message: String },
        /// The user canceled or interrupted this turn.
        Canceled,
    }

    /// A single user-visible event in a chat turn.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    #[non_exhaustive]
    pub struct ChatTurnEvent {
        /// Unique event identifier.
        #[serde(default = "new_message_id")]
        pub id: String,
        /// Optional chat message ID represented by this turn event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub message_id: Option<String>,
        /// Unix timestamp in milliseconds when the event was recorded.
        pub timestamp: i64,
        /// Event payload.
        pub kind: ChatTurnEventKind,
    }

    impl ChatTurnEvent {
        /// Create a new turn event with the current timestamp.
        pub fn new(kind: ChatTurnEventKind) -> Self {
            Self {
                id: new_message_id(),
                message_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                kind,
            }
        }
    }

    /// A single user turn containing ordered UI/runtime events.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
    pub struct ChatTurn {
        /// Stable turn identifier. Streaming IPC uses the stream id as the turn id.
        pub id: String,
        /// Current lifecycle state.
        pub status: ChatTurnStatus,
        /// Unix timestamp in milliseconds when the turn started.
        pub started_at: i64,
        /// Unix timestamp in milliseconds when the turn was last updated.
        pub updated_at: i64,
        /// Unix timestamp in milliseconds when the turn ended.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub completed_at: Option<i64>,
        /// Ordered user-visible runtime events for this turn.
        #[serde(default)]
        pub events: Vec<ChatTurnEvent>,
    }

    fn new_message_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    impl ChatMessage {
        /// Create a new user message.
        pub fn user(content: impl Into<String>) -> Self {
            Self {
                id: new_message_id(),
                role: ChatRole::User,
                content: content.into(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                execution: None,
                media: None,
                transcript: None,
            }
        }

        /// Create a new assistant message.
        pub fn assistant(content: impl Into<String>) -> Self {
            Self {
                id: new_message_id(),
                role: ChatRole::Assistant,
                content: content.into(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                execution: None,
                media: None,
                transcript: None,
            }
        }

        /// Create a new system message.
        pub fn system(content: impl Into<String>) -> Self {
            Self {
                id: new_message_id(),
                role: ChatRole::System,
                content: content.into(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                execution: None,
                media: None,
                transcript: None,
            }
        }

        /// Add execution details to an assistant message.
        pub fn with_execution(mut self, execution: MessageExecution) -> Self {
            self.execution = Some(execution);
            self
        }

        /// Attach structured media metadata.
        pub fn with_media(mut self, media: ChatMessageMedia) -> Self {
            self.media = Some(media);
            self
        }

        /// Attach structured transcript metadata.
        pub fn with_transcript(mut self, transcript: ChatMessageTranscript) -> Self {
            self.transcript = Some(transcript);
            self
        }
    }

    /// Metadata for a chat session.
    ///
    /// Tracks aggregate statistics about the session.
    #[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
    pub struct ChatSessionMetadata {
        /// Total tokens used across all messages
        pub total_tokens: u32,
        /// Number of messages in the session
        pub message_count: u32,
    }

    impl ChatSessionMetadata {
        /// Create new empty metadata.
        pub fn new() -> Self {
            Self::default()
        }

        /// Update metadata after adding a message.
        pub fn update(&mut self, tokens: u32) {
            self.total_tokens += tokens;
            self.message_count += 1;
        }
    }

    /// A chat session containing conversation history with an agent.
    ///
    /// Sessions persist conversations across application restarts and can be
    /// associated with specific skills for context-aware interactions.
    ///
    /// # Example
    ///
    /// ```rust
    /// use types::{ChatMessage, ChatSession};
    ///
    /// let mut session = ChatSession::new(
    ///     "research-agent".to_string(),
    ///     "claude-sonnet-4-20250514".to_string(),
    /// );
    ///
    /// session.add_message(ChatMessage::user("Hello!"));
    /// session.add_message(ChatMessage::assistant("Hi there! How can I help?"));
    /// ```
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
    pub struct ChatSession {
        /// Unique identifier for this session
        pub id: String,
        /// Human-readable session name
        pub name: String,
        /// ID of the agent this session is with
        pub agent_id: String,
        /// Current provider for this session.
        ///
        /// Kept as a string for now because session JSON is a user-facing persistence
        /// format. Prefer typed accessors at call sites before changing this field.
        #[serde(default)]
        pub provider: String,
        /// Current model for this session
        pub model: String,
        /// Ordered list of messages in the conversation
        pub messages: Vec<ChatMessage>,
        /// Ordered turn/event history used by terminal UI projection and replay.
        #[serde(default)]
        pub turns: Vec<ChatTurn>,
        /// Unix timestamp in milliseconds when the session was created
        pub created_at: i64,
        /// Unix timestamp in milliseconds when the session was last updated
        pub updated_at: i64,
        /// Optional skill ID for context-aware sessions
        #[serde(skip_serializing_if = "Option::is_none")]
        pub skill_id: Option<String>,
        /// Optional per-session retention policy (e.g., "1h", "1d", "7d", "30d")
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub retention: Option<String>,
        /// Summary message pointer for compacted sessions
        #[serde(default)]
        pub summary_message_id: Option<String>,
        /// Cumulative prompt tokens used in this session
        #[serde(default)]
        pub prompt_tokens: i64,
        /// Cumulative completion tokens used in this session
        #[serde(default)]
        pub completion_tokens: i64,
        /// Total cost accumulated for this session (including compaction)
        #[serde(default)]
        pub cost: f64,
        /// Session metadata (tokens, message count, etc.)
        pub metadata: ChatSessionMetadata,
        /// Unix timestamp in milliseconds when the session was archived.
        /// None means the session is active.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub archived_at: Option<i64>,
    }

    /// Partial update payload for a chat session.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, Default, PartialEq)]
    pub struct ChatSessionUpdate {
        pub agent_id: Option<String>,
        pub model: Option<String>,
        pub name: Option<String>,
    }

    impl ChatSession {
        pub fn resolve_model_identity(model: &str) -> (String, String) {
            if let Ok(model_ref) = ModelRef::from_legacy_providerless_user_input(model) {
                return (
                    model_ref.provider.as_canonical_str().to_string(),
                    model_ref.model.as_serialized_str().to_string(),
                );
            }
            (String::new(), model.trim().to_string())
        }

        /// Create a new chat session with the given agent and model.
        pub fn new(agent_id: String, model: String) -> Self {
            let now = chrono::Utc::now().timestamp_millis();
            let (provider, model) = Self::resolve_model_identity(&model);
            Self {
                id: uuid::Uuid::new_v4().to_string(),
                name: "New Chat".to_string(),
                agent_id,
                provider,
                model,
                messages: Vec::new(),
                turns: Vec::new(),
                created_at: now,
                updated_at: now,
                skill_id: None,
                retention: None,
                summary_message_id: None,
                prompt_tokens: 0,
                completion_tokens: 0,
                cost: 0.0,
                metadata: ChatSessionMetadata::new(),
                archived_at: None,
            }
        }

        pub fn set_model_identity(&mut self, model: ModelId) {
            self.provider = model.provider().as_canonical_str().to_string();
            self.model = model.as_serialized_str().to_string();
            self.updated_at = chrono::Utc::now().timestamp_millis();
        }

        pub fn set_model_identity_from_raw(&mut self, model: &str) {
            let (provider, normalized_model) = Self::resolve_model_identity(model);
            self.provider = provider;
            self.model = normalized_model;
            self.updated_at = chrono::Utc::now().timestamp_millis();
        }

        /// Create a new chat session with a custom name.
        pub fn with_name(mut self, name: impl Into<String>) -> Self {
            self.name = name.into();
            self
        }

        /// Associate the session with a skill.
        pub fn with_skill(mut self, skill_id: impl Into<String>) -> Self {
            self.skill_id = Some(skill_id.into());
            self
        }

        /// Set an optional retention policy for this session.
        pub fn with_retention(mut self, retention: impl Into<String>) -> Self {
            self.retention = Some(retention.into());
            self
        }

        /// Maximum messages stored per session to prevent unbounded DB growth.
        const MAX_STORED_MESSAGES: usize = 200;

        /// Add a message to the session.
        pub fn add_message(&mut self, message: ChatMessage) {
            // Update metadata
            if let Some(ref exec) = message.execution {
                self.metadata.update(exec.tokens_used);
            } else {
                self.metadata.message_count += 1;
            }

            self.messages.push(message);

            // Prevent unbounded growth in long-running sessions.
            if self.messages.len() > Self::MAX_STORED_MESSAGES {
                let excess = self.messages.len() - Self::MAX_STORED_MESSAGES;
                self.messages.drain(..excess);
                self.metadata.message_count = self.messages.len() as u32;
                self.prune_turn_history_to_retained_messages();
            }

            self.updated_at = chrono::Utc::now().timestamp_millis();
        }

        fn prune_turn_history_to_retained_messages(&mut self) {
            let retained_message_ids = self
                .messages
                .iter()
                .map(|message| message.id.clone())
                .collect::<HashSet<_>>();
            if self
                .summary_message_id
                .as_ref()
                .is_some_and(|summary_id| !retained_message_ids.contains(summary_id))
            {
                self.summary_message_id = None;
            }
            let oldest_retained_timestamp = self
                .messages
                .iter()
                .map(|message| message.timestamp)
                .min()
                .unwrap_or_default();
            for turn in &mut self.turns {
                turn.events.retain(|event| {
                    event
                        .message_id
                        .as_ref()
                        .is_some_and(|message_id| retained_message_ids.contains(message_id))
                        || event.timestamp > oldest_retained_timestamp
                });
            }
            self.turns.retain(|turn| !turn.events.is_empty());
        }

        fn ensure_turn_index(&mut self, turn_id: &str) -> usize {
            if let Some(index) = self.turns.iter().position(|turn| turn.id == turn_id) {
                return index;
            }

            let now = chrono::Utc::now().timestamp_millis();
            self.turns.push(ChatTurn {
                id: turn_id.to_string(),
                status: ChatTurnStatus::Running,
                started_at: now,
                updated_at: now,
                completed_at: None,
                events: Vec::new(),
            });
            self.turns.len() - 1
        }

        /// Start or update a turn with the user input that drives it.
        pub fn record_turn_user_message(&mut self, turn_id: &str, content: impl Into<String>) {
            let content = content.into();
            let index = self.ensure_turn_index(turn_id);
            let message_id = self
                .messages
                .iter()
                .rev()
                .find(|message| message.role == ChatRole::User && message.content == content)
                .map(|message| message.id.clone());
            let turn = &mut self.turns[index];
            if turn.status.is_terminal() {
                return;
            }
            turn.status = ChatTurnStatus::Running;
            turn.completed_at = None;
            if let Some(event) = turn.events.iter_mut().find(|event| {
                matches!(
                    &event.kind,
                    ChatTurnEventKind::UserMessage { content: existing } if existing == &content
                )
            }) {
                if event.message_id.is_none() {
                    event.message_id = message_id;
                }
            } else {
                let mut event = ChatTurnEvent::new(ChatTurnEventKind::UserMessage { content });
                event.message_id = message_id;
                turn.events.push(event);
            }
            let now = chrono::Utc::now().timestamp_millis();
            turn.updated_at = now;
            self.updated_at = now;
        }

        /// Append a distinct user update event to a turn.
        ///
        /// Unlike `record_turn_user_message`, this never deduplicates by content.
        /// Use it for steering updates where repeated text is still a distinct
        /// user action that must be visible in the session log.
        pub fn append_turn_user_message(&mut self, turn_id: &str, content: impl Into<String>) {
            let content = content.into();
            let index = self.ensure_turn_index(turn_id);
            let message_id = self
                .messages
                .iter()
                .rev()
                .find(|message| message.role == ChatRole::User && message.content == content)
                .map(|message| message.id.clone());
            let turn = &mut self.turns[index];
            if turn.status.is_terminal() {
                return;
            }
            turn.status = ChatTurnStatus::Running;
            turn.completed_at = None;
            let mut event = ChatTurnEvent::new(ChatTurnEventKind::UserMessage { content });
            event.message_id = message_id;
            turn.events.push(event);
            let now = chrono::Utc::now().timestamp_millis();
            turn.updated_at = now;
            self.updated_at = now;
        }

        /// Append an ordered event to a turn.
        pub fn record_turn_event(&mut self, turn_id: &str, kind: ChatTurnEventKind) {
            let index = self.ensure_turn_index(turn_id);
            let turn = &mut self.turns[index];
            if turn.status.is_terminal() {
                return;
            }
            turn.events.push(ChatTurnEvent::new(kind));
            let now = chrono::Utc::now().timestamp_millis();
            turn.updated_at = now;
            self.updated_at = now;
        }

        /// Mark a turn as completed and persist its assistant output.
        ///
        /// Uses trimmed comparison when matching against existing turn events and
        /// legacy messages so that whitespace differences (e.g. a leading `"\n\n"`)
        /// from streaming do not prevent deduplication.
        pub fn complete_turn_with_assistant_message(
            &mut self,
            turn_id: &str,
            content: impl Into<String>,
        ) {
            let content = content.into();
            let index = self.ensure_turn_index(turn_id);
            let message_id = self
                .messages
                .iter()
                .rev()
                .find(|message| {
                    message.role == ChatRole::Assistant
                        && message.content.trim() == content.trim()
                })
                .map(|message| message.id.clone());
            let turn = &mut self.turns[index];
            if turn.status.is_terminal() {
                return;
            }
            if !content.trim().is_empty()
                && let Some(event) = turn.events.iter_mut().find(|event| {
                    matches!(
                        &event.kind,
                        ChatTurnEventKind::AssistantMessage { content: existing }
                            if existing.trim() == content.trim()
                    )
                })
            {
                if event.message_id.is_none() {
                    event.message_id = message_id;
                }
                // Normalize the stored content so downstream dedup sees a
                // canonical form.
                event.kind = ChatTurnEventKind::AssistantMessage {
                    content: content.trim().to_string(),
                };
            } else if !content.trim().is_empty() {
                let mut event = ChatTurnEvent::new(ChatTurnEventKind::AssistantMessage {
                    content: content.trim().to_string(),
                });
                event.message_id = message_id;
                turn.events.push(event);
            }
            let now = chrono::Utc::now().timestamp_millis();
            turn.status = ChatTurnStatus::Completed;
            turn.updated_at = now;
            turn.completed_at = Some(now);
            self.updated_at = now;
        }

        /// Mark a turn as completed without appending another assistant event.
        ///
        /// Use this when assistant deltas were already persisted as ordered
        /// turn events during streaming.
        pub fn complete_turn(&mut self, turn_id: &str) {
            let index = self.ensure_turn_index(turn_id);
            let turn = &mut self.turns[index];
            if turn.status.is_terminal() {
                return;
            }
            let now = chrono::Utc::now().timestamp_millis();
            turn.status = ChatTurnStatus::Completed;
            turn.updated_at = now;
            turn.completed_at = Some(now);
            self.updated_at = now;
        }

        /// Mark a turn as failed and persist the error message.
        pub fn fail_turn(&mut self, turn_id: &str, message: impl Into<String>) {
            let message = message.into();
            let index = self.ensure_turn_index(turn_id);
            let turn = &mut self.turns[index];
            if turn.status.is_terminal() {
                return;
            }
            if !message.trim().is_empty() {
                turn.events
                    .push(ChatTurnEvent::new(ChatTurnEventKind::Error { message }));
            }
            let now = chrono::Utc::now().timestamp_millis();
            turn.status = ChatTurnStatus::Failed;
            turn.updated_at = now;
            turn.completed_at = Some(now);
            self.updated_at = now;
        }

        /// Mark a turn as canceled.
        pub fn cancel_turn(&mut self, turn_id: &str) {
            let index = self.ensure_turn_index(turn_id);
            let turn = &mut self.turns[index];
            if turn.status.is_terminal() {
                return;
            }
            let already_recorded = turn
                .events
                .iter()
                .any(|event| matches!(event.kind, ChatTurnEventKind::Canceled));
            if !already_recorded {
                turn.events
                    .push(ChatTurnEvent::new(ChatTurnEventKind::Canceled));
            }
            let now = chrono::Utc::now().timestamp_millis();
            turn.status = ChatTurnStatus::Canceled;
            turn.updated_at = now;
            turn.completed_at = Some(now);
            self.updated_at = now;
        }

        /// Rename the session.
        pub fn rename(&mut self, name: impl Into<String>) {
            self.name = name.into();
            self.updated_at = chrono::Utc::now().timestamp_millis();
        }

        /// Generate a session name from the first user message.
        ///
        /// Truncates to 30 characters with ellipsis if needed.
        pub fn auto_name_from_first_message(&mut self) {
            if let Some(msg) = self.messages.iter().find(|m| m.role == ChatRole::User) {
                let name: String = msg.content.chars().take(30).collect();
                self.name = if msg.content.chars().count() > 30 {
                    format!("{}...", name)
                } else {
                    name
                };
                self.updated_at = chrono::Utc::now().timestamp_millis();
            }
        }

        /// Get the last N messages from the session.
        pub fn last_messages(&self, n: usize) -> &[ChatMessage] {
            let start = self.messages.len().saturating_sub(n);
            &self.messages[start..]
        }

        /// Mark this session as archived.
        pub fn archive(&mut self) {
            let now = chrono::Utc::now().timestamp_millis();
            self.archived_at = Some(now);
            self.updated_at = now;
        }

        /// Mark this session as active.
        pub fn unarchive(&mut self) {
            self.archived_at = None;
            self.updated_at = chrono::Utc::now().timestamp_millis();
        }

        /// Whether this session is archived.
        pub fn is_archived(&self) -> bool {
            self.archived_at.is_some()
        }
    }

    /// Summary view of a chat session (for listing).
    #[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
    pub struct ChatSessionSummary {
        /// Session ID
        pub id: String,
        /// Session name
        pub name: String,
        /// Agent ID
        pub agent_id: String,
        /// Provider used
        pub provider: String,
        /// Model used
        pub model: String,
        /// Optional skill ID for context-aware sessions
        #[serde(skip_serializing_if = "Option::is_none")]
        pub skill_id: Option<String>,
        /// Number of messages
        pub message_count: u32,
        /// Last update timestamp
        pub updated_at: i64,
        /// Preview of last message (truncated)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub last_message_preview: Option<String>,
        /// Unix timestamp in milliseconds when the session was archived.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub archived_at: Option<i64>,
    }

    impl From<&ChatSession> for ChatSessionSummary {
        fn from(session: &ChatSession) -> Self {
            let last_message_preview = session.messages.last().map(|m| {
                let preview: String = m.content.chars().take(50).collect();
                if m.content.chars().count() > 50 {
                    format!("{}...", preview)
                } else {
                    preview
                }
            });

            Self {
                id: session.id.clone(),
                name: session.name.clone(),
                agent_id: session.agent_id.clone(),
                provider: session.provider.clone(),
                model: session.model.clone(),
                skill_id: session.skill_id.clone(),
                message_count: session.metadata.message_count,
                updated_at: session.updated_at,
                last_message_preview,
                archived_at: session.archived_at,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_chat_role_default() {
            assert_eq!(ChatRole::default(), ChatRole::User);
        }

        #[test]
        fn test_execution_status_default() {
            assert_eq!(ChatExecutionStatus::default(), ChatExecutionStatus::Running);
        }

        #[test]
        fn records_turn_events_without_adding_model_context_messages() {
            let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());

            session.record_turn_user_message("turn-1", "hello");
            session.record_turn_event(
                "turn-1",
                ChatTurnEventKind::ToolCall {
                    call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    arguments: "{\"cmd\":\"pwd\"}".to_string(),
                },
            );
            session.record_turn_event(
                "turn-1",
                ChatTurnEventKind::ToolResult {
                    call_id: "call-1".to_string(),
                    success: true,
                    result: "/tmp".to_string(),
                },
            );
            session.complete_turn_with_assistant_message("turn-1", "done");

            assert!(session.messages.is_empty());
            assert_eq!(session.turns.len(), 1);
            assert_eq!(session.turns[0].status, ChatTurnStatus::Completed);
            assert_eq!(session.turns[0].events.len(), 4);
            assert!(session.turns[0].completed_at.is_some());
        }

        #[test]
        fn canceled_turn_status_is_not_overwritten_by_late_completion() {
            let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());

            session.record_turn_user_message("turn-1", "hello");
            session.cancel_turn("turn-1");
            session.complete_turn_with_assistant_message("turn-1", "late answer");

            assert_eq!(session.turns[0].status, ChatTurnStatus::Canceled);
            assert!(
                session.turns[0]
                    .events
                    .iter()
                    .any(|event| matches!(event.kind, ChatTurnEventKind::Canceled))
            );
            assert!(!session.turns[0].events.iter().any(|event| {
                matches!(
                    &event.kind,
                    ChatTurnEventKind::AssistantMessage { content }
                        if content == "late answer"
                )
            }));
        }

        #[test]
        fn completed_turn_status_is_not_mutated_by_late_completion() {
            let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());

            session.record_turn_user_message("turn-1", "hello");
            session.complete_turn_with_assistant_message("turn-1", "done");
            let completed_at = session.turns[0].completed_at;
            session.complete_turn_with_assistant_message("turn-1", "late answer");

            assert_eq!(session.turns[0].status, ChatTurnStatus::Completed);
            assert_eq!(session.turns[0].completed_at, completed_at);
            assert!(!session.turns[0].events.iter().any(|event| {
                matches!(
                    &event.kind,
                    ChatTurnEventKind::AssistantMessage { content }
                        if content == "late answer"
                )
            }));
        }

        #[test]
        fn complete_turn_marks_status_without_adding_assistant_event() {
            let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());

            session.record_turn_user_message("turn-1", "hello");
            session.record_turn_event(
                "turn-1",
                ChatTurnEventKind::AssistantMessage {
                    content: "streamed".to_string(),
                },
            );
            session.complete_turn("turn-1");

            assert_eq!(session.turns[0].status, ChatTurnStatus::Completed);
            assert_eq!(session.turns[0].events.len(), 2);
            assert!(matches!(
                &session.turns[0].events[1].kind,
                ChatTurnEventKind::AssistantMessage { content } if content == "streamed"
            ));
        }

        #[test]
        fn turn_events_backfill_message_ids_when_legacy_messages_arrive_later() {
            let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());

            session.record_turn_user_message("turn-1", "hello");
            session.record_turn_event(
                "turn-1",
                ChatTurnEventKind::AssistantMessage {
                    content: "done".to_string(),
                },
            );
            assert!(
                session.turns[0]
                    .events
                    .iter()
                    .all(|event| event.message_id.is_none())
            );

            let user = ChatMessage::user("hello");
            let user_id = user.id.clone();
            let assistant = ChatMessage::assistant("done");
            let assistant_id = assistant.id.clone();
            session.add_message(user);
            session.add_message(assistant);

            session.record_turn_user_message("turn-1", "hello");
            session.complete_turn_with_assistant_message("turn-1", "done");

            let user_event = session.turns[0]
                .events
                .iter()
                .find(|event| matches!(event.kind, ChatTurnEventKind::UserMessage { .. }))
                .expect("user event");
            let assistant_event = session.turns[0]
                .events
                .iter()
                .find(|event| matches!(event.kind, ChatTurnEventKind::AssistantMessage { .. }))
                .expect("assistant event");

            assert_eq!(user_event.message_id.as_deref(), Some(user_id.as_str()));
            assert_eq!(
                assistant_event.message_id.as_deref(),
                Some(assistant_id.as_str())
            );
            assert_eq!(session.turns[0].events.len(), 2);
        }

        #[test]
        fn test_execution_step_info_new() {
            let step = ExecutionStepInfo::new("tool_call", "Search files");
            assert_eq!(step.step_type, "tool_call");
            assert_eq!(step.name, "Search files");
            assert_eq!(step.status, "running");
            assert!(step.duration_ms.is_none());
        }

        #[test]
        fn test_execution_step_info_with_status_and_duration() {
            let step = ExecutionStepInfo::new("api_call", "Call LLM")
                .with_status("completed")
                .with_duration(150);
            assert_eq!(step.status, "completed");
            assert_eq!(step.duration_ms, Some(150));
        }

        #[test]
        fn test_message_execution_complete() {
            let mut exec = MessageExecution::new();
            exec.add_step(ExecutionStepInfo::new("thinking", "Planning"));
            let exec = exec.complete(1500, 250);

            assert_eq!(exec.status, ChatExecutionStatus::Completed);
            assert_eq!(exec.duration_ms, 1500);
            assert_eq!(exec.tokens_used, 250);
            assert_eq!(exec.steps.len(), 1);
        }

        #[test]
        fn test_message_execution_fail() {
            let exec = MessageExecution::new().fail(500);
            assert_eq!(exec.status, ChatExecutionStatus::Failed);
            assert_eq!(exec.duration_ms, 500);
        }

        #[test]
        fn test_chat_message_user() {
            let msg = ChatMessage::user("Hello!");
            assert_eq!(msg.role, ChatRole::User);
            assert_eq!(msg.content, "Hello!");
            assert!(msg.execution.is_none());
            assert!(msg.media.is_none());
            assert!(msg.transcript.is_none());
        }

        #[test]
        fn test_chat_message_assistant() {
            let msg = ChatMessage::assistant("Hi there!");
            assert_eq!(msg.role, ChatRole::Assistant);
            assert_eq!(msg.content, "Hi there!");
        }

        #[test]
        fn test_chat_message_system() {
            let msg = ChatMessage::system("You are a helpful assistant.");
            assert_eq!(msg.role, ChatRole::System);
        }

        #[test]
        fn test_chat_message_with_execution() {
            let exec = MessageExecution::new().complete(1000, 100);
            let msg = ChatMessage::assistant("Done!").with_execution(exec);
            assert!(msg.execution.is_some());
            assert_eq!(msg.execution.unwrap().tokens_used, 100);
        }

        #[test]
        fn test_chat_message_with_media_and_transcript() {
            let msg = ChatMessage::user("[Voice message]")
                .with_media(ChatMessageMedia::voice("/tmp/voice.webm", Some(8)))
                .with_transcript(ChatMessageTranscript::new(
                    "hello",
                    Some("whisper-1".to_string()),
                ));
            assert!(msg.media.is_some());
            assert!(msg.transcript.is_some());
            assert_eq!(
                msg.media.as_ref().map(|m| m.media_type),
                Some(ChatMediaType::Voice)
            );
            assert_eq!(
                msg.transcript.as_ref().map(|t| t.text.as_str()),
                Some("hello")
            );
        }

        #[test]
        fn test_chat_session_new() {
            let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
            assert!(!session.id.is_empty());
            assert_eq!(session.name, "New Chat");
            assert_eq!(session.agent_id, "agent-1");
            assert_eq!(session.provider, "anthropic");
            assert_eq!(session.model, "claude-sonnet-4-5");
            assert!(session.messages.is_empty());
            assert!(session.skill_id.is_none());
        }

        #[test]
        fn test_chat_session_with_name() {
            let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
                .with_name("My Coding Session");
            assert_eq!(session.name, "My Coding Session");
        }

        #[test]
        fn test_chat_session_with_skill() {
            let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
                .with_skill("skill-123");
            assert_eq!(session.skill_id, Some("skill-123".to_string()));
        }

        #[test]
        fn test_chat_session_with_retention() {
            let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
                .with_retention("7d");
            assert_eq!(session.retention, Some("7d".to_string()));
        }

        #[test]
        fn test_chat_session_add_message() {
            let mut session =
                ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
            let initial_updated = session.updated_at;

            // Small delay to ensure timestamp changes
            std::thread::sleep(std::time::Duration::from_millis(1));

            session.add_message(ChatMessage::user("Hello!"));
            assert_eq!(session.messages.len(), 1);
            assert_eq!(session.metadata.message_count, 1);
            assert!(session.updated_at >= initial_updated);
        }

        #[test]
        fn test_chat_session_rename() {
            let mut session =
                ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
            session.rename("Renamed Session");
            assert_eq!(session.name, "Renamed Session");
        }

        #[test]
        fn test_chat_session_archive_and_unarchive() {
            let mut session =
                ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
            assert!(!session.is_archived());
            assert!(session.archived_at.is_none());

            session.archive();
            assert!(session.is_archived());
            assert!(session.archived_at.is_some());

            session.unarchive();
            assert!(!session.is_archived());
            assert!(session.archived_at.is_none());
        }

        #[test]
        fn test_chat_session_auto_name_short() {
            let mut session =
                ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
            session.add_message(ChatMessage::user("Help me debug"));
            session.auto_name_from_first_message();
            assert_eq!(session.name, "Help me debug");
        }

        #[test]
        fn test_chat_session_auto_name_long() {
            let mut session =
                ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
            session.add_message(ChatMessage::user(
                "This is a very long message that should be truncated to thirty characters",
            ));
            session.auto_name_from_first_message();
            assert!(session.name.ends_with("..."));
            assert!(session.name.len() <= 33); // 30 chars + "..."
        }

        #[test]
        fn test_chat_session_last_messages() {
            let mut session =
                ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
            session.add_message(ChatMessage::user("Message 1"));
            session.add_message(ChatMessage::assistant("Response 1"));
            session.add_message(ChatMessage::user("Message 2"));
            session.add_message(ChatMessage::assistant("Response 2"));

            let last_two = session.last_messages(2);
            assert_eq!(last_two.len(), 2);
            assert_eq!(last_two[0].content, "Message 2");
            assert_eq!(last_two[1].content, "Response 2");
        }

        #[test]
        fn test_chat_session_summary_from() {
            let mut session =
                ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
                    .with_name("Test Session");
            session.add_message(ChatMessage::user("Hello!"));
            session.archive();

            let summary = ChatSessionSummary::from(&session);
            assert_eq!(summary.id, session.id);
            assert_eq!(summary.name, "Test Session");
            assert_eq!(summary.agent_id, "agent-1");
            assert_eq!(summary.provider, session.provider);
            assert_eq!(summary.model, session.model);
            assert_eq!(summary.message_count, 1);
            assert_eq!(summary.last_message_preview, Some("Hello!".to_string()));
            assert!(summary.archived_at.is_some());
        }

        #[test]
        fn test_chat_session_summary_truncates_preview() {
            let mut session =
                ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
            session.add_message(ChatMessage::user(
                "This is a very long message that exceeds fifty characters and should be truncated",
            ));

            let summary = ChatSessionSummary::from(&session);
            assert!(summary.last_message_preview.unwrap().ends_with("..."));
        }

        #[test]
        fn test_chat_session_metadata_update() {
            let mut metadata = ChatSessionMetadata::new();
            metadata.update(100);

            assert_eq!(metadata.total_tokens, 100);
            assert_eq!(metadata.message_count, 1);
        }

        #[test]
        fn test_chat_session_resolves_provider_from_raw_model() {
            let (provider, model) = ChatSession::resolve_model_identity("minimax:MiniMax-M2.5");

            assert_eq!(provider, "minimax");
            assert_eq!(model, "minimax-m2-5");
        }

        #[test]
        fn test_chat_session_preserves_legacy_providerless_api_model() {
            let (provider, model) = ChatSession::resolve_model_identity("gpt-5.5");

            assert_eq!(provider, "openai");
            assert_eq!(model, "gpt-5-5");
        }

        #[test]
        fn test_chat_session_new_sets_model_identity() {
            let session = ChatSession::new("agent-1".to_string(), "openai:gpt-5".to_string());

            assert_eq!(session.provider, "openai");
            assert_eq!(session.model, "gpt-5");
        }

        // TypeScript binding export tests
        #[test]
        fn test_add_message_enforces_max_stored_limit() {
            let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let total = ChatSession::MAX_STORED_MESSAGES + 10;

            for i in 0..total {
                if i % 2 == 0 {
                    session.add_message(ChatMessage::user(format!("msg {}", i)));
                } else {
                    session.add_message(ChatMessage::assistant(format!("reply {}", i)));
                }
            }

            assert_eq!(session.messages.len(), ChatSession::MAX_STORED_MESSAGES);

            // Most recent message should be retained
            let last = session.messages.last().unwrap();
            assert!(last.content.contains(&(total - 1).to_string()));
        }

        #[test]
        fn test_add_message_below_cap_is_unaffected() {
            let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
            session.add_message(ChatMessage::user("hello"));
            session.add_message(ChatMessage::assistant("hi"));
            assert_eq!(session.messages.len(), 2);
        }

        #[test]
        fn test_message_count_matches_after_drain() {
            let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let total = ChatSession::MAX_STORED_MESSAGES + 50;

            for i in 0..total {
                session.add_message(ChatMessage::user(format!("msg {}", i)));
            }

            assert_eq!(session.messages.len(), ChatSession::MAX_STORED_MESSAGES);
            assert_eq!(
                session.metadata.message_count,
                session.messages.len() as u32
            );
        }

        #[test]
        fn message_cap_prunes_stale_turn_events_and_summary_pointer() {
            let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
            session.record_turn_user_message("old-turn", "old");
            session.complete_turn_with_assistant_message("old-turn", "old done");
            for event in &mut session.turns[0].events {
                event.timestamp = 1;
            }

            let summary = ChatMessage::assistant("summary");
            let summary_id = summary.id.clone();
            session.add_message(summary);
            session.summary_message_id = Some(summary_id);

            for i in 0..ChatSession::MAX_STORED_MESSAGES {
                let mut message = ChatMessage::user(format!("msg {i}"));
                message.timestamp = 10 + i as i64;
                session.add_message(message);
            }

            assert_eq!(session.messages.len(), ChatSession::MAX_STORED_MESSAGES);
            assert!(session.summary_message_id.is_none());
            assert!(session.turns.iter().all(|turn| turn.id != "old-turn"));
        }

        #[test]
        fn message_cap_prunes_unmatched_turn_events_at_retention_timestamp_boundary() {
            let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let mut retained_message_id = String::new();
            for i in 0..ChatSession::MAX_STORED_MESSAGES {
                let mut message = ChatMessage::user(format!("msg {i}"));
                message.timestamp = 10 + i as i64;
                if i == 1 {
                    retained_message_id = message.id.clone();
                }
                session.add_message(message);
            }

            session.record_turn_event(
                "stale-turn",
                ChatTurnEventKind::AssistantMessage {
                    content: "stale".to_string(),
                },
            );
            session.turns[0].events[0].timestamp = 11;
            session.record_turn_event(
                "retained-turn",
                ChatTurnEventKind::UserMessage {
                    content: "msg 1".to_string(),
                },
            );
            let retained_turn = session
                .turns
                .iter_mut()
                .find(|turn| turn.id == "retained-turn")
                .expect("retained turn");
            retained_turn.events[0].timestamp = 11;
            retained_turn.events[0].message_id = Some(retained_message_id);
            session.add_message(ChatMessage::assistant("push over cap"));

            assert_eq!(session.messages.len(), ChatSession::MAX_STORED_MESSAGES);
            assert!(session.turns.iter().all(|turn| turn.id != "stale-turn"));
            assert!(session.turns.iter().any(|turn| turn.id == "retained-turn"));
        }
    }
}

pub mod skill {
    //! Skill model types and provider trait.

    use serde::{Deserialize, Deserializer, Serialize};
    use specta::Type;

    /// Skill source used by the unified skill catalog.
    #[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default, Type)]
    #[serde(rename_all = "snake_case")]
    pub enum SkillSource {
        /// Shipped with RestFlow and read-only.
        System,
        /// Created or imported by the user.
        #[default]
        User,
        /// Installed from a remote/package/binary source.
        External,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    pub struct SkillScript {
        pub id: String,
        pub path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub lang: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    pub struct SkillReference {
        pub id: String,
        pub path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub summary: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    pub struct SkillGating {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub bins: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub env: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub os: Option<Vec<String>>,
    }

    /// Skill lifecycle status used for discovery and planning.
    #[derive(Debug, Clone, Serialize, Deserialize, Type, Default, PartialEq, Eq)]
    #[serde(rename_all = "lowercase")]
    pub enum SkillStatus {
        #[default]
        Active,
        Completed,
        Archived,
        Draft,
    }

    /// A skill represents a reusable AI prompt template.
    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    pub struct Skill {
        pub id: String,
        pub name: String,
        pub description: Option<String>,
        pub tags: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        #[serde(default)]
        pub executable: bool,
        pub content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub folder_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub suggested_tools: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub scripts: Vec<SkillScript>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub references: Vec<SkillReference>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub gating: Option<SkillGating>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub author: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub license: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub content_hash: Option<String>,
        #[serde(default)]
        pub status: SkillStatus,
        #[serde(default)]
        pub auto_complete: bool,
        #[serde(default)]
        pub source: SkillSource,
        #[serde(default)]
        pub read_only: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub source_ref: Option<String>,
        pub created_at: i64,
        pub updated_at: i64,
    }

    impl Skill {
        pub fn new(
            id: String,
            name: String,
            description: Option<String>,
            tags: Option<Vec<String>>,
            content: String,
        ) -> Self {
            let now = chrono::Utc::now().timestamp_millis();
            Self {
                id,
                name,
                description,
                tags,
                kind: None,
                executable: false,
                content,
                folder_path: None,
                suggested_tools: Vec::new(),
                scripts: Vec::new(),
                references: Vec::new(),
                gating: None,
                version: None,
                author: None,
                license: None,
                content_hash: None,
                status: SkillStatus::Active,
                auto_complete: false,
                source: SkillSource::User,
                read_only: false,
                source_ref: None,
                created_at: now,
                updated_at: now,
            }
        }

        pub fn update(
            &mut self,
            name: Option<String>,
            description: Option<Option<String>>,
            tags: Option<Option<Vec<String>>>,
            content: Option<String>,
        ) {
            if let Some(name) = name {
                self.name = name;
            }
            if let Some(description) = description {
                self.description = description;
            }
            if let Some(tags) = tags {
                self.tags = tags;
            }
            if let Some(content) = content {
                self.content = content;
            }
            self.updated_at = chrono::Utc::now().timestamp_millis();
        }

        pub fn to_markdown(&self) -> String {
            let frontmatter = SkillFrontmatter {
                name: self.name.clone(),
                description: self.description.clone(),
                tags: self.tags.clone(),
                suggested_tools: if self.suggested_tools.is_empty() {
                    None
                } else {
                    Some(self.suggested_tools.clone())
                },
                scripts: if self.scripts.is_empty() {
                    None
                } else {
                    Some(self.scripts.clone())
                },
                references: if self.references.is_empty() {
                    None
                } else {
                    Some(self.references.clone())
                },
                gating: self.gating.clone(),
                version: self.version.clone(),
                author: self.author.clone(),
                license: self.license.clone(),
                status: if self.status == SkillStatus::Active {
                    None
                } else {
                    Some(self.status.clone())
                },
                auto_complete: if self.auto_complete { Some(true) } else { None },
            };

            let yaml = serde_yaml::to_string(&frontmatter).unwrap_or_default();
            format!("---\n{}\n---\n\n{}", yaml, self.content)
        }

        pub fn from_markdown(id: &str, markdown: &str) -> anyhow::Result<Self> {
            if !markdown.starts_with("---") {
                anyhow::bail!("Invalid markdown format: missing frontmatter");
            }

            let lines: Vec<&str> = markdown.lines().collect();
            let end_line_offset = lines
                .iter()
                .skip(1)
                .position(|line| line.trim() == "---")
                .map(|index| index + 1)
                .ok_or_else(|| {
                    anyhow::anyhow!("Invalid markdown format: frontmatter not closed")
                })?;

            let frontmatter_lines = &lines[1..end_line_offset];
            let frontmatter_str = frontmatter_lines.join("\n");
            let content_start = lines[..=end_line_offset].join("\n").len() + "\n".len();
            let content = markdown[content_start..].trim().to_string();
            let frontmatter: SkillFrontmatter = serde_yaml::from_str(&frontmatter_str)?;

            let mut skill = Self::new(
                id.to_string(),
                frontmatter.name,
                frontmatter.description,
                frontmatter.tags,
                content,
            );
            skill.suggested_tools = frontmatter.suggested_tools.unwrap_or_default();
            skill.scripts = frontmatter.scripts.unwrap_or_default();
            skill.references = frontmatter.references.unwrap_or_default();
            skill.gating = frontmatter.gating;
            skill.version = frontmatter.version;
            skill.author = frontmatter.author;
            skill.license = frontmatter.license;
            skill.status = frontmatter.status.unwrap_or_default();
            skill.auto_complete = frontmatter.auto_complete.unwrap_or(false);

            Ok(skill)
        }
    }

    impl Default for Skill {
        fn default() -> Self {
            Self::new(String::new(), String::new(), None, None, String::new())
        }
    }

    /// Frontmatter structure for import/export.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SkillFrontmatter {
        pub name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tags: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub suggested_tools: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub scripts: Option<Vec<SkillScript>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub references: Option<Vec<SkillReference>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub gating: Option<SkillGating>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub author: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub license: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub status: Option<SkillStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub auto_complete: Option<bool>,
    }

    /// Skill metadata stored separately from markdown content.
    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    pub struct SkillMeta {
        pub id: String,
        pub name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tags: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub folder_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub content_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub scripts: Vec<SkillScript>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub references: Vec<SkillReference>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub suggested_tools: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub gating: Option<SkillGating>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub author: Option<String>,
        pub created_at: i64,
        pub updated_at: i64,
    }

    impl<'de> Deserialize<'de> for SkillSource {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let value = serde_json::Value::deserialize(deserializer)?;
            let raw_source = match value {
                serde_json::Value::String(source) => source,
                serde_json::Value::Object(mut object) => object
                    .remove("type")
                    .and_then(|value| value.as_str().map(str::to_string))
                    .ok_or_else(|| serde::de::Error::custom("skill source object missing type"))?,
                _ => {
                    return Err(serde::de::Error::custom(
                        "skill source must be a string or object",
                    ));
                }
            };

            match raw_source.as_str() {
                "system" | "builtin" => Ok(Self::System),
                "user" | "local" => Ok(Self::User),
                "external" | "marketplace" | "github" | "git_hub" | "git" => Ok(Self::External),
                other => Err(serde::de::Error::unknown_variant(
                    other,
                    &[
                        "system",
                        "user",
                        "external",
                        "builtin",
                        "local",
                        "marketplace",
                        "github",
                        "git_hub",
                        "git",
                    ],
                )),
            }
        }
    }

    impl std::fmt::Display for SkillSource {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let value = match self {
                Self::System => "system",
                Self::User => "user",
                Self::External => "external",
            };
            f.write_str(value)
        }
    }

    /// Skill info for listing
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SkillInfo {
        pub id: String,
        pub name: String,
        pub description: Option<String>,
        pub tags: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        #[serde(default)]
        pub executable: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub suggested_tools: Vec<String>,
        #[serde(default)]
        pub source: SkillSource,
        #[serde(default)]
        pub read_only: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub source_ref: Option<String>,
    }

    /// Skill content for reading
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SkillContent {
        pub id: String,
        pub name: String,
        pub content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        #[serde(default)]
        pub executable: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub suggested_tools: Vec<String>,
        #[serde(default)]
        pub source: SkillSource,
        #[serde(default)]
        pub read_only: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub source_ref: Option<String>,
    }

    /// Provider trait for accessing skills (implemented in runtime)
    pub trait SkillProvider: Send + Sync {
        /// List all available skills
        fn list_skills(&self) -> Vec<SkillInfo>;
        /// Get skill content by ID
        fn get_skill(&self, id: &str) -> Option<SkillContent>;
        /// Export a skill to markdown
        fn export_skill(&self, id: &str) -> Result<String, String>;
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn system_skill_source_serializes_as_system() {
            let value = serde_json::to_value(SkillSource::System).unwrap();
            assert_eq!(value, serde_json::json!("system"));
        }

        #[test]
        fn legacy_skill_source_shapes_deserialize() {
            let local: SkillSource = serde_json::from_str(r#"{"type":"local"}"#).unwrap();
            let builtin: SkillSource = serde_json::from_str(r#"{"type":"builtin"}"#).unwrap();
            let github: SkillSource = serde_json::from_str(r#"{"type":"git_hub"}"#).unwrap();

            assert_eq!(local, SkillSource::User);
            assert_eq!(builtin, SkillSource::System);
            assert_eq!(github, SkillSource::External);
        }

        #[test]
        fn skill_markdown_round_trips_references() {
            let mut skill = Skill::new(
                "reference-skill".to_string(),
                "Reference Skill".to_string(),
                None,
                None,
                "# Root content".to_string(),
            );
            skill.references = vec![SkillReference {
                id: "ref-1".to_string(),
                path: "references/ref-1.md".to_string(),
                title: Some("Reference One".to_string()),
                summary: Some("One line summary".to_string()),
            }];

            let markdown = skill.to_markdown();
            let parsed = Skill::from_markdown("reference-skill", &markdown).unwrap();

            assert_eq!(parsed.references.len(), 1);
            let reference = &parsed.references[0];
            assert_eq!(reference.id, "ref-1");
            assert_eq!(reference.path, "references/ref-1.md");
            assert_eq!(reference.title.as_deref(), Some("Reference One"));
            assert_eq!(reference.summary.as_deref(), Some("One line summary"));
        }

        #[test]
        fn skill_frontmatter_with_yaml_separator_in_value() {
            let markdown = r#"---
name: Test Skill
description: "Supports --- separator"
tags:
  - test
---

# Content"#;

            let skill = Skill::from_markdown("test", markdown).unwrap();

            assert_eq!(skill.name, "Test Skill");
            assert_eq!(
                skill.description,
                Some("Supports --- separator".to_string())
            );
            assert!(skill.content.contains("# Content"));
        }
    }
}

pub mod steer {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    /// Command payload for a steer message.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum SteerCommand {
        /// Inject a text message into the running conversation.
        Message { instruction: String },
        /// Interrupt execution with a reason visible to the runtime.
        Interrupt {
            reason: String,
            #[serde(default)]
            metadata: Value,
        },
        /// Cancel a specific running tool call by its ID.
        CancelToolCall { tool_call_id: String },
    }

    /// A message injected into a running agent's ReAct loop.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SteerMessage {
        /// The command to execute.
        pub command: SteerCommand,
        pub source: SteerSource,
        pub timestamp: i64,
    }

    impl SteerMessage {
        /// Create a text-injection steer message (backward-compatible helper).
        pub fn message(instruction: impl Into<String>, source: SteerSource) -> Self {
            Self {
                command: SteerCommand::Message {
                    instruction: instruction.into(),
                },
                source,
                timestamp: chrono::Utc::now().timestamp_millis(),
            }
        }

        /// Create an interrupt steer message.
        pub fn interrupt(reason: impl Into<String>, source: SteerSource) -> Self {
            Self {
                command: SteerCommand::Interrupt {
                    reason: reason.into(),
                    metadata: Value::Null,
                },
                source,
                timestamp: chrono::Utc::now().timestamp_millis(),
            }
        }

        /// Backward-compatible accessor for steer command payload text.
        pub fn instruction(&self) -> &str {
            match &self.command {
                SteerCommand::Message { instruction } => instruction,
                SteerCommand::Interrupt { reason, .. } => reason,
                SteerCommand::CancelToolCall { tool_call_id } => tool_call_id,
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum SteerSource {
        /// Direct from UI or CLI.
        User,
        /// From internal system automation.
        System,
        /// From REST/WebSocket API.
        Api,
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn steer_message_constructor_sets_message_payload() {
            let msg = SteerMessage::message("do something", SteerSource::User);
            assert_eq!(msg.instruction(), "do something");
            assert!(matches!(msg.command, SteerCommand::Message { .. }));
        }

        #[test]
        fn steer_interrupt_constructor_sets_interrupt_payload() {
            let msg = SteerMessage::interrupt("approval needed", SteerSource::Api);
            assert_eq!(msg.instruction(), "approval needed");
            assert!(matches!(msg.command, SteerCommand::Interrupt { .. }));
        }

        #[test]
        fn steer_command_serialization_round_trips() {
            let cmd = SteerCommand::Interrupt {
                reason: "test".into(),
                metadata: serde_json::json!({"key": "value"}),
            };
            let json = serde_json::to_string(&cmd).unwrap();
            assert!(json.contains("interrupt"));

            let deserialized: SteerCommand = serde_json::from_str(&json).unwrap();
            assert!(matches!(deserialized, SteerCommand::Interrupt { .. }));
        }

        #[test]
        fn steer_command_message_round_trips() {
            let cmd = SteerCommand::Message {
                instruction: "hello".into(),
            };
            let json = serde_json::to_string(&cmd).unwrap();
            let deserialized: SteerCommand = serde_json::from_str(&json).unwrap();
            match deserialized {
                SteerCommand::Message { instruction } => assert_eq!(instruction, "hello"),
                _ => panic!("Expected Message variant"),
            }
        }
    }
}

pub mod store {
    //! Storage trait abstractions for tools.
    //!
    //! These traits define the storage interfaces that tools require.
    //! Implementations are provided by downstream crates (e.g., runtime).

    use std::future::Future;
    use std::pin::Pin;

    use crate::contracts::request::AgentNode as ContractAgentNode;
    use serde::Deserialize;
    use serde_json::Value;

    use crate::config_types::ConfigDocument;
    use crate::error::Result;

    // ── AgentStore ───────────────────────────────────────────────────────

    #[derive(Clone, Debug, Deserialize)]
    pub struct AgentCreateRequest {
        pub name: String,
        pub agent: ContractAgentNode,
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct AgentUpdateRequest {
        pub id: String,
        #[serde(default)]
        pub name: Option<String>,
        #[serde(default)]
        pub agent: Option<ContractAgentNode>,
    }

    pub trait AgentStore: Send + Sync {
        fn list_agents(&self) -> Result<Value>;
        fn get_agent(&self, id: &str) -> Result<Value>;
        fn create_agent(&self, request: AgentCreateRequest) -> Result<Value>;
        fn update_agent(&self, request: AgentUpdateRequest) -> Result<Value>;
        fn delete_agent(&self, id: &str) -> Result<Value>;
    }

    // ── SessionStore ─────────────────────────────────────────────────────

    #[derive(Clone, Debug, Deserialize)]
    pub struct SessionCreateRequest {
        pub agent_id: String,
        pub model: String,
        #[serde(default)]
        pub name: Option<String>,
        #[serde(default)]
        pub skill_id: Option<String>,
        #[serde(default)]
        pub retention: Option<String>,
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct SessionSearchQuery {
        pub query: String,
        #[serde(default)]
        pub agent_id: Option<String>,
        #[serde(default)]
        pub skill_id: Option<String>,
        #[serde(default)]
        pub include_archived: Option<bool>,
        #[serde(default)]
        pub limit: Option<u32>,
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct SessionListFilter {
        #[serde(default)]
        pub agent_id: Option<String>,
        #[serde(default)]
        pub skill_id: Option<String>,
        #[serde(default)]
        pub include_messages: Option<bool>,
        #[serde(default)]
        pub include_archived: Option<bool>,
    }

    pub trait SessionStore: Send + Sync {
        fn list_sessions(&self, filter: SessionListFilter) -> Result<Value>;
        fn get_session(&self, id: &str) -> Result<Value>;
        fn create_session(&self, request: SessionCreateRequest) -> Result<Value>;
        fn archive_session(&self, id: &str) -> Result<Value>;
        fn unarchive_session(&self, id: &str) -> Result<Value>;
        fn purge_session(&self, id: &str) -> Result<Value>;
        fn delete_session(&self, id: &str) -> Result<Value>;
        fn search_sessions(&self, query: SessionSearchQuery) -> Result<Value>;
        fn cleanup_sessions(&self) -> Result<Value>;
    }

    // ── ReplySender ──────────────────────────────────────────────────────

    pub trait ReplySender: Send + Sync {
        fn send(&self, message: String)
        -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
    }

    // ── SecretStore ──────────────────────────────────────────────────────

    pub trait SecretStore: Send + Sync {
        fn list_secrets(&self) -> Result<Value>;
        fn get_secret(&self, key: &str) -> Result<Option<String>>;
        fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()>;
        fn delete_secret(&self, key: &str) -> Result<()>;
        fn has_secret(&self, key: &str) -> Result<bool>;
    }

    // ── ConfigStore ──────────────────────────────────────────────────────

    pub trait ConfigStore: Send + Sync {
        fn get_effective_config(&self) -> Result<ConfigDocument>;
        fn get_writable_config(&self) -> Result<ConfigDocument>;
        fn persist_config(&self, config: &ConfigDocument) -> Result<()>;
        fn reset_config(&self) -> Result<ConfigDocument>;
    }

    // ── OpsProvider ─────────────────────────────────────────────────────

    pub trait OpsProvider: Send + Sync {
        fn daemon_health(&self) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>>;
        fn log_tail(&self, lines: usize, path: Option<&str>) -> Result<Value>;
    }
}

pub mod subagent {
    //! Sub-agent data types and trait definitions.
    //!
    //! Runtime implementations (SubagentTracker, spawn_subagent) remain in ai.

    use serde::{Deserialize, Serialize};

    pub use crate::contracts::request::RunSpawnRequest as ContractRunSpawnRequest;
    use crate::contracts::request::{
        InlineAgentRunConfig as ContractInlineAgentRunConfig,
        SpawnPriority as ContractSpawnPriority,
    };
    use crate::error::ToolError;
    use crate::{
        DEFAULT_AGENT_MAX_ITERATIONS, DEFAULT_MAX_PARALLEL_SUBAGENTS, DEFAULT_SUBAGENT_MAX_DEPTH,
        DEFAULT_SUBAGENT_TIMEOUT_SECS,
    };
    /// Snapshot of a sub-agent definition with all fields needed for execution.
    ///
    /// This is a simple owned data struct that captures the fields from a concrete
    /// agent definition. It decouples the ai crate from the full
    /// `AgentDefinition` struct (which lives in runtime and carries
    /// extra derives that ai doesn't need).
    #[derive(Debug, Clone)]
    pub struct SubagentDefSnapshot {
        /// Display name
        pub name: String,
        /// System prompt for the agent
        pub system_prompt: String,
        /// Allowed tool names
        pub allowed_tools: Vec<String>,
        /// Maximum ReAct loop iterations
        pub max_iterations: Option<u32>,
        /// Default model for this agent type (from agent definition).
        pub default_model: Option<String>,
    }

    /// Summary info for listing a sub-agent definition.
    #[derive(Debug, Clone, serde::Serialize)]
    pub struct SubagentDefSummary {
        /// Unique identifier
        pub id: String,
        /// Display name
        pub name: String,
        /// Description of when to use this agent
        pub description: String,
        /// Tags for categorization
        pub tags: Vec<String>,
    }

    /// Trait for looking up sub-agent definitions by ID.
    ///
    /// Implemented by `AgentDefinitionRegistry` in runtime so that
    /// ai can spawn sub-agents without depending on runtime.
    pub trait SubagentDefLookup: Send + Sync {
        /// Look up a sub-agent definition by ID, returning a snapshot of the
        /// fields needed for execution.
        fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot>;

        /// List all callable sub-agent definitions (for display/listing purposes).
        fn list_callable(&self) -> Vec<SubagentDefSummary>;
    }

    /// Configuration for sub-agent execution.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SubagentConfig {
        /// Maximum number of parallel sub-agents.
        pub max_parallel_agents: usize,
        /// Default timeout for sub-agents in seconds.
        pub subagent_timeout_secs: u64,
        /// Maximum iterations for sub-agents.
        pub max_iterations: usize,
        /// Maximum nesting depth for sub-agents.
        pub max_depth: usize,
    }

    impl Default for SubagentConfig {
        fn default() -> Self {
            Self {
                max_parallel_agents: DEFAULT_MAX_PARALLEL_SUBAGENTS,
                subagent_timeout_secs: DEFAULT_SUBAGENT_TIMEOUT_SECS,
                max_iterations: DEFAULT_AGENT_MAX_ITERATIONS,
                max_depth: DEFAULT_SUBAGENT_MAX_DEPTH,
            }
        }
    }

    /// Request to spawn a sub-agent.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SpawnRequest {
        /// Agent type ID (e.g., "researcher", "coder").
        ///
        /// When omitted, runtime creates a temporary sub-agent from `inline` config.
        #[serde(default)]
        pub agent_id: Option<String>,

        /// Optional inline configuration for temporary sub-agent creation.
        ///
        /// This is used when `agent_id` is omitted.
        #[serde(default)]
        pub inline: Option<InlineRunConfig>,

        /// Task description for the agent.
        pub task: String,

        /// Optional timeout in seconds.
        pub timeout_secs: Option<u64>,

        /// Optional max iterations override for this spawn.
        #[serde(default)]
        pub max_iterations: Option<u32>,

        /// Optional priority level.
        pub priority: Option<SpawnPriority>,

        /// Optional model override for this spawn (e.g., "minimax/coding-plan").
        #[serde(default)]
        pub model: Option<String>,

        /// Optional provider selector paired with `model` (e.g., "openai-codex").
        ///
        /// When provided, runtime validates that the resolved model belongs to this provider.
        #[serde(default)]
        pub model_provider: Option<String>,

        /// Optional parent run ID used for context propagation.
        #[serde(default)]
        pub parent_run_id: Option<String>,

        /// Optional authoritative run ID for this sub-agent execution.
        ///
        /// When provided, runtime must use this as the canonical sub-agent run ID.
        #[serde(default)]
        pub run_id: Option<String>,
    }

    impl SpawnRequest {
        /// Returns the canonical parent run identifier for this child spawn.
        pub fn parent_run_id(&self) -> Option<&str> {
            self.parent_run_id.as_deref()
        }

        /// Sets the canonical parent run identifier.
        pub fn set_parent_run_id(&mut self, parent_run_id: Option<String>) {
            self.parent_run_id = parent_run_id;
        }
    }

    /// Inline configuration for temporary sub-agent creation.
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct InlineSubagentConfig {
        /// Display name for the temporary sub-agent.
        #[serde(default)]
        pub name: Option<String>,

        /// System prompt override for the temporary sub-agent.
        #[serde(default)]
        pub system_prompt: Option<String>,

        /// Allowed tool names for the temporary sub-agent.
        ///
        /// If omitted, runtime uses all tools currently available to the parent.
        #[serde(default)]
        pub allowed_tools: Option<Vec<String>>,

        /// Optional max iterations override for the temporary sub-agent.
        #[serde(default)]
        pub max_iterations: Option<u32>,
    }

    /// Canonical inline run configuration alias.
    pub type InlineRunConfig = InlineSubagentConfig;

    /// Priority level for sub-agent spawning.
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub enum SpawnPriority {
        Low,
        #[default]
        Normal,
        High,
    }

    /// Source used to determine one effective sub-agent limit.
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum SubagentLimitSource {
        ConfigDefault,
        RequestOverride,
        InlineConfig,
        AgentDefinition,
    }

    /// Effective sub-agent runtime limits resolved at spawn time.
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct SubagentEffectiveLimits {
        /// Effective timeout in seconds.
        pub timeout_secs: u64,
        /// Where the timeout value came from.
        pub timeout_source: SubagentLimitSource,
        /// Effective maximum iterations.
        pub max_iterations: usize,
        /// Where the max_iterations value came from.
        pub max_iterations_source: SubagentLimitSource,
    }

    /// Handle returned after spawning a sub-agent.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SpawnHandle {
        /// Unique task ID.
        pub id: String,

        /// Agent name.
        pub agent_name: String,

        /// Effective runtime limits resolved for this spawn.
        pub effective_limits: SubagentEffectiveLimits,
    }

    /// Sub-agent running state
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SubagentState {
        /// Unique task ID
        pub id: String,

        /// Agent name (e.g., "researcher", "coder")
        pub agent_name: String,

        /// Task description
        pub task: String,

        /// Parent run ID, when spawned from another execution.
        pub parent_run_id: Option<String>,

        /// Current status
        pub status: SubagentStatus,

        /// Start timestamp (Unix ms)
        pub started_at: i64,

        /// Completion timestamp (Unix ms)
        pub completed_at: Option<i64>,

        /// Result (when completed)
        pub result: Option<SubagentResult>,
    }

    /// Sub-agent status
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub enum SubagentStatus {
        Pending,
        Running,
        Completed,
        Failed,
        Interrupted,
        TimedOut,
    }

    /// Result from a sub-agent execution
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SubagentResult {
        /// Whether execution succeeded
        pub success: bool,

        /// Output content
        pub output: String,

        /// Optional summary of the output
        pub summary: Option<String>,

        /// Duration in milliseconds
        pub duration_ms: u64,

        /// Tokens used
        pub tokens_used: Option<u32>,

        /// Cost in USD
        pub cost_usd: Option<f64>,

        /// Error message (if failed)
        pub error: Option<String>,
    }

    /// Completion notification
    #[derive(Debug, Clone)]
    pub struct SubagentCompletion {
        /// Task ID
        pub id: String,

        /// Parent run ID, when this completion belongs to a sub-agent run.
        pub parent_run_id: Option<String>,

        /// Final terminal status.
        pub status: SubagentStatus,

        /// Execution result payload when available.
        pub result: Option<SubagentResult>,
    }

    /// High-level subagent lifecycle management.
    ///
    /// Abstracts `SubagentTracker` + `SubagentDefLookup` + `spawn_subagent` so that
    /// tool implementations can manage subagents without depending on `ai`.
    #[async_trait::async_trait]
    pub trait SubagentManager: Send + Sync {
        /// Spawn a new sub-agent from a contract request payload.
        fn spawn(
            &self,
            request: ContractRunSpawnRequest,
        ) -> std::result::Result<SpawnHandle, ToolError>;

        /// List all callable sub-agent definitions.
        fn list_callable(&self) -> Vec<SubagentDefSummary>;

        /// List currently running sub-agents across all parents.
        ///
        /// This is the legacy/global view kept for backward compatibility.
        fn list_running(&self) -> Vec<SubagentState>;

        /// List currently running sub-agents that belong to one parent run.
        fn list_running_for_parent(&self, parent_run_id: &str) -> Vec<SubagentState> {
            let parent_run_id = parent_run_id.trim();
            if parent_run_id.is_empty() {
                return Vec::new();
            }

            self.list_running()
                .into_iter()
                .filter(|state| state.parent_run_id.as_deref() == Some(parent_run_id))
                .collect()
        }

        /// Number of currently running sub-agents.
        fn running_count(&self) -> usize;

        /// Wait for a sub-agent to complete, returning its terminal outcome.
        async fn wait(&self, task_id: &str) -> Option<SubagentCompletion>;

        /// Wait for a sub-agent that is owned by the given parent run.
        async fn wait_for_parent_owned_task(
            &self,
            task_id: &str,
            parent_run_id: &str,
        ) -> Option<SubagentCompletion>;

        /// Access the sub-agent configuration.
        fn config(&self) -> &SubagentConfig;
    }

    fn normalize_identifier(value: &str) -> String {
        let mut normalized = String::with_capacity(value.len());
        let mut previous_dash = false;

        for ch in value.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                normalized.push(ch.to_ascii_lowercase());
                previous_dash = false;
                continue;
            }
            if !previous_dash {
                normalized.push('-');
                previous_dash = true;
            }
        }

        normalized.trim_matches('-').to_string()
    }

    fn normalize_optional_text(value: Option<String>) -> Option<String> {
        value
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    }

    fn normalize_model_provider_pair(
        model: Option<String>,
        model_provider: Option<String>,
    ) -> std::result::Result<(Option<String>, Option<String>), ToolError> {
        let model = normalize_optional_text(model);
        let model_provider = normalize_optional_text(model_provider);
        if model.is_some() != model_provider.is_some() {
            return Err(ToolError::Tool(
                "Model override requires both 'model' and 'provider' fields.".to_string(),
            ));
        }
        Ok((model, model_provider))
    }

    fn normalize_inline_config(
        inline: Option<ContractInlineAgentRunConfig>,
    ) -> Option<InlineRunConfig> {
        let inline = inline?;
        let config = InlineRunConfig {
            name: inline.name,
            system_prompt: inline.system_prompt,
            allowed_tools: inline.allowed_tools,
            max_iterations: inline.max_iterations,
        };

        if config.name.is_none()
            && config.system_prompt.is_none()
            && config.allowed_tools.is_none()
            && config.max_iterations.is_none()
        {
            None
        } else {
            Some(config)
        }
    }

    pub fn resolve_agent_id(
        available_agents: &[SubagentDefSummary],
        requested: &str,
    ) -> std::result::Result<String, ToolError> {
        let query = requested.trim();
        if query.is_empty() {
            return Err(ToolError::Tool("Agent name must not be empty".to_string()));
        }

        if available_agents.is_empty() {
            return Err(ToolError::Tool(
                "No callable sub-agents available. Create an agent first.".to_string(),
            ));
        }

        if let Some(found) = available_agents.iter().find(|agent| agent.id == query) {
            return Ok(found.id.clone());
        }

        if let Some(found) = available_agents
            .iter()
            .find(|agent| agent.id.eq_ignore_ascii_case(query))
        {
            return Ok(found.id.clone());
        }

        let exact_name_matches: Vec<_> = available_agents
            .iter()
            .filter(|agent| agent.name.eq_ignore_ascii_case(query))
            .collect();
        if exact_name_matches.len() == 1 {
            return Ok(exact_name_matches[0].id.clone());
        }
        if exact_name_matches.len() > 1 {
            let ids = exact_name_matches
                .iter()
                .map(|agent| agent.id.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ToolError::Tool(format!(
                "Ambiguous agent name '{}'. Matching IDs: {}",
                query, ids
            )));
        }

        let normalized_query = normalize_identifier(query);
        let normalized_matches: Vec<_> = available_agents
            .iter()
            .filter(|agent| {
                normalize_identifier(&agent.id) == normalized_query
                    || normalize_identifier(&agent.name) == normalized_query
            })
            .collect();
        if normalized_matches.len() == 1 {
            return Ok(normalized_matches[0].id.clone());
        }
        if normalized_matches.len() > 1 {
            let ids = normalized_matches
                .iter()
                .map(|agent| agent.id.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ToolError::Tool(format!(
                "Ambiguous agent identifier '{}'. Matching IDs: {}",
                query, ids
            )));
        }

        let suggestions = available_agents
            .iter()
            .take(8)
            .map(|agent| format!("{} ({})", agent.name, agent.id))
            .collect::<Vec<_>>()
            .join(", ");
        Err(ToolError::Tool(format!(
            "Unknown agent '{}'. Available agents: {}",
            query, suggestions
        )))
    }

    pub fn spawn_request_from_contract(
        available_agents: &[SubagentDefSummary],
        request: ContractRunSpawnRequest,
    ) -> std::result::Result<SpawnRequest, ToolError> {
        let task = request.task.trim();
        if task.is_empty() {
            return Err(ToolError::Tool(
                "Single spawn requires non-empty 'task'.".to_string(),
            ));
        }

        let inline = normalize_inline_config(request.inline);
        let agent_id = match request.agent_id {
            Some(agent_id) => Some(resolve_agent_id(available_agents, &agent_id)?),
            None => None,
        };
        if agent_id.is_some() && inline.is_some() {
            return Err(ToolError::Tool(
                "Inline temporary-subagent fields cannot be combined with 'agent'.".to_string(),
            ));
        }

        let (model, model_provider) =
            normalize_model_provider_pair(request.model, request.model_provider)?;

        let mut spawn_request = SpawnRequest {
            agent_id,
            inline,
            task: task.to_string(),
            timeout_secs: request.timeout_secs,
            max_iterations: request.max_iterations,
            priority: request.priority.map(Into::into),
            model,
            model_provider,
            parent_run_id: None,
            run_id: None,
        };
        spawn_request.set_parent_run_id(request.parent_run_id);
        Ok(spawn_request)
    }

    impl From<ContractSpawnPriority> for SpawnPriority {
        fn from(value: ContractSpawnPriority) -> Self {
            match value {
                ContractSpawnPriority::Low => Self::Low,
                ContractSpawnPriority::Normal => Self::Normal,
                ContractSpawnPriority::High => Self::High,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::Mutex;

        struct MockSubagentManager {
            running: Mutex<Vec<SubagentState>>,
            config: SubagentConfig,
        }

        #[async_trait::async_trait]
        impl SubagentManager for MockSubagentManager {
            fn spawn(
                &self,
                _request: ContractRunSpawnRequest,
            ) -> std::result::Result<SpawnHandle, ToolError> {
                Err(ToolError::Tool("not implemented".to_string()))
            }

            fn list_callable(&self) -> Vec<SubagentDefSummary> {
                Vec::new()
            }

            fn list_running(&self) -> Vec<SubagentState> {
                self.running.lock().expect("running lock").clone()
            }

            fn running_count(&self) -> usize {
                self.running.lock().expect("running lock").len()
            }

            async fn wait(&self, _task_id: &str) -> Option<SubagentCompletion> {
                None
            }

            async fn wait_for_parent_owned_task(
                &self,
                _task_id: &str,
                _parent_run_id: &str,
            ) -> Option<SubagentCompletion> {
                None
            }

            fn config(&self) -> &SubagentConfig {
                &self.config
            }
        }

        #[test]
        fn test_spawn_handle_serialization() {
            let handle = SpawnHandle {
                id: "task-123".to_string(),
                agent_name: "Researcher".to_string(),
                effective_limits: SubagentEffectiveLimits {
                    timeout_secs: 300,
                    timeout_source: SubagentLimitSource::ConfigDefault,
                    max_iterations: DEFAULT_AGENT_MAX_ITERATIONS,
                    max_iterations_source: SubagentLimitSource::ConfigDefault,
                },
            };

            let json = serde_json::to_string(&handle).unwrap();
            assert!(json.contains("task-123"));
        }

        #[test]
        fn test_list_running_for_parent_filters_legacy_global_view() {
            let manager = MockSubagentManager {
                running: Mutex::new(vec![
                    SubagentState {
                        id: "run-1".to_string(),
                        agent_name: "child-a".to_string(),
                        task: "task-a".to_string(),
                        parent_run_id: Some("parent-1".to_string()),
                        status: SubagentStatus::Running,
                        started_at: 1,
                        completed_at: None,
                        result: None,
                    },
                    SubagentState {
                        id: "run-2".to_string(),
                        agent_name: "child-b".to_string(),
                        task: "task-b".to_string(),
                        parent_run_id: Some("parent-2".to_string()),
                        status: SubagentStatus::Running,
                        started_at: 2,
                        completed_at: None,
                        result: None,
                    },
                ]),
                config: SubagentConfig::default(),
            };

            let filtered = manager.list_running_for_parent("parent-1");
            assert_eq!(filtered.len(), 1);
            assert_eq!(filtered[0].id, "run-1");
        }

        #[test]
        fn test_list_running_for_parent_rejects_blank_parent() {
            let manager = MockSubagentManager {
                running: Mutex::new(Vec::new()),
                config: SubagentConfig::default(),
            };

            assert!(manager.list_running_for_parent("   ").is_empty());
        }

        #[test]
        fn test_spawn_request_serializes_parent_run_id_canonically() {
            let mut request = SpawnRequest {
                agent_id: Some("coder".to_string()),
                inline: None,
                task: "Investigate".to_string(),
                timeout_secs: None,
                max_iterations: None,
                priority: None,
                model: None,
                model_provider: None,
                parent_run_id: None,
                run_id: None,
            };
            request.set_parent_run_id(Some("parent-1".to_string()));

            let serialized = serde_json::to_value(request).expect("serialize spawn request");
            assert_eq!(serialized["parent_run_id"], "parent-1");
        }
    }
}

pub mod tool {
    //! Tool trait and types for AI agent tools.

    pub use crate::contracts::ToolErrorCategory;
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::sync::Arc;

    use crate::error::Result;

    /// Describes an action a tool is about to take.
    #[derive(Debug, Clone)]
    pub struct ToolAction {
        pub tool_name: String,
        pub operation: String,
        pub target: String,
        pub summary: String,
    }

    /// Result of a security check.
    #[derive(Debug, Clone)]
    pub struct SecurityDecision {
        pub allowed: bool,
        pub requires_approval: bool,
        pub approval_id: Option<String>,
        pub reason: Option<String>,
    }

    impl SecurityDecision {
        pub fn allowed(reason: Option<String>) -> Self {
            Self {
                allowed: true,
                requires_approval: false,
                approval_id: None,
                reason,
            }
        }

        pub fn blocked(reason: Option<String>) -> Self {
            Self {
                allowed: false,
                requires_approval: false,
                approval_id: None,
                reason,
            }
        }

        pub fn requires_approval(approval_id: String, reason: Option<String>) -> Self {
            Self {
                allowed: false,
                requires_approval: true,
                approval_id: Some(approval_id),
                reason,
            }
        }
    }

    /// Application-level security approval interface.
    #[async_trait]
    pub trait SecurityGate: Send + Sync {
        async fn check_command(
            &self,
            command: &str,
            task_id: &str,
            agent_id: &str,
            workdir: Option<&str>,
        ) -> Result<SecurityDecision>;

        async fn check_tool_action(
            &self,
            _action: &ToolAction,
            _agent_id: Option<&str>,
            _task_id: Option<&str>,
        ) -> Result<SecurityDecision> {
            Ok(SecurityDecision::allowed(None))
        }
    }

    /// Type alias for secret resolution callbacks.
    pub type SecretResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

    /// Check security gate and return a blocking message if the action is denied.
    pub async fn check_security(
        gate: Option<&dyn SecurityGate>,
        action: ToolAction,
        agent_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<Option<String>> {
        let Some(gate) = gate else {
            // Default-open fallback for environments where security policies
            // are intentionally not configured.
            return Ok(None);
        };

        let decision = gate.check_tool_action(&action, agent_id, task_id).await?;

        if decision.allowed {
            return Ok(None);
        }

        if decision.requires_approval {
            let approval_id = decision
                .approval_id
                .unwrap_or_else(|| "unknown".to_string());
            return Ok(Some(format!(
                "Action requires user approval (ID: {}). Waiting for approval of: {}",
                approval_id, action.summary
            )));
        }

        let reason = decision
            .reason
            .unwrap_or_else(|| "Action blocked by policy".to_string());
        Ok(Some(format!("Action blocked: {}", reason)))
    }

    /// JSON Schema for tool parameters.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolSchema {
        pub name: String,
        pub description: String,
        pub parameters: Value, // JSON Schema object
    }

    /// Result of tool execution.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolOutput {
        pub success: bool,
        pub result: Value,
        pub error: Option<String>,
        pub error_category: Option<ToolErrorCategory>,
        pub retryable: Option<bool>,
        pub retry_after_ms: Option<u64>,
    }

    impl ToolOutput {
        /// Create a successful tool output.
        pub fn success(result: Value) -> Self {
            Self {
                success: true,
                result,
                error: None,
                error_category: None,
                retryable: None,
                retry_after_ms: None,
            }
        }

        /// Create an error tool output.
        pub fn error(message: impl Into<String>) -> Self {
            Self {
                success: false,
                result: Value::Null,
                error: Some(message.into()),
                error_category: None,
                retryable: None,
                retry_after_ms: None,
            }
        }

        pub fn retryable_error(message: impl Into<String>, category: ToolErrorCategory) -> Self {
            Self {
                success: false,
                result: Value::Null,
                error: Some(message.into()),
                error_category: Some(category),
                retryable: Some(true),
                retry_after_ms: None,
            }
        }

        pub fn non_retryable_error(
            message: impl Into<String>,
            category: ToolErrorCategory,
        ) -> Self {
            Self {
                success: false,
                result: Value::Null,
                error: Some(message.into()),
                error_category: Some(category),
                retryable: Some(false),
                retry_after_ms: None,
            }
        }

        pub fn with_error_message(mut self, message: impl Into<String>) -> Self {
            self.error = Some(message.into());
            self
        }

        pub fn classify_if_error(
            mut self,
            classifier: impl FnOnce(&str) -> (ToolErrorCategory, bool, Option<u64>),
        ) -> Self {
            if !self.success
                && let Some(err) = self.error.as_deref()
            {
                let (category, retryable, retry_after_ms) = classifier(err);
                self.error_category = Some(category);
                self.retryable = Some(retryable);
                self.retry_after_ms = retry_after_ms;
            }
            self
        }
    }

    /// Core trait for agent tools.
    ///
    /// TODO(ToolSearch): Add tool discovery/deferral metadata to support lazy-loading.
    /// Claude Code's ToolSearch pattern (src/tools/ToolSearchTool/) saves prompt tokens
    /// by only exposing core tools upfront and loading the rest on-demand.
    ///
    /// Proposed additions:
    /// ```ignore
    /// /// Whether this tool can run concurrently with other tools.
    /// /// When false, the executor runs it serially (after concurrent batch).
    /// fn is_concurrency_safe(&self, _input: &Value) -> bool { false }
    ///
    /// /// Whether this tool only reads data (no side effects).
    /// /// Read-only tools in the same batch can run in parallel.
    /// fn is_read_only(&self, _input: &Value) -> bool { false }
    ///
    /// /// Whether this tool performs irreversible operations (delete, overwrite, send).
    /// fn is_destructive(&self, _input: &Value) -> bool { false }
    ///
    /// /// Whether to defer loading this tool's schema until the model requests it.
    /// /// Deferred tools are hidden from the initial prompt and discovered via ToolSearch.
    /// fn should_defer(&self) -> bool { false }
    ///
    /// /// Whether this tool must always appear in the initial prompt (never deferred).
    /// fn always_load(&self) -> bool { false }
    ///
    /// /// Short capability phrase for keyword-based tool search (3-10 words).
    /// fn search_hint(&self) -> Option<&str> { None }
    /// ```
    ///
    /// Implementation plan:
    /// 1. Add these methods with defaults to this trait
    /// 2. Create a `ToolSearchTool` in tools that does keyword matching
    ///    over deferred tools (no embedding needed — Claude Code uses pure keyword
    ///    scoring: name parts +10, search_hint +4, description +2)
    /// 3. In executor (ai/src/agent/executor/mod.rs:560), split tools into
    ///    `always_load` vs `should_defer`, only send loaded tools to the LLM
    /// 4. When the model calls ToolSearch, return matching tool schemas as text
    ///    (Anthropic's `tool_reference` beta is provider-specific, so return full
    ///    schema text and inject into next API call's tools array)
    /// 5. Auto-enable when deferred tool schemas exceed 10% of context window
    ///    (see Claude Code's `tst-auto` mode in src/utils/toolSearch.ts)
    ///
    /// Benefits: 58 tools × ~800 tokens/schema = ~46K tokens. ToolSearch cuts this
    /// to ~5K for the average request (6 core tools + ToolSearch).
    #[async_trait]
    pub trait Tool: Send + Sync {
        /// Unique tool name (used in LLM function calls).
        fn name(&self) -> &str;

        /// Human-readable description for LLM context.
        fn description(&self) -> &str;

        /// JSON Schema for input parameters.
        fn parameters_schema(&self) -> Value;

        /// Execute the tool with given input.
        async fn execute(&self, input: Value) -> Result<ToolOutput>;

        /// Build complete schema for LLM.
        fn schema(&self) -> ToolSchema {
            ToolSchema {
                name: self.name().to_string(),
                description: self.description().to_string(),
                parameters: self.parameters_schema(),
            }
        }
    }
}

pub mod toolset {
    //! Composable toolset abstraction.

    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::Value;
    use tokio::sync::Semaphore;

    use crate::error::{Result, ToolError};
    use crate::tool::{Tool, ToolOutput, ToolSchema};

    /// Intercepts tool execution and optionally delegates to the next tool in the chain.
    #[async_trait]
    pub trait ToolWrapper: Send + Sync {
        fn wrapper_name(&self) -> &str;

        async fn wrap_execute(
            &self,
            tool_name: &str,
            input: Value,
            next: &dyn Tool,
        ) -> Result<ToolOutput>;
    }

    /// A tool implementation that applies wrappers around an inner tool.
    pub struct WrappedTool {
        inner: Arc<dyn Tool>,
        wrappers: Vec<Arc<dyn ToolWrapper>>,
    }

    impl WrappedTool {
        pub fn new(inner: Arc<dyn Tool>, wrappers: Vec<Arc<dyn ToolWrapper>>) -> Self {
            Self { inner, wrappers }
        }

        pub fn inner(&self) -> &Arc<dyn Tool> {
            &self.inner
        }
    }

    struct RemainingChain<'a> {
        tool_name: &'a str,
        inner: &'a dyn Tool,
        wrappers: &'a [Arc<dyn ToolWrapper>],
        index: usize,
    }

    #[async_trait]
    impl Tool for RemainingChain<'_> {
        fn name(&self) -> &str {
            self.tool_name
        }

        fn description(&self) -> &str {
            self.inner.description()
        }

        fn parameters_schema(&self) -> Value {
            self.inner.parameters_schema()
        }

        async fn execute(&self, input: Value) -> Result<ToolOutput> {
            execute_chain(self.tool_name, self.inner, self.wrappers, self.index, input).await
        }
    }

    async fn execute_chain(
        tool_name: &str,
        inner: &dyn Tool,
        wrappers: &[Arc<dyn ToolWrapper>],
        index: usize,
        input: Value,
    ) -> Result<ToolOutput> {
        if index >= wrappers.len() {
            return inner.execute(input).await;
        }

        let next = RemainingChain {
            tool_name,
            inner,
            wrappers,
            index: index + 1,
        };
        wrappers[index].wrap_execute(tool_name, input, &next).await
    }

    #[async_trait]
    impl Tool for WrappedTool {
        fn name(&self) -> &str {
            self.inner.name()
        }

        fn description(&self) -> &str {
            self.inner.description()
        }

        fn parameters_schema(&self) -> Value {
            self.inner.parameters_schema()
        }

        async fn execute(&self, input: Value) -> Result<ToolOutput> {
            execute_chain(self.name(), self.inner.as_ref(), &self.wrappers, 0, input).await
        }
    }

    /// Wrapper that enforces a timeout per tool call.
    pub struct TimeoutWrapper {
        timeout: Duration,
    }

    impl TimeoutWrapper {
        pub fn new(timeout: Duration) -> Self {
            Self { timeout }
        }
    }

    #[async_trait]
    impl ToolWrapper for TimeoutWrapper {
        fn wrapper_name(&self) -> &str {
            "timeout"
        }

        async fn wrap_execute(
            &self,
            tool_name: &str,
            input: Value,
            next: &dyn Tool,
        ) -> Result<ToolOutput> {
            match tokio::time::timeout(self.timeout, next.execute(input)).await {
                Ok(result) => result,
                Err(_) => Err(ToolError::Tool(format!(
                    "Tool '{tool_name}' timed out after {}ms",
                    self.timeout.as_millis()
                ))),
            }
        }
    }

    /// Wrapper that limits concurrent executions of the wrapped tool.
    pub struct RateLimitWrapper {
        semaphore: Arc<Semaphore>,
    }

    impl RateLimitWrapper {
        pub fn new(max_concurrent: usize) -> Self {
            let permits = max_concurrent.max(1);
            Self {
                semaphore: Arc::new(Semaphore::new(permits)),
            }
        }
    }

    #[async_trait]
    impl ToolWrapper for RateLimitWrapper {
        fn wrapper_name(&self) -> &str {
            "rate_limit"
        }

        async fn wrap_execute(
            &self,
            tool_name: &str,
            input: Value,
            next: &dyn Tool,
        ) -> Result<ToolOutput> {
            let _permit = self.semaphore.acquire().await.map_err(|_| {
                ToolError::Tool(format!(
                    "Rate limiter for tool '{tool_name}' is unavailable"
                ))
            })?;
            next.execute(input).await
        }
    }

    /// Registry for managing available tools.
    pub struct ToolRegistry {
        tools: HashMap<String, Arc<dyn Tool>>,
    }

    impl Default for ToolRegistry {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ToolRegistry {
        pub fn new() -> Self {
            Self {
                tools: HashMap::new(),
            }
        }

        pub fn register<T: Tool + 'static>(&mut self, tool: T) {
            let name = tool.name().to_string();
            self.tools.insert(name, Arc::new(tool));
        }

        pub fn register_arc(&mut self, tool: Arc<dyn Tool>) {
            let name = tool.name().to_string();
            self.tools.insert(name, tool);
        }

        pub fn register_wrapped_arc(
            &mut self,
            tool: Arc<dyn Tool>,
            wrappers: Vec<Arc<dyn ToolWrapper>>,
        ) {
            let wrapped = Arc::new(WrappedTool::new(tool, wrappers));
            let name = wrapped.name().to_string();
            self.tools.insert(name, wrapped);
        }

        pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
            self.tools.get(name).cloned()
        }

        pub fn has(&self, name: &str) -> bool {
            self.tools.contains_key(name)
        }

        pub fn list(&self) -> Vec<&str> {
            self.tools.keys().map(|s| s.as_str()).collect()
        }

        pub fn schemas(&self) -> Vec<ToolSchema> {
            self.tools.values().map(|t| t.schema()).collect()
        }

        pub async fn execute(&self, name: &str, input: Value) -> Result<ToolOutput> {
            let tool = self
                .get(name)
                .ok_or_else(|| ToolError::NotFound(name.to_string()))?;
            tool.execute(input).await
        }

        pub async fn execute_safe(&self, name: &str, input: Value) -> Result<ToolOutput> {
            self.execute(name, input).await
        }
    }

    pub type ToolPredicate = Arc<dyn Fn(&ToolSchema) -> bool + Send + Sync>;

    /// Toolset wrapper that filters visible/callable tools by predicate.
    pub struct FilteredToolset<T> {
        inner: T,
        predicate: ToolPredicate,
    }

    impl<T> FilteredToolset<T> {
        pub fn new(inner: T, predicate: ToolPredicate) -> Self {
            Self { inner, predicate }
        }
    }

    impl<T: Toolset> FilteredToolset<T> {
        pub fn from_allowlist(inner: T, allowed_tools: &[String]) -> Self {
            let allowed: HashSet<String> = allowed_tools
                .iter()
                .map(|name| name.trim())
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .collect();

            let predicate = Arc::new(move |tool: &ToolSchema| {
                if allowed.is_empty() {
                    return true;
                }
                allowed.contains(&tool.name)
            });

            Self::new(inner, predicate)
        }
    }

    #[async_trait]
    impl<T: Toolset> Toolset for FilteredToolset<T> {
        fn list_tools(&self) -> Vec<ToolSchema> {
            self.inner
                .list_tools()
                .into_iter()
                .filter(|tool| (self.predicate)(tool))
                .collect()
        }

        async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput> {
            if !self
                .list_tools()
                .iter()
                .any(|tool| tool.name.as_str() == name)
            {
                return Err(ToolError::NotFound(name.to_string()));
            }
            self.inner.call_tool(name, args).await
        }

        async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput> {
            if !self
                .list_tools()
                .iter()
                .any(|tool| tool.name.as_str() == name)
            {
                return Err(ToolError::NotFound(name.to_string()));
            }
            self.inner.call_tool_safe(name, args).await
        }
    }

    /// Runtime context for optional per-step toolset preparation.
    #[derive(Debug, Clone, Default)]
    pub struct ToolsetContext {
        pub step: Option<usize>,
        pub agent_id: Option<String>,
    }

    /// Common abstraction over different toolset implementations.
    #[async_trait]
    pub trait Toolset: Send + Sync {
        /// List schemas for all currently available tools.
        fn list_tools(&self) -> Vec<ToolSchema>;

        /// Call a tool by name.
        async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput>;

        /// Call a tool with parallel-safety semantics.
        async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput>;

        /// Optional preparation callback before each step.
        async fn prepare(&self, _context: &ToolsetContext) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl Toolset for ToolRegistry {
        fn list_tools(&self) -> Vec<ToolSchema> {
            self.schemas()
        }

        async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput> {
            self.execute(name, args).await
        }

        async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput> {
            self.execute_safe(name, args).await
        }
    }

    #[async_trait]
    impl<T: Toolset + ?Sized> Toolset for Arc<T> {
        fn list_tools(&self) -> Vec<ToolSchema> {
            self.as_ref().list_tools()
        }

        async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput> {
            self.as_ref().call_tool(name, args).await
        }

        async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput> {
            self.as_ref().call_tool_safe(name, args).await
        }

        async fn prepare(&self, context: &ToolsetContext) -> Result<()> {
            self.as_ref().prepare(context).await
        }
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use serde_json::json;
        use tokio::time::sleep;

        use super::*;

        struct EchoTool;
        struct ReverseTool;

        #[async_trait]
        impl Tool for EchoTool {
            fn name(&self) -> &str {
                "echo"
            }

            fn description(&self) -> &str {
                "Echo input"
            }

            fn parameters_schema(&self) -> Value {
                json!({"type":"object"})
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                Ok(ToolOutput::success(input))
            }
        }

        #[async_trait]
        impl Tool for ReverseTool {
            fn name(&self) -> &str {
                "reverse"
            }

            fn description(&self) -> &str {
                "Reverse input"
            }

            fn parameters_schema(&self) -> Value {
                json!({"type":"object"})
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                Ok(ToolOutput::success(input))
            }
        }

        #[test]
        fn registry_as_toolset_lists_tools() {
            let registry = ToolRegistry::new();
            let tools = Toolset::list_tools(&registry);
            assert!(tools.is_empty());
        }

        #[tokio::test]
        async fn registry_as_toolset_call_unknown_fails() {
            let registry = ToolRegistry::new();
            let result = Toolset::call_tool(&registry, "missing_tool", json!({})).await;
            assert!(result.is_err());
        }

        #[test]
        fn allowlist_filters_tool_schemas() {
            let mut registry = ToolRegistry::new();
            registry.register(EchoTool);
            registry.register(ReverseTool);

            let toolset = FilteredToolset::from_allowlist(registry, &["echo".to_string()]);
            let names: Vec<String> = toolset
                .list_tools()
                .into_iter()
                .map(|schema| schema.name)
                .collect();

            assert_eq!(names, vec!["echo".to_string()]);
        }

        #[tokio::test]
        async fn blocked_tool_call_returns_not_found() {
            let mut registry = ToolRegistry::new();
            registry.register(EchoTool);
            registry.register(ReverseTool);

            let toolset = FilteredToolset::from_allowlist(registry, &["echo".to_string()]);
            let err = toolset
                .call_tool("reverse", json!({"text":"hello"}))
                .await
                .unwrap_err();

            assert!(matches!(err, ToolError::NotFound(ref name) if name == "reverse"));
        }

        struct TraceWrapper {
            name: &'static str,
            trace: Arc<Mutex<Vec<String>>>,
        }

        #[async_trait]
        impl ToolWrapper for TraceWrapper {
            fn wrapper_name(&self) -> &str {
                self.name
            }

            async fn wrap_execute(
                &self,
                _tool_name: &str,
                input: Value,
                next: &dyn Tool,
            ) -> Result<ToolOutput> {
                self.trace
                    .lock()
                    .expect("trace mutex should not be poisoned")
                    .push(format!("before:{}", self.name));
                let result = next.execute(input).await;
                self.trace
                    .lock()
                    .expect("trace mutex should not be poisoned")
                    .push(format!("after:{}", self.name));
                result
            }
        }

        #[tokio::test]
        async fn wrapper_chain_executes_in_order() {
            let trace = Arc::new(Mutex::new(Vec::new()));
            let wrappers: Vec<Arc<dyn ToolWrapper>> = vec![
                Arc::new(TraceWrapper {
                    name: "w1",
                    trace: trace.clone(),
                }),
                Arc::new(TraceWrapper {
                    name: "w2",
                    trace: trace.clone(),
                }),
            ];
            let tool = WrappedTool::new(Arc::new(EchoTool), wrappers);

            let output = tool
                .execute(json!({"msg":"hello"}))
                .await
                .expect("wrapped execution should succeed");
            assert!(output.success);
            let events = trace
                .lock()
                .expect("trace mutex should not be poisoned")
                .clone();
            assert_eq!(
                events,
                vec!["before:w1", "before:w2", "after:w2", "after:w1"]
            );
        }

        #[tokio::test]
        async fn timeout_wrapper_cancels_slow_tool() {
            struct SlowTool;

            #[async_trait]
            impl Tool for SlowTool {
                fn name(&self) -> &str {
                    "slow"
                }

                fn description(&self) -> &str {
                    "Slow tool"
                }

                fn parameters_schema(&self) -> Value {
                    json!({"type":"object"})
                }

                async fn execute(&self, _input: Value) -> Result<ToolOutput> {
                    sleep(Duration::from_millis(80)).await;
                    Ok(ToolOutput::success(json!({"ok":true})))
                }
            }

            let wrapped = WrappedTool::new(
                Arc::new(SlowTool),
                vec![Arc::new(TimeoutWrapper::new(Duration::from_millis(20)))],
            );
            let error = wrapped
                .execute(json!({}))
                .await
                .expect_err("slow tool should timeout");
            assert!(error.to_string().contains("timed out"));
        }

        #[tokio::test]
        async fn rate_limit_wrapper_limits_concurrency() {
            struct CountingTool {
                in_flight: Arc<AtomicUsize>,
                max_seen: Arc<AtomicUsize>,
            }

            #[async_trait]
            impl Tool for CountingTool {
                fn name(&self) -> &str {
                    "counting"
                }

                fn description(&self) -> &str {
                    "Counting tool"
                }

                fn parameters_schema(&self) -> Value {
                    json!({"type":"object"})
                }

                async fn execute(&self, _input: Value) -> Result<ToolOutput> {
                    let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    self.max_seen.fetch_max(current, Ordering::SeqCst);
                    sleep(Duration::from_millis(40)).await;
                    self.in_flight.fetch_sub(1, Ordering::SeqCst);
                    Ok(ToolOutput::success(json!({"ok":true})))
                }
            }

            let in_flight = Arc::new(AtomicUsize::new(0));
            let max_seen = Arc::new(AtomicUsize::new(0));
            let wrapped = Arc::new(WrappedTool::new(
                Arc::new(CountingTool {
                    in_flight: in_flight.clone(),
                    max_seen: max_seen.clone(),
                }),
                vec![Arc::new(RateLimitWrapper::new(1))],
            ));

            let mut tasks = Vec::new();
            for _ in 0..3 {
                let tool = wrapped.clone();
                tasks.push(tokio::spawn(async move { tool.execute(json!({})).await }));
            }
            for task in tasks {
                let result = task.await.expect("task should join");
                assert!(result.is_ok());
            }

            assert_eq!(max_seen.load(Ordering::SeqCst), 1);
        }
    }
}

pub mod contracts {
    pub mod request {
        use serde::{Deserialize, Serialize};
        use std::collections::HashMap;

        use super::ExecutionScope;
        use crate::ChatRole;

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(tag = "type", content = "data")]
        #[non_exhaustive]
        pub enum IpcRequest {
            Ping,
            GetStatus,
            Shutdown,

            ListAgents,
            GetAgent {
                id: String,
            },
            CreateAgent {
                name: String,
                agent: AgentNode,
            },
            UpdateAgent {
                id: String,
                name: Option<String>,
                agent: Option<AgentNode>,
            },
            DeleteAgent {
                id: String,
            },

            ListSkills,
            GetSkill {
                id: String,
            },
            GetSkillReference {
                skill_id: String,
                ref_id: String,
            },

            RunCleanup,

            ListSecrets,
            GetSecret {
                key: String,
            },
            SetSecret {
                key: String,
                value: String,
                description: Option<String>,
            },
            DeleteSecret {
                key: String,
            },

            GetConfig,
            GetGlobalConfig,
            SetConfig {
                config: SystemConfig,
            },

            ListSessions,
            ListFullSessions,
            ListSessionsByAgent {
                agent_id: String,
            },
            ListSessionsBySkill {
                skill_id: String,
            },
            CountSessions,
            DeleteSessionsOlderThan {
                older_than_ms: i64,
            },
            GetSession {
                id: String,
            },
            CreateSession {
                agent_id: Option<String>,
                model: Option<String>,
                name: Option<String>,
                skill_id: Option<String>,
            },
            CreateSessionWithProvider {
                agent_id: Option<String>,
                provider: Option<String>,
                model: Option<String>,
                name: Option<String>,
                skill_id: Option<String>,
            },
            UpdateSession {
                id: String,
                updates: ChatSessionUpdate,
            },
            RenameSession {
                id: String,
                name: String,
            },
            ArchiveSession {
                id: String,
            },
            DeleteSession {
                id: String,
            },
            SearchSessions {
                query: String,
                agent_id: Option<String>,
                limit: Option<usize>,
            },
            AddMessage {
                session_id: String,
                role: ChatRole,
                content: String,
            },
            AppendMessage {
                session_id: String,
                message: ChatMessage,
            },
            SteerChatSessionStream {
                session_id: String,
                instruction: String,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                scope: Option<ExecutionScope>,
            },
            CancelChatSessionStream {
                stream_id: String,
            },
            CancelChatSessionStreamScoped {
                stream_id: String,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                session_id: Option<String>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                scope: Option<ExecutionScope>,
            },
            GetSessionMessages {
                session_id: String,
                limit: Option<usize>,
            },
            ListExecutionContainers,
            ListRuns {
                query: RunListQuery,
            },
            SubscribeSessionEvents,
            SwitchSessionModel {
                session_id: String,
                model_ref: WireModelRef,
                #[serde(default)]
                reason: Option<String>,
            },

            GetSystemInfo,
            GetAvailableModels,
            GetAvailableTools,
            GetAvailableToolDefinitions,
            ListMcpServers,

            BuildAgentSystemPrompt {
                agent_node: AgentNode,
            },
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "snake_case")]
        pub enum CodexCliExecutionMode {
            Safe,
            Bypass,
            #[default]
            Unknown,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "snake_case")]
        pub enum SkillPreflightPolicyMode {
            Off,
            #[default]
            Warn,
            Enforce,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub struct ModelRoutingConfig {
            pub enabled: bool,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub routine_model: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub moderate_model: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub complex_model: Option<String>,
            pub escalate_on_failure: bool,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(rename_all = "snake_case", tag = "type", content = "value")]
        pub enum ApiKeyConfig {
            Direct(String),
            Secret(String),
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub struct WireModelRef {
            pub provider: String,
            pub model: String,
        }

        pub type ModelRef = WireModelRef;

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        pub struct AgentNode {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub model_ref: Option<WireModelRef>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub prompt: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub temperature: Option<f64>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub codex_cli_reasoning_effort: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub codex_cli_execution_mode: Option<CodexCliExecutionMode>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub api_key_config: Option<ApiKeyConfig>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub tools: Option<Vec<String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub skills: Option<Vec<String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub skill_variables: Option<HashMap<String, String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub skill_preflight_policy_mode: Option<SkillPreflightPolicyMode>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub model_routing: Option<ModelRoutingConfig>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        pub struct InlineAgentRunConfig {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub name: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub system_prompt: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub allowed_tools: Option<Vec<String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub max_iterations: Option<u32>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "snake_case")]
        pub enum SpawnPriority {
            Low,
            #[default]
            Normal,
            High,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        pub struct RunSpawnRequest {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub agent_id: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub inline: Option<InlineAgentRunConfig>,
            pub task: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub timeout_secs: Option<u64>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub max_iterations: Option<u32>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub priority: Option<SpawnPriority>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub model: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub model_provider: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub parent_run_id: Option<String>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "snake_case")]
        pub enum ChatExecutionStatus {
            #[default]
            Running,
            Completed,
            Failed,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(rename_all = "snake_case")]
        pub enum ChatMediaType {
            Voice,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub struct ChatMessageMedia {
            pub media_type: ChatMediaType,
            pub file_path: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub duration_sec: Option<u32>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub struct ChatMessageTranscript {
            pub text: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub model: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub updated_at: Option<i64>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub struct ExecutionStepInfo {
            pub step_type: String,
            pub name: String,
            pub status: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub duration_ms: Option<u64>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub struct MessageExecution {
            pub steps: Vec<ExecutionStepInfo>,
            pub duration_ms: u64,
            pub tokens_used: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub cost_usd: Option<f64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub input_tokens: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub output_tokens: Option<u32>,
            pub status: ChatExecutionStatus,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub struct ChatMessage {
            #[serde(default)]
            pub id: String,
            pub role: ChatRole,
            pub content: String,
            pub timestamp: i64,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub execution: Option<MessageExecution>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub media: Option<ChatMessageMedia>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub transcript: Option<ChatMessageTranscript>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
        pub struct ChatSessionUpdate {
            pub agent_id: Option<String>,
            pub model: Option<String>,
            pub name: Option<String>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub struct SkillScript {
            pub id: String,
            pub path: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub lang: Option<String>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub struct SkillReference {
            pub id: String,
            pub path: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub title: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub summary: Option<String>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub struct SkillGating {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub bins: Option<Vec<String>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub env: Option<Vec<String>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub os: Option<Vec<String>>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "lowercase")]
        pub enum SkillStatus {
            #[default]
            Active,
            Completed,
            Archived,
            Draft,
        }

        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "snake_case")]
        pub enum SkillSource {
            System,
            #[default]
            User,
            External,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        pub struct Skill {
            pub id: String,
            pub name: String,
            pub description: Option<String>,
            pub tags: Option<Vec<String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub kind: Option<String>,
            #[serde(default)]
            pub executable: bool,
            pub content: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub folder_path: Option<String>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            pub suggested_tools: Vec<String>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            pub scripts: Vec<SkillScript>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            pub references: Vec<SkillReference>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub gating: Option<SkillGating>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub version: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub author: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub license: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub content_hash: Option<String>,
            #[serde(default)]
            pub status: SkillStatus,
            #[serde(default)]
            pub auto_complete: bool,
            #[serde(default)]
            pub source: SkillSource,
            #[serde(default)]
            pub read_only: bool,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub source_ref: Option<String>,
            pub created_at: i64,
            pub updated_at: i64,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(rename_all = "snake_case")]
        pub enum ExecutionContainerKind {
            Workspace,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub struct ExecutionContainerRef {
            pub kind: ExecutionContainerKind,
            pub id: String,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub struct RunListQuery {
            pub container: ExecutionContainerRef,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        pub struct AgentSettings {
            pub tool_timeout_secs: u64,
            pub llm_timeout_secs: Option<u64>,
            pub bash_timeout_secs: u64,
            pub python_timeout_secs: u64,
            pub browser_timeout_secs: u64,
            pub approval_timeout_secs: u64,
            #[serde(default)]
            pub auto_review_tools: bool,
            pub max_iterations: usize,
            pub max_depth: usize,
            pub subagent_timeout_secs: u64,
            pub max_parallel_subagents: usize,
            pub max_tool_calls: usize,
            pub max_tool_concurrency: usize,
            pub max_tool_result_length: usize,
            pub prune_tool_max_chars: usize,
            pub compact_preserve_tokens: usize,
            pub max_wall_clock_secs: Option<u64>,
            #[serde(default)]
            pub fallback_models: Option<Vec<String>>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        pub struct ApiSettings {
            pub session_list_limit: u32,
            pub web_search_num_results: usize,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        pub struct RuntimeSettings {
            pub chat_max_session_history: usize,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        pub struct RegistrySettings {
            pub github_cache_ttl_secs: u64,
            pub marketplace_cache_ttl_secs: u64,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
        pub struct SystemConfig {
            pub worker_count: usize,
            pub stall_timeout_seconds: u64,
            #[serde(default)]
            pub chat_response_timeout_seconds: Option<u64>,
            pub max_retries: u32,
            pub chat_session_retention_days: u32,
            pub log_file_retention_days: u32,
            pub experimental_features: Vec<String>,
            #[serde(default)]
            pub agent: AgentSettings,
            #[serde(default)]
            pub api_defaults: ApiSettings,
            #[serde(default)]
            pub runtime_defaults: RuntimeSettings,
            #[serde(default)]
            pub registry_defaults: RegistrySettings,
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            fn assert_roundtrip<T>(value: &T)
            where
                T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
            {
                let json = serde_json::to_string(value).unwrap();
                let decoded: T = serde_json::from_str(&json).unwrap();
                assert_eq!(&decoded, value);
            }

            fn sample_agent_node() -> AgentNode {
                AgentNode {
                    model_ref: Some(WireModelRef {
                        provider: "openai".to_string(),
                        model: "gpt-5".to_string(),
                    }),
                    prompt: Some("You are helpful.".to_string()),
                    temperature: Some(0.2),
                    codex_cli_reasoning_effort: Some("high".to_string()),
                    codex_cli_execution_mode: Some(CodexCliExecutionMode::Safe),
                    api_key_config: Some(ApiKeyConfig::Secret("OPENAI_API_KEY".to_string())),
                    tools: Some(vec!["bash".to_string()]),
                    skills: Some(vec!["skill-1".to_string()]),
                    skill_variables: Some(HashMap::from([(
                        "topic".to_string(),
                        "contracts".to_string(),
                    )])),
                    skill_preflight_policy_mode: Some(SkillPreflightPolicyMode::Warn),
                    model_routing: Some(ModelRoutingConfig {
                        enabled: true,
                        routine_model: Some("gpt-5-mini".to_string()),
                        moderate_model: Some("gpt-5".to_string()),
                        complex_model: Some("gpt-5-pro".to_string()),
                        escalate_on_failure: true,
                    }),
                }
            }

            #[test]
            fn wire_model_ref_alias_round_trips() {
                let model_ref = WireModelRef {
                    provider: "openai".to_string(),
                    model: "gpt-5".to_string(),
                };
                assert_roundtrip(&model_ref);

                let legacy_alias: ModelRef = model_ref.clone();
                assert_eq!(legacy_alias, model_ref);
            }

            #[test]
            fn run_spawn_request_round_trips() {
                let request = RunSpawnRequest {
                    agent_id: Some("coder".to_string()),
                    inline: Some(InlineAgentRunConfig {
                        name: Some("Temp".to_string()),
                        system_prompt: Some("You are focused.".to_string()),
                        allowed_tools: Some(vec!["bash".to_string()]),
                        max_iterations: Some(3),
                    }),
                    task: "Write code".to_string(),
                    timeout_secs: Some(30),
                    max_iterations: Some(5),
                    priority: Some(SpawnPriority::High),
                    model: Some("gpt-5.4-codex".to_string()),
                    model_provider: Some("openai-codex".to_string()),
                    parent_run_id: Some("run-1".to_string()),
                };

                assert_roundtrip(&request);
            }

            #[test]
            fn list_runs_round_trip() {
                let request = IpcRequest::ListRuns {
                    query: RunListQuery {
                        container: ExecutionContainerRef {
                            kind: ExecutionContainerKind::Workspace,
                            id: "workspace".to_string(),
                        },
                    },
                };
                assert_roundtrip(&request);
            }

            #[test]
            fn ipc_request_session_round_trips() {
                let request = IpcRequest::AppendMessage {
                    session_id: "session-1".to_string(),
                    message: ChatMessage {
                        id: "msg-1".to_string(),
                        role: ChatRole::User,
                        content: "hello".to_string(),
                        timestamp: 1,
                        execution: Some(MessageExecution {
                            steps: vec![ExecutionStepInfo {
                                step_type: "tool_call".to_string(),
                                name: "bash".to_string(),
                                status: "completed".to_string(),
                                duration_ms: Some(12),
                            }],
                            duration_ms: 12,
                            tokens_used: 20,
                            cost_usd: Some(0.01),
                            input_tokens: Some(10),
                            output_tokens: Some(10),
                            status: ChatExecutionStatus::Completed,
                        }),
                        media: Some(ChatMessageMedia {
                            media_type: ChatMediaType::Voice,
                            file_path: "/tmp/audio.wav".to_string(),
                            duration_sec: Some(3),
                        }),
                        transcript: Some(ChatMessageTranscript {
                            text: "hello".to_string(),
                            model: Some("whisper-1".to_string()),
                            updated_at: Some(1),
                        }),
                    },
                };
                assert_roundtrip(&request);
            }

            #[test]
            fn ipc_request_agent_round_trips() {
                let request = IpcRequest::BuildAgentSystemPrompt {
                    agent_node: sample_agent_node(),
                };
                assert_roundtrip(&request);
            }
        }
    }

    /// Shared boundary contracts used across transport and app layers.
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use specta::Type;

    pub use request::IpcRequest;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum ErrorKind {
        Validation,
        ConfirmationRequired,
        NotFound,
        Conflict,
        Unauthorized,
        Forbidden,
        RateLimit,
        Timeout,
        Protocol,
        Internal,
    }

    impl ErrorKind {
        pub fn from_code(code: i32) -> Self {
            match code {
                400 => Self::Validation,
                428 => Self::ConfirmationRequired,
                401 => Self::Unauthorized,
                403 => Self::Forbidden,
                404 => Self::NotFound,
                408 | 504 => Self::Timeout,
                409 => Self::Conflict,
                429 => Self::RateLimit,
                code if code < 0 => Self::Protocol,
                _ => Self::Internal,
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct ErrorPayload {
        pub code: i32,
        pub kind: ErrorKind,
        pub message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub details: Option<Value>,
    }

    impl ErrorPayload {
        pub fn new(code: i32, message: impl Into<String>, details: Option<Value>) -> Self {
            Self {
                code,
                kind: ErrorKind::from_code(code),
                message: message.into(),
                details,
            }
        }

        pub fn with_kind(
            code: i32,
            kind: ErrorKind,
            message: impl Into<String>,
            details: Option<Value>,
        ) -> Self {
            Self {
                code,
                kind,
                message: message.into(),
                details,
            }
        }

        pub fn not_found(what: &str) -> Self {
            Self::with_kind(
                404,
                ErrorKind::NotFound,
                format!("{} not found", what),
                None,
            )
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "response_type", content = "data")]
    pub enum ResponseEnvelope<T> {
        Pong,
        Success(T),
        Error(ErrorPayload),
    }

    impl ResponseEnvelope<Value> {
        pub fn success<T: Serialize>(data: T) -> Self {
            match serde_json::to_value(data) {
                Ok(value) => Self::Success(value),
                Err(error) => Self::Error(ErrorPayload::with_kind(
                    500,
                    ErrorKind::Internal,
                    "Failed to serialize response payload",
                    Some(serde_json::json!({ "cause": error.to_string() })),
                )),
            }
        }

        pub fn error(code: i32, message: impl Into<String>) -> Self {
            Self::error_with_details(code, message, None)
        }

        pub fn error_with_details(
            code: i32,
            message: impl Into<String>,
            details: Option<Value>,
        ) -> Self {
            Self::Error(ErrorPayload::new(code, message, details))
        }

        pub fn not_found(what: &str) -> Self {
            Self::Error(ErrorPayload::not_found(what))
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct ToolCallFrame {
        pub id: String,
        pub name: String,
        pub arguments: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct ToolResultFrame {
        pub id: String,
        pub result: String,
        pub success: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "stream_type", content = "data")]
    pub enum StreamEnvelope<TEvent> {
        Start {
            stream_id: String,
        },
        Ack {
            content: String,
        },
        Progress {
            content: String,
        },
        Data {
            content: String,
        },
        ToolCall {
            id: String,
            name: String,
            arguments: Value,
        },
        ToolResult {
            id: String,
            result: String,
            success: bool,
        },
        Event {
            event: TEvent,
        },
        Done {
            total_tokens: Option<u32>,
        },
        Error(ErrorPayload),
    }

    impl<TEvent> StreamEnvelope<TEvent> {
        pub fn error(code: i32, message: impl Into<String>) -> Self {
            Self::Error(ErrorPayload::new(code, message, None))
        }

        pub fn error_with_details(
            code: i32,
            message: impl Into<String>,
            details: Option<Value>,
        ) -> Self {
            Self::Error(ErrorPayload::new(code, message, details))
        }
    }

    impl<TEvent> From<ToolCallFrame> for StreamEnvelope<TEvent> {
        fn from(frame: ToolCallFrame) -> Self {
            Self::ToolCall {
                id: frame.id,
                name: frame.name,
                arguments: frame.arguments,
            }
        }
    }

    impl<TEvent> From<ToolResultFrame> for StreamEnvelope<TEvent> {
        fn from(frame: ToolResultFrame) -> Self {
            Self::ToolResult {
                id: frame.id,
                result: frame.result,
                success: frame.success,
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum ChatSessionEvent {
        Created { session_id: String },
        Updated { session_id: String },
        MessageAdded { session_id: String, source: String },
        Deleted { session_id: String },
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum IpcStreamEvent {
        Session(ChatSessionEvent),
    }

    pub type StreamFrame = StreamEnvelope<IpcStreamEvent>;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum ExecutionScope {
        Foreground {
            client_id: String,
            terminal_id: String,
        },
        Subagent {
            parent_run_id: String,
        },
    }

    impl ExecutionScope {
        pub fn foreground(client_id: impl Into<String>, terminal_id: impl Into<String>) -> Self {
            Self::Foreground {
                client_id: client_id.into(),
                terminal_id: terminal_id.into(),
            }
        }

        pub fn subagent(parent_run_id: impl Into<String>) -> Self {
            Self::Subagent {
                parent_run_id: parent_run_id.into(),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct DeleteResponse {
        pub deleted: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct DeleteWithIdResponse {
        pub id: String,
        pub deleted: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct ArchiveResponse {
        pub archived: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct ClearResponse {
        pub deleted: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct CancelResponse {
        pub canceled: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct SteerResponse {
        pub steered: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct ApprovalHandledResponse {
        pub handled: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct OkResponse {
        pub ok: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct IdResponse {
        pub id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct ApiKeyResponse {
        pub api_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub profile_id: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct PromptResponse {
        pub prompt: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct SecretResponse {
        pub value: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct CleanupReportResponse {
        pub chat_sessions: usize,
        pub daemon_log_files: usize,
    }

    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum DaemonRuntimeStatus {
        Running,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct IpcDaemonStatus {
        pub status: DaemonRuntimeStatus,
        pub protocol_version: String,
        pub daemon_version: String,
        pub pid: u32,
        pub started_at_ms: i64,
        pub uptime_secs: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub enum ToolErrorCategory {
        Network,
        Auth,
        Config,
        Execution,
        RateLimit,
        NotFound,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct ToolDefinition {
        pub name: String,
        pub description: String,
        pub parameters: Value,
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::Value;

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(rename_all = "snake_case")]
        enum TestEvent {
            Session,
        }

        struct FailingSerialize;

        impl Serialize for FailingSerialize {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("boom"))
            }
        }

        fn assert_roundtrip<T>(value: &T)
        where
            T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
        {
            let json = serde_json::to_string(value).unwrap();
            let decoded: T = serde_json::from_str(&json).unwrap();
            assert_eq!(&decoded, value);
        }

        #[test]
        fn public_contract_exports_are_accessible() {
            let _ = ErrorPayload::new(500, "boom", None);
            let _ = ResponseEnvelope::<Value>::Pong;
            let _ = StreamEnvelope::<()>::Done { total_tokens: None };
            let _ = OkResponse { ok: true };
            let _ = IpcRequest::Ping;
            let _ = IpcDaemonStatus {
                status: DaemonRuntimeStatus::Running,
                protocol_version: "2".to_string(),
                daemon_version: "0.4.0".to_string(),
                pid: 1,
                started_at_ms: 0,
                uptime_secs: 0,
            };
        }

        #[test]
        fn error_payload_round_trips() {
            let payload = ErrorPayload::with_kind(
                500,
                ErrorKind::Internal,
                "failed",
                Some(serde_json::json!({ "field": "agent_id" })),
            );

            assert_roundtrip(&payload);
        }

        #[test]
        fn error_kind_maps_from_code() {
            assert_eq!(ErrorKind::from_code(404), ErrorKind::NotFound);
            assert_eq!(ErrorKind::from_code(429), ErrorKind::RateLimit);
            assert_eq!(ErrorKind::from_code(428), ErrorKind::ConfirmationRequired);
            assert_eq!(ErrorKind::from_code(-2), ErrorKind::Protocol);
        }

        #[test]
        fn response_success_round_trips() {
            let response = ResponseEnvelope::<Value>::success(serde_json::json!({
                "deleted": true
            }));

            let encoded = serde_json::to_string(&response).unwrap();
            let decoded: ResponseEnvelope<Value> = serde_json::from_str(&encoded).unwrap();

            assert_eq!(decoded, response);
            assert!(encoded.contains("response_type"));
        }

        #[test]
        fn response_error_round_trips() {
            let response = ResponseEnvelope::<Value>::error_with_details(
                500,
                "failed",
                Some(serde_json::json!({ "error_kind": "session" })),
            );

            assert_roundtrip(&response);
        }

        #[test]
        fn response_success_serialization_failure_returns_error_payload() {
            let response = ResponseEnvelope::<Value>::success(FailingSerialize);

            match response {
                ResponseEnvelope::Error(error) => {
                    assert_eq!(error.code, 500);
                    assert_eq!(error.kind, ErrorKind::Internal);
                    assert_eq!(error.message, "Failed to serialize response payload");
                    assert_eq!(error.details.unwrap()["cause"], "boom");
                }
                other => panic!("unexpected response variant: {other:?}"),
            }
        }

        #[test]
        fn stream_frames_round_trip() {
            let frames = vec![
                StreamEnvelope::<TestEvent>::Start {
                    stream_id: "stream-1".to_string(),
                },
                StreamEnvelope::<TestEvent>::Ack {
                    content: "ack".to_string(),
                },
                StreamEnvelope::<TestEvent>::Progress {
                    content: "progress".to_string(),
                },
                StreamEnvelope::<TestEvent>::Data {
                    content: "data".to_string(),
                },
                StreamEnvelope::<TestEvent>::ToolCall {
                    id: "call-1".to_string(),
                    name: "search".to_string(),
                    arguments: serde_json::json!({ "q": "restflow" }),
                },
                StreamEnvelope::<TestEvent>::ToolResult {
                    id: "call-1".to_string(),
                    result: "done".to_string(),
                    success: true,
                },
                StreamEnvelope::<TestEvent>::Event {
                    event: TestEvent::Session,
                },
                StreamEnvelope::<TestEvent>::Done {
                    total_tokens: Some(12),
                },
                StreamEnvelope::<TestEvent>::error_with_details(
                    500,
                    "boom",
                    Some(serde_json::json!({ "scope": "stream" })),
                ),
            ];

            for frame in frames {
                assert_roundtrip(&frame);
            }
        }

        #[test]
        fn operation_responses_round_trip() {
            assert_roundtrip(&IdResponse {
                id: "memory-1".to_string(),
            });
            assert_roundtrip(&DeleteResponse { deleted: true });
            assert_roundtrip(&DeleteWithIdResponse {
                id: "task-1".to_string(),
                deleted: true,
            });
            assert_roundtrip(&ArchiveResponse { archived: true });
            assert_roundtrip(&ClearResponse { deleted: 3 });
            assert_roundtrip(&CancelResponse { canceled: true });
            assert_roundtrip(&SteerResponse { steered: true });
            assert_roundtrip(&ApprovalHandledResponse { handled: false });
            assert_roundtrip(&OkResponse { ok: true });
            assert_roundtrip(&SecretResponse {
                value: Some("token".to_string()),
            });
            assert_roundtrip(&ApiKeyResponse {
                api_key: "key".to_string(),
                profile_id: Some("profile-1".to_string()),
            });
            assert_roundtrip(&PromptResponse {
                prompt: "hello".to_string(),
            });
            assert_roundtrip(&IpcDaemonStatus {
                status: DaemonRuntimeStatus::Running,
                protocol_version: "2".to_string(),
                daemon_version: "0.4.0".to_string(),
                pid: 42,
                started_at_ms: 123,
                uptime_secs: 456,
            });
            assert_roundtrip(&CleanupReportResponse {
                chat_sessions: 1,
                daemon_log_files: 4,
            });
        }

        #[test]
        fn tool_definition_contract_round_trips() {
            assert_roundtrip(&ToolDefinition {
                name: "search".to_string(),
                description: "Search documents".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    }
                }),
            });
        }
    }
}

// ── Top-level re-exports ─────────────────────────────────────────────

// Error types
pub use error::{
    Result as ToolResult, SESSION_NOT_FOUND, ToolError, ValidationError, ValidationErrorResponse,
    encode_validation_error, session_not_found_message,
};

// Assessment types
pub use agent::{
    AgentMeta, AgentNode, AgentType, ApiKeyConfig, CodexCliExecutionMode, ModelRoutingConfig,
    SkillPreflightPolicyMode,
};
pub use run::{
    ExecutionContainerKind, ExecutionContainerRef, ExecutionContainerSummary, RunKind,
    RunListQuery, RunStatus, RunSummary,
};
pub use session::{
    ChatExecutionStatus, ChatMediaType, ChatMessage, ChatMessageMedia, ChatMessageTranscript,
    ChatRole, ChatSession, ChatSessionMetadata, ChatSessionSummary, ChatSessionUpdate, ChatTurn,
    ChatTurnEvent, ChatTurnEventKind, ChatTurnStatus, ExecutionStepInfo, MessageExecution,
};

// Tool trait and core types
pub use tool::{
    SecretResolver, SecurityDecision, SecurityGate, Tool, ToolAction, ToolErrorCategory,
    ToolOutput, ToolSchema, check_security,
};

// Registry and toolset
pub use toolset::{
    FilteredToolset, RateLimitWrapper, TimeoutWrapper, ToolPredicate, ToolRegistry, ToolWrapper,
    Toolset, ToolsetContext, WrappedTool,
};

// Skill types
pub use skill::{
    Skill, SkillContent, SkillFrontmatter, SkillGating, SkillInfo, SkillMeta, SkillProvider,
    SkillReference, SkillScript, SkillSource, SkillStatus,
};

// Store traits
pub use store::{
    AgentCreateRequest, AgentStore, AgentUpdateRequest, ConfigStore, OpsProvider, ReplySender,
    SecretStore, SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore,
};

pub use orchestrator::{AgentOrchestrator, ExecutionMode, ExecutionOutcome, ExecutionPlan};

// Sub-agent types
pub use subagent::{
    ContractRunSpawnRequest, InlineRunConfig, InlineSubagentConfig, SpawnHandle, SpawnPriority,
    SpawnRequest, SubagentCompletion, SubagentConfig, SubagentDefLookup, SubagentDefSnapshot,
    SubagentDefSummary, SubagentEffectiveLimits, SubagentLimitSource, SubagentManager,
    SubagentResult, SubagentState, SubagentStatus, resolve_agent_id, spawn_request_from_contract,
};

// LLM switching
pub use llm::{ClientKind, LlmProvider, LlmSwitcher, SwapResult};

// Shared model/provider normalization
pub use model::{ModelMetadata, ModelMetadataDTO, ModelProvider, ModelRef};
pub use model_id::ModelId;
pub use provider::{
    ALL_PROVIDER_META, Provider, ProviderMeta, ProviderSelector, parse_model_reference,
    parse_provider_selector, provider_meta, resolve_available_model_name,
    split_provider_qualified_model,
};

/// Runtime model specification consumed by the LLM factory.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub name: String,
    pub provider: LlmProvider,
    pub client_model: String,
    /// Override the provider's default base URL for this specific model.
    pub base_url: Option<String>,
    pub client_kind: ClientKind,
}

impl ModelSpec {
    pub fn new(
        name: impl Into<String>,
        provider: LlmProvider,
        client_model: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            provider,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::Http,
        }
    }

    /// Set a custom base URL override for this model.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn codex(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::OpenAI,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::CodexCli,
        }
    }

    pub fn opencode(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::OpenAI,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::OpenCodeCli,
        }
    }

    pub fn gemini_cli(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::Google,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::GeminiCli,
        }
    }

    pub fn claude_code(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::Anthropic,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::ClaudeCodeCli,
        }
    }

    pub fn is_codex_cli(&self) -> bool {
        self.client_kind == ClientKind::CodexCli
    }

    pub fn is_opencode_cli(&self) -> bool {
        self.client_kind == ClientKind::OpenCodeCli
    }

    pub fn is_gemini_cli(&self) -> bool {
        self.client_kind == ClientKind::GeminiCli
    }

    pub fn is_cli(&self) -> bool {
        self.client_kind.is_cli()
    }
}

// Shared steer/runtime control types
pub use steer::{SteerCommand, SteerMessage, SteerSource};

// Shared transport and IPC contracts
// These are workspace-internal daemon contracts. The crates that expose them
// are `publish = false`; the installed public surface remains the `restflow`
// CLI, not these Rust transport types.
pub use contracts::request;
pub use contracts::{
    ApiKeyResponse, ApprovalHandledResponse, ArchiveResponse, CancelResponse, ChatSessionEvent,
    CleanupReportResponse, ClearResponse, DaemonRuntimeStatus, DeleteResponse,
    DeleteWithIdResponse, ErrorKind, ErrorPayload, ExecutionScope, IdResponse, IpcDaemonStatus,
    IpcRequest, IpcStreamEvent, OkResponse, PromptResponse, ResponseEnvelope, SecretResponse,
    SteerResponse, StreamEnvelope, StreamFrame, ToolDefinition,
};

// Shared default constants
pub use defaults::{
    DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS, DEFAULT_AGENT_BASH_TIMEOUT_SECS,
    DEFAULT_AGENT_BROWSER_TIMEOUT_SECS, DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS,
    DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS, DEFAULT_AGENT_LLM_TIMEOUT_SECS,
    DEFAULT_AGENT_MAX_ITERATIONS, DEFAULT_AGENT_MAX_TOOL_CALLS, DEFAULT_AGENT_MAX_TOOL_CONCURRENCY,
    DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH, DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS,
    DEFAULT_AGENT_PYTHON_TIMEOUT_SECS, DEFAULT_AGENT_TOOL_TIMEOUT_SECS,
    DEFAULT_API_WEB_SEARCH_RESULTS, DEFAULT_CHAT_MAX_SESSION_HISTORY,
    DEFAULT_GITHUB_CACHE_TTL_SECS, DEFAULT_MARKETPLACE_CACHE_TTL_SECS,
    DEFAULT_MAX_PARALLEL_SUBAGENTS, DEFAULT_SUBAGENT_MAX_DEPTH, DEFAULT_SUBAGENT_TIMEOUT_SECS,
    DEFAULT_WORKSPACE_CONTEXT_MAX_FILE_BYTES, DEFAULT_WORKSPACE_CONTEXT_MAX_TOTAL_BYTES,
    MAX_API_WEB_SEARCH_RESULTS,
};
