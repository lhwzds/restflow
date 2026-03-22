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
    /// Previous provider name.
    pub previous_provider: String,
    /// Previous model name.
    pub previous_model: String,
    /// New provider name.
    pub new_provider: String,
    /// New model name.
    pub new_model: String,
}

/// Runtime LLM model switching.
///
/// Abstracts `SwappableLlm` + `LlmClientFactory` so that tool implementations
/// can switch models without depending on the concrete AI framework.
pub trait LlmSwitcher: Send + Sync {
    /// Current model name.
    fn current_model(&self) -> String;

    /// Current provider name.
    fn current_provider(&self) -> String;

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
                new_provider: "openai".to_string(),
                new_model: model.to_string(),
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
}
