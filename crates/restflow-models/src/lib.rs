//! Shared model/provider primitives used by runtime, core, and tools.

pub mod catalog;
mod model_id;
mod model_metadata;
mod provider;
mod provider_meta;
mod selector;

pub use model_id::ModelId;
pub use model_metadata::{ModelMetadata, ModelMetadataDTO};
pub use provider::Provider;
pub use restflow_traits::{ClientKind, LlmProvider};
pub use selector::{
    ProviderSelector, parse_model_reference, parse_provider_selector, resolve_available_model_name,
    split_provider_qualified_model,
};

pub use provider_meta::{ALL_PROVIDER_META, ProviderMeta, provider_meta};

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

    pub fn is_claude_code_cli(&self) -> bool {
        self.client_kind == ClientKind::ClaudeCodeCli
    }

    pub fn is_cli(&self) -> bool {
        self.client_kind.is_cli()
    }
}

#[cfg(test)]
mod tests {
    use restflow_traits::ModelProvider;
    use ts_rs::TS;

    use super::{
        ALL_PROVIDER_META, ClientKind, LlmProvider, ModelId, ModelMetadataDTO, ModelSpec, Provider,
        provider_meta,
    };

    #[test]
    fn provider_base_urls_are_stable() {
        assert_eq!(LlmProvider::OpenAI.base_url(), "https://api.openai.com/v1");
        assert_eq!(LlmProvider::Zai.base_url(), "https://api.z.ai/api/paas/v4");
        assert_eq!(
            LlmProvider::ZaiCodingPlan.base_url(),
            "https://api.z.ai/api/coding/paas/v4"
        );
    }

    #[test]
    fn model_spec_builders_mark_cli_variants() {
        let codex = ModelSpec::codex("gpt-5.3-codex", "gpt-5.3-codex");
        assert_eq!(codex.client_kind, ClientKind::CodexCli);
        assert!(codex.is_codex_cli());
        assert!(codex.is_cli());

        let opencode = ModelSpec::opencode("opencode-cli", "opencode-cli");
        assert_eq!(opencode.client_kind, ClientKind::OpenCodeCli);
        assert!(opencode.is_opencode_cli());
        assert!(opencode.is_cli());

        let gemini = ModelSpec::gemini_cli("gemini-cli", "gemini-cli");
        assert_eq!(gemini.client_kind, ClientKind::GeminiCli);
        assert!(gemini.is_gemini_cli());
        assert!(gemini.is_cli());

        let claude_code = ModelSpec::claude_code("claude-code-opus", "opus");
        assert_eq!(claude_code.client_kind, ClientKind::ClaudeCodeCli);
        assert!(claude_code.is_claude_code_cli());
        assert!(claude_code.is_cli());
    }

    #[test]
    fn new_model_specs_default_to_http_execution() {
        let spec = ModelSpec::new("gpt-5", LlmProvider::OpenAI, "gpt-5");
        assert_eq!(spec.client_kind, ClientKind::Http);
        assert!(!spec.is_cli());
    }

    #[test]
    fn model_spec_with_base_url_overrides_provider_default() {
        let spec = ModelSpec::new("glm-5", LlmProvider::Zai, "glm-5")
            .with_base_url("https://example.invalid");
        assert_eq!(spec.base_url.as_deref(), Some("https://example.invalid"));
    }

    #[test]
    fn provider_meta_exposes_runtime_provider_and_env() {
        let google = provider_meta(ModelProvider::Google);
        assert_eq!(google.runtime_provider, LlmProvider::Google);
        assert_eq!(google.api_key_env, Some("GEMINI_API_KEY"));
        assert_eq!(google.api_key_env_aliases, &["GOOGLE_API_KEY"]);
        assert_eq!(google.default_model_id, ModelId::Gemini25Pro);
        assert_eq!(google.models_dev_provider_ids, &["google"]);

        let claude_code = provider_meta(ModelProvider::ClaudeCode);
        assert_eq!(claude_code.runtime_provider, LlmProvider::Anthropic);
        assert_eq!(claude_code.api_key_env, None);
        assert_eq!(claude_code.default_model_id, ModelId::ClaudeCodeOpus);
        assert_eq!(
            claude_code.models_dev_provider_ids,
            &["claude-code", "anthropic"]
        );
    }

    #[test]
    fn provider_meta_exposes_models_dev_aliases() {
        assert_eq!(
            provider_meta(ModelProvider::Qwen).models_dev_provider_ids,
            &["alibaba-cn", "alibaba"]
        );
        assert_eq!(
            provider_meta(ModelProvider::Moonshot).models_dev_provider_ids,
            &["moonshotai", "moonshotai-cn", "kimi-for-coding"]
        );
        assert_eq!(
            provider_meta(ModelProvider::MiniMaxCodingPlan).default_model_id,
            ModelId::MiniMaxM25CodingPlan
        );
    }

    #[test]
    fn provider_meta_catalog_stays_in_sync_with_model_provider() {
        assert_eq!(ALL_PROVIDER_META.len(), 18);
        assert_eq!(
            provider_meta(ModelProvider::MiniMaxCodingPlan).canonical_name(),
            "minimax-coding-plan"
        );
    }

    #[test]
    fn glm_5_1_coding_plan_is_recognized_by_api_and_serialized_names() {
        assert_eq!(
            ModelId::from_api_name("glm-5.1"),
            Some(ModelId::Glm5_1CodingPlan)
        );
        assert_eq!(
            ModelId::from_serialized_str("zai-coding-plan-glm-5-1"),
            Some(ModelId::Glm5_1CodingPlan)
        );
        assert_eq!(
            ModelId::normalize_model_id_for_provider(Provider::ZaiCodingPlan, "glm-5.1").as_deref(),
            Some("zai-coding-plan-glm-5-1")
        );
        assert_eq!(
            provider_meta(ModelProvider::ZaiCodingPlan).default_model_id,
            ModelId::Glm5_1CodingPlan
        );
    }

    #[test]
    fn export_bindings_provider() {
        Provider::export_to_string(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_model_id() {
        ModelId::export_to_string(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_model_metadata_dto() {
        ModelMetadataDTO::export_to_string(&ts_rs::Config::default()).unwrap();
    }
}
