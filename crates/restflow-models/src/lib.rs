//! Shared model/provider primitives used by runtime, core, and tools.

mod provider_meta;

pub use provider_meta::{ALL_PROVIDER_META, ProviderMeta, provider_meta};

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
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::CodexCli => "codex-cli",
            Self::OpenCodeCli => "opencode-cli",
            Self::GeminiCli => "gemini-cli",
            Self::ClaudeCodeCli => "claude-code-cli",
        }
    }

    pub fn is_cli(&self) -> bool {
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
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            pub fn base_url(&self) -> &'static str {
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

    use super::{ALL_PROVIDER_META, ClientKind, LlmProvider, ModelSpec, provider_meta};

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
        assert_eq!(google.default_model_id, "gemini-2-5-pro");
        assert_eq!(google.models_dev_provider_ids, &["google"]);

        let claude_code = provider_meta(ModelProvider::ClaudeCode);
        assert_eq!(claude_code.runtime_provider, LlmProvider::Anthropic);
        assert_eq!(claude_code.api_key_env, None);
        assert_eq!(claude_code.default_model_id, "claude-code-opus");
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
            "minimax-coding-plan-m2-5"
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
}
