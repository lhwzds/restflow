//! Shared model/provider primitives used by runtime, core, and tools.

/// Concrete execution path used to satisfy an LLM request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientKind {
    Http,
    CodexCli,
    OpenCodeCli,
    GeminiCli,
}

impl ClientKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::CodexCli => "codex-cli",
            Self::OpenCodeCli => "opencode-cli",
            Self::GeminiCli => "gemini-cli",
        }
    }

    pub fn is_cli(&self) -> bool {
        !matches!(self, Self::Http)
    }
}

/// Runtime provider bucket used by the LLM factory layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmProvider {
    OpenAI,
    Anthropic,
    DeepSeek,
    Google,
    Groq,
    OpenRouter,
    XAI,
    Qwen,
    Zai,
    ZaiCodingPlan,
    Moonshot,
    Doubao,
    Yi,
    SiliconFlow,
    MiniMax,
    MiniMaxCodingPlan,
}

impl LlmProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::DeepSeek => "deepseek",
            Self::Google => "google",
            Self::Groq => "groq",
            Self::OpenRouter => "openrouter",
            Self::XAI => "xai",
            Self::Qwen => "qwen",
            Self::Zai => "zai",
            Self::ZaiCodingPlan => "zai-coding-plan",
            Self::Moonshot => "moonshot",
            Self::Doubao => "doubao",
            Self::Yi => "yi",
            Self::SiliconFlow => "siliconflow",
            Self::MiniMax => "minimax",
            Self::MiniMaxCodingPlan => "minimax-coding-plan",
        }
    }

    pub fn base_url(&self) -> &'static str {
        match self {
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Anthropic => "",
            Self::DeepSeek => "https://api.deepseek.com/v1",
            Self::Google => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::XAI => "https://api.x.ai/v1",
            Self::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1",
            Self::Zai => "https://api.z.ai/api/paas/v4",
            Self::ZaiCodingPlan => "https://api.z.ai/api/coding/paas/v4",
            Self::Moonshot => "https://api.moonshot.cn/v1",
            Self::Doubao => "https://ark.cn-beijing.volces.com/api/v3",
            Self::Yi => "https://api.lingyiwanwu.com/v1",
            Self::SiliconFlow => "https://api.siliconflow.cn/v1",
            Self::MiniMax => "https://api.minimax.io",
            Self::MiniMaxCodingPlan => "https://api.minimax.io",
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{ClientKind, LlmProvider, ModelSpec};

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
}
