use crate::models::{ModelId, ModelRef, Provider};
use restflow_contracts::request::WireModelRef;
use restflow_models::{LlmProvider, catalog};

#[test]
fn test_provider() {
    assert_eq!(ModelId::Gpt5.provider(), Provider::OpenAI);
    assert_eq!(ModelId::ClaudeSonnet4_5.provider(), Provider::Anthropic);
    assert_eq!(ModelId::ClaudeCodeSonnet.provider(), Provider::ClaudeCode);
    assert_eq!(ModelId::DeepseekChat.provider(), Provider::DeepSeek);
    assert_eq!(ModelId::Gemini25Pro.provider(), Provider::Google);
    assert_eq!(ModelId::GroqLlama4Scout.provider(), Provider::Groq);
    assert_eq!(ModelId::Grok4.provider(), Provider::XAI);
    assert_eq!(ModelId::Qwen3Max.provider(), Provider::Qwen);
    assert_eq!(ModelId::Glm4_7.provider(), Provider::Zai);
    assert_eq!(ModelId::Glm5Turbo.provider(), Provider::Zai);
    assert_eq!(ModelId::Glm5CodingPlan.provider(), Provider::ZaiCodingPlan);
    assert_eq!(
        ModelId::Glm5TurboCodingPlan.provider(),
        Provider::ZaiCodingPlan
    );
    assert_eq!(ModelId::KimiK2_5.provider(), Provider::Moonshot);
    assert_eq!(ModelId::DoubaoPro.provider(), Provider::Doubao);
    assert_eq!(ModelId::YiLightning.provider(), Provider::Yi);
    assert_eq!(ModelId::MiniMaxM25.provider(), Provider::MiniMax);
    assert_eq!(ModelId::MiniMaxM21.provider(), Provider::MiniMax);
    assert_eq!(ModelId::MiniMaxM27.provider(), Provider::MiniMax);
    assert_eq!(ModelId::MiniMaxM27Highspeed.provider(), Provider::MiniMax);
    assert_eq!(
        ModelId::MiniMaxM25CodingPlan.provider(),
        Provider::MiniMaxCodingPlan
    );
    assert_eq!(
        ModelId::MiniMaxM25CodingPlanHighspeed.provider(),
        Provider::MiniMaxCodingPlan
    );
    assert_eq!(ModelId::CodexCli.provider(), Provider::Codex);
    assert_eq!(ModelId::Gpt5_4Codex.provider(), Provider::Codex);
    assert_eq!(ModelId::Gpt5_4MiniCodex.provider(), Provider::Codex);
}

#[test]
fn test_supports_temperature() {
    // Models that don't support temperature
    assert!(!ModelId::Gpt5.supports_temperature());
    assert!(!ModelId::Gpt5Mini.supports_temperature());
    assert!(!ModelId::Gpt5_1.supports_temperature());
    assert!(!ModelId::Gpt5_2.supports_temperature());
    assert!(!ModelId::Gpt5Codex.supports_temperature());
    assert!(!ModelId::Gpt5_4Codex.supports_temperature());
    assert!(!ModelId::Gpt5_4MiniCodex.supports_temperature());
    assert!(!ModelId::Gpt5_1Codex.supports_temperature());
    assert!(!ModelId::Gpt5_2Codex.supports_temperature());
    assert!(!ModelId::CodexCli.supports_temperature());
    assert!(!ModelId::OpenCodeCli.supports_temperature());
    assert!(!ModelId::GeminiCli.supports_temperature());

    // Models that support temperature
    assert!(ModelId::ClaudeSonnet4_5.supports_temperature());
    assert!(ModelId::ClaudeHaiku4_5.supports_temperature());
    assert!(ModelId::DeepseekChat.supports_temperature());
    assert!(ModelId::Gemini25Flash.supports_temperature());
    assert!(ModelId::GroqLlama4Maverick.supports_temperature());
}

#[test]
fn test_is_codex_cli() {
    assert!(ModelId::Gpt5Codex.is_codex_cli());
    assert!(ModelId::Gpt5_4Codex.is_codex_cli());
    assert!(ModelId::Gpt5_4MiniCodex.is_codex_cli());
    assert!(ModelId::Gpt5_1Codex.is_codex_cli());
    assert!(ModelId::Gpt5_2Codex.is_codex_cli());
    assert!(ModelId::CodexCli.is_codex_cli());
    assert!(!ModelId::Gpt5.is_codex_cli());
}

#[test]
fn test_is_opencode_cli() {
    assert!(ModelId::OpenCodeCli.is_opencode_cli());
    assert!(!ModelId::Gpt5.is_opencode_cli());
}

#[test]
fn test_is_gemini_cli() {
    assert!(ModelId::GeminiCli.is_gemini_cli());
    assert!(!ModelId::Gpt5.is_gemini_cli());
}

#[test]
fn test_as_str() {
    assert_eq!(ModelId::Gpt5.as_str(), "gpt-5");
    assert_eq!(ModelId::Gpt5_1.as_str(), "gpt-5.1");
    assert_eq!(ModelId::ClaudeSonnet4_5.as_str(), "claude-sonnet-4-5");
    assert_eq!(ModelId::ClaudeHaiku4_5.as_str(), "claude-haiku-4-5");
    assert_eq!(ModelId::Gpt5Codex.as_str(), "gpt-5-codex");
    assert_eq!(ModelId::Gpt5_4Codex.as_str(), "gpt-5.4");
    assert_eq!(ModelId::Gpt5_4MiniCodex.as_str(), "gpt-5.4-mini");
    assert_eq!(ModelId::Gpt5_1Codex.as_str(), "gpt-5.1-codex");
    assert_eq!(ModelId::Gpt5_2Codex.as_str(), "gpt-5.2-codex");
    assert_eq!(ModelId::CodexCli.as_str(), "gpt-5.3-codex");
    assert_eq!(ModelId::OpenCodeCli.as_str(), "opencode");
    assert_eq!(ModelId::GeminiCli.as_str(), "gemini-2.5-pro");
    assert_eq!(ModelId::MiniMaxM21.as_str(), "MiniMax-M2.1");
    assert_eq!(ModelId::MiniMaxM27.as_str(), "MiniMax-M2.7");
    assert_eq!(
        ModelId::MiniMaxM27Highspeed.as_str(),
        "MiniMax-M2.7-highspeed"
    );
    assert_eq!(ModelId::MiniMaxM21CodingPlan.as_str(), "MiniMax-M2.1");
    assert_eq!(ModelId::MiniMaxM25CodingPlan.as_str(), "MiniMax-M2.5");
    assert_eq!(
        ModelId::MiniMaxM25CodingPlanHighspeed.as_str(),
        "MiniMax-M2.5-highspeed"
    );
    assert_eq!(ModelId::Glm5Turbo.as_str(), "glm-5-turbo");
    assert_eq!(ModelId::Glm5Code.as_str(), "glm-5");
    assert_eq!(ModelId::Glm5CodingPlan.as_str(), "glm-5");
    assert_eq!(ModelId::Glm5TurboCodingPlan.as_str(), "glm-5-turbo");
    assert_eq!(ModelId::DeepseekChat.as_str(), "deepseek-chat");
    assert_eq!(ModelId::Gemini25Pro.as_str(), "gemini-2.5-pro");
    assert_eq!(
        ModelId::GroqLlama4Scout.as_str(),
        "meta-llama/llama-4-scout-17b-16e-instruct"
    );
}

#[test]
fn test_from_api_name() {
    assert_eq!(
        ModelId::from_api_name("gpt-5.4-codex"),
        Some(ModelId::Gpt5_4Codex)
    );
    assert_eq!(ModelId::from_api_name("gpt-5-1"), Some(ModelId::Gpt5_1));
    assert_eq!(ModelId::from_api_name("gpt-5-2"), Some(ModelId::Gpt5_2));
    assert_eq!(
        ModelId::from_api_name("gpt-5.4-mini-codex"),
        Some(ModelId::Gpt5_4MiniCodex)
    );
    assert_eq!(
        ModelId::from_api_name("claude-sonnet-4-5-20250514"),
        Some(ModelId::ClaudeSonnet4_5)
    );
    assert_eq!(
        ModelId::from_api_name("claude-sonnet-4-20250514"),
        Some(ModelId::ClaudeSonnet4_5)
    );
    assert_eq!(
        ModelId::from_api_name("claude-opus-4-latest"),
        Some(ModelId::ClaudeOpus4_6)
    );
    assert_eq!(
        ModelId::from_api_name("claude-haiku-4-preview"),
        Some(ModelId::ClaudeHaiku4_5)
    );
    assert_eq!(
        ModelId::from_api_name("claude-code-opus"),
        Some(ModelId::ClaudeCodeOpus)
    );
    assert_eq!(
        ModelId::from_api_name("gemini-pro"),
        Some(ModelId::Gemini25Pro)
    );
    assert_eq!(
        ModelId::from_api_name("gemini-cli"),
        Some(ModelId::GeminiCli)
    );
    assert_eq!(
        ModelId::from_api_name("groq-scout"),
        Some(ModelId::GroqLlama4Scout)
    );
    assert_eq!(ModelId::from_api_name("grok4"), Some(ModelId::Grok4));
    assert_eq!(ModelId::from_api_name("qwen"), Some(ModelId::Qwen3Max));
    assert_eq!(ModelId::from_api_name("glm-4-7"), Some(ModelId::Glm4_7));
    assert_eq!(ModelId::from_api_name("moonshot"), Some(ModelId::KimiK2_5));
    assert_eq!(ModelId::from_api_name("doubao"), Some(ModelId::DoubaoPro));
    assert_eq!(ModelId::from_api_name("yi"), Some(ModelId::YiLightning));
    assert_eq!(
        ModelId::from_api_name("openrouter"),
        Some(ModelId::OpenRouterAuto)
    );
    assert_eq!(
        ModelId::from_api_name("siliconflow"),
        Some(ModelId::SiliconFlowAuto)
    );
    assert_eq!(ModelId::from_api_name("nonexistent"), None);
}

#[test]
fn test_for_provider_and_model() {
    assert_eq!(
        ModelId::for_provider_and_model(Provider::MiniMax, "minimax-m2-5"),
        Some(ModelId::MiniMaxM25)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::MiniMax, "minimax-m2.7"),
        Some(ModelId::MiniMaxM27)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::MiniMax, "minimax-m2-7-highspeed"),
        Some(ModelId::MiniMaxM27Highspeed)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::MiniMaxCodingPlan, "minimax-m2-5"),
        Some(ModelId::MiniMaxM25CodingPlan)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::MiniMaxCodingPlan, "minimax-m2.5-highspeed"),
        Some(ModelId::MiniMaxM25CodingPlanHighspeed)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::ZaiCodingPlan, "glm-5"),
        Some(ModelId::Glm5CodingPlan)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::Zai, "glm-5-turbo"),
        Some(ModelId::Glm5Turbo)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::ZaiCodingPlan, "glm5-turbo"),
        Some(ModelId::Glm5TurboCodingPlan)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::Codex, "gpt-5.4-codex"),
        Some(ModelId::Gpt5_4Codex)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::Codex, "gpt-5.4-mini-codex"),
        Some(ModelId::Gpt5_4MiniCodex)
    );
    assert_eq!(
        ModelId::for_provider_and_model(Provider::Anthropic, "claude-sonnet-4-preview"),
        Some(ModelId::ClaudeSonnet4_5)
    );
}

#[test]
fn test_remap_provider() {
    assert_eq!(
        ModelId::MiniMaxM25.remap_provider(Provider::MiniMaxCodingPlan),
        Some(ModelId::MiniMaxM25CodingPlan)
    );
    assert_eq!(
        ModelId::MiniMaxM27.remap_provider(Provider::MiniMaxCodingPlan),
        Some(ModelId::MiniMaxM27CodingPlan)
    );
    assert_eq!(
        ModelId::MiniMaxM25CodingPlanHighspeed.remap_provider(Provider::MiniMax),
        None
    );
    assert_eq!(
        ModelId::Glm5CodingPlan.remap_provider(Provider::Zai),
        Some(ModelId::Glm5)
    );
    assert_eq!(
        ModelId::Glm5Turbo.remap_provider(Provider::ZaiCodingPlan),
        Some(ModelId::Glm5TurboCodingPlan)
    );
    assert_eq!(
        ModelId::ClaudeSonnet4_5.remap_provider(Provider::MiniMax),
        None
    );
}

#[test]
fn test_display_name() {
    assert_eq!(ModelId::Gpt5.display_name(), "GPT-5");
    assert_eq!(ModelId::Gpt5_2.display_name(), "GPT-5.2");
    assert_eq!(ModelId::ClaudeSonnet4_5.display_name(), "Claude Sonnet 4.5");
    assert_eq!(ModelId::ClaudeHaiku4_5.display_name(), "Claude Haiku 4.5");
    assert_eq!(ModelId::Gpt5Codex.display_name(), "Codex GPT-5");
    assert_eq!(ModelId::Gpt5_4Codex.display_name(), "GPT-5.4");
    assert_eq!(ModelId::Gpt5_4MiniCodex.display_name(), "GPT-5.4 Mini");
    assert_eq!(ModelId::Gpt5_1Codex.display_name(), "Codex GPT-5.1");
    assert_eq!(ModelId::Gpt5_2Codex.display_name(), "Codex GPT-5.2");
    assert_eq!(ModelId::CodexCli.display_name(), "Codex GPT-5.3");
    assert_eq!(ModelId::OpenCodeCli.display_name(), "OpenCode CLI");
    assert_eq!(ModelId::GeminiCli.display_name(), "Gemini CLI");
    assert_eq!(ModelId::DeepseekChat.display_name(), "DeepSeek Chat");
    assert_eq!(ModelId::MiniMaxM21.display_name(), "MiniMax M2.1");
    assert_eq!(ModelId::MiniMaxM27.display_name(), "MiniMax M2.7");
    assert_eq!(
        ModelId::MiniMaxM25CodingPlanHighspeed.display_name(),
        "MiniMax M2.5 Highspeed (Coding Plan)"
    );
    assert_eq!(ModelId::Glm5Turbo.display_name(), "GLM-5 Turbo");
}

#[test]
fn test_all_models() {
    let models = ModelId::all();
    assert_eq!(models.len(), 66);
    assert!(models.contains(&ModelId::Gpt5));
    assert!(models.contains(&ModelId::Gpt5_1));
    assert!(models.contains(&ModelId::ClaudeOpus4_6));
    assert!(models.contains(&ModelId::ClaudeSonnet4_5));
    assert!(models.contains(&ModelId::ClaudeHaiku4_5));
    assert!(models.contains(&ModelId::Gpt5_4Codex));
    assert!(models.contains(&ModelId::Gpt5_4MiniCodex));
    assert!(models.contains(&ModelId::Gpt5Codex));
    assert!(models.contains(&ModelId::Gpt5_1Codex));
    assert!(models.contains(&ModelId::Gpt5_2Codex));
    assert!(models.contains(&ModelId::CodexCli));
    assert!(models.contains(&ModelId::OpenCodeCli));
    assert!(models.contains(&ModelId::GeminiCli));
    assert!(models.contains(&ModelId::DeepseekChat));
    assert!(models.contains(&ModelId::Gemini25Pro));
    assert!(models.contains(&ModelId::MiniMaxM21));
    assert!(models.contains(&ModelId::MiniMaxM27));
    assert!(models.contains(&ModelId::MiniMaxM27Highspeed));
    assert!(models.contains(&ModelId::MiniMaxM21CodingPlan));
    assert!(models.contains(&ModelId::MiniMaxM27CodingPlan));
    assert!(models.contains(&ModelId::MiniMaxM27CodingPlanHighspeed));
    assert!(models.contains(&ModelId::MiniMaxM25CodingPlanHighspeed));
    assert!(models.contains(&ModelId::Glm5Turbo));
    assert!(models.contains(&ModelId::Glm5TurboCodingPlan));
}

#[test]
fn test_metadata() {
    // Test metadata for GPT-5 (no temperature)
    let metadata = ModelId::Gpt5.metadata();
    assert_eq!(metadata.provider, Provider::OpenAI);
    assert!(!metadata.supports_temperature);
    assert_eq!(metadata.name, "GPT-5");

    // Test metadata for Claude Sonnet 4.5 (with temperature)
    let metadata = ModelId::ClaudeSonnet4_5.metadata();
    assert_eq!(metadata.provider, Provider::Anthropic);
    assert!(metadata.supports_temperature);
    assert_eq!(metadata.name, "Claude Sonnet 4.5");

    // Test metadata for DeepSeek Chat
    let metadata = ModelId::DeepseekChat.metadata();
    assert_eq!(metadata.provider, Provider::DeepSeek);
    assert!(metadata.supports_temperature);
    assert_eq!(metadata.name, "DeepSeek Chat");
}

#[test]
fn test_provider_as_llm_provider() {
    assert_eq!(Provider::OpenAI.as_llm_provider(), LlmProvider::OpenAI);
    assert_eq!(
        Provider::Anthropic.as_llm_provider(),
        LlmProvider::Anthropic
    );
    assert_eq!(
        Provider::ClaudeCode.as_llm_provider(),
        LlmProvider::Anthropic
    );
    assert_eq!(Provider::Codex.as_llm_provider(), LlmProvider::OpenAI);
    assert_eq!(Provider::Google.as_llm_provider(), LlmProvider::Google);
}

#[test]
fn test_build_model_specs_contains_codex_cli() {
    let specs = ModelId::build_model_specs();
    assert!(specs.iter().any(|spec| spec.name == "gpt-5.4"
        && spec.client_model == "gpt-5.4"
        && spec.client_kind == restflow_models::ClientKind::CodexCli));
    assert!(specs.iter().any(|spec| spec.name == "gpt-5.4-mini"
        && spec.client_model == "gpt-5.4-mini"
        && spec.client_kind == restflow_models::ClientKind::CodexCli));
    assert!(specs.iter().any(|spec| spec.name == "gpt-5-codex"
        && spec.client_kind == restflow_models::ClientKind::CodexCli));
    assert!(specs.iter().any(|spec| spec.name == "gpt-5.1-codex"
        && spec.client_kind == restflow_models::ClientKind::CodexCli));
    assert!(specs.iter().any(|spec| spec.name == "gpt-5.2-codex"
        && spec.client_kind == restflow_models::ClientKind::CodexCli));
    assert!(specs.iter().any(|spec| spec.name == "gpt-5.3-codex"
        && spec.client_kind == restflow_models::ClientKind::CodexCli));
    assert!(specs.iter().any(|spec| spec.name == "opus"
        && spec.client_model == "opus"
        && spec.client_kind == restflow_models::ClientKind::ClaudeCodeCli));
}

#[test]
fn test_glm5_code_uses_glm5_model_with_coding_endpoint() {
    let spec = ModelId::Glm5Code.as_model_spec();
    assert_eq!(spec.client_model, "glm-5");
    assert_eq!(spec.name, "glm-5-code");
    assert_eq!(
        spec.base_url.as_deref(),
        Some("https://api.z.ai/api/coding/paas/v4")
    );

    let turbo_spec = ModelId::Glm5Turbo.as_model_spec();
    assert_eq!(turbo_spec.client_model, "glm-5-turbo");
    assert_eq!(turbo_spec.name, "glm-5-turbo");
    assert_eq!(turbo_spec.base_url, None);

    let coding_plan_spec = ModelId::Glm5CodingPlan.as_model_spec();
    assert_eq!(coding_plan_spec.client_model, "glm-5");
    assert_eq!(
        coding_plan_spec.base_url.as_deref(),
        Some("https://api.z.ai/api/coding/paas/v4")
    );

    let coding_plan_turbo_spec = ModelId::Glm5TurboCodingPlan.as_model_spec();
    assert_eq!(coding_plan_turbo_spec.client_model, "glm-5-turbo");
    assert_eq!(
        coding_plan_turbo_spec.base_url.as_deref(),
        Some("https://api.z.ai/api/coding/paas/v4")
    );
}

#[test]
fn test_provider_api_key_env() {
    assert_eq!(Provider::Google.api_key_env(), Some("GEMINI_API_KEY"));
    assert_eq!(Provider::Groq.api_key_env(), Some("GROQ_API_KEY"));
    assert_eq!(Provider::Qwen.api_key_env(), Some("DASHSCOPE_API_KEY"));
    assert_eq!(Provider::MiniMax.api_key_env(), Some("MINIMAX_API_KEY"));
    assert_eq!(
        Provider::MiniMaxCodingPlan.api_key_env(),
        Some("MINIMAX_CODING_PLAN_API_KEY")
    );
    assert_eq!(Provider::Zai.api_key_env(), Some("ZAI_API_KEY"));
    assert_eq!(
        Provider::ZaiCodingPlan.api_key_env(),
        Some("ZAI_CODING_PLAN_API_KEY")
    );
    assert_eq!(Provider::ClaudeCode.api_key_env(), None);
    assert_eq!(Provider::Codex.api_key_env(), None);
}

#[test]
fn test_same_provider_fallback() {
    // Anthropic chain
    assert_eq!(
        ModelId::ClaudeOpus4_6.same_provider_fallback(),
        Some(ModelId::ClaudeSonnet4_5)
    );
    assert_eq!(
        ModelId::ClaudeSonnet4_5.same_provider_fallback(),
        Some(ModelId::ClaudeHaiku4_5)
    );
    assert_eq!(ModelId::ClaudeHaiku4_5.same_provider_fallback(), None);

    // OpenAI chain
    assert_eq!(
        ModelId::Gpt5Pro.same_provider_fallback(),
        Some(ModelId::Gpt5)
    );
    assert_eq!(
        ModelId::Gpt5.same_provider_fallback(),
        Some(ModelId::Gpt5Mini)
    );
    assert_eq!(
        ModelId::Gpt5Mini.same_provider_fallback(),
        Some(ModelId::Gpt5Nano)
    );
    assert_eq!(ModelId::Gpt5Nano.same_provider_fallback(), None);

    // DeepSeek chain
    assert_eq!(
        ModelId::DeepseekReasoner.same_provider_fallback(),
        Some(ModelId::DeepseekChat)
    );
    assert_eq!(ModelId::DeepseekChat.same_provider_fallback(), None);

    // GLM chain
    assert_eq!(
        ModelId::Glm5.same_provider_fallback(),
        Some(ModelId::Glm5Turbo)
    );
    assert_eq!(
        ModelId::Glm5Turbo.same_provider_fallback(),
        Some(ModelId::Glm5Code)
    );
    assert_eq!(
        ModelId::Glm5CodingPlan.same_provider_fallback(),
        Some(ModelId::Glm5TurboCodingPlan)
    );
    assert_eq!(
        ModelId::Glm5TurboCodingPlan.same_provider_fallback(),
        Some(ModelId::Glm5CodeCodingPlan)
    );
    assert_eq!(
        ModelId::MiniMaxM25CodingPlanHighspeed.same_provider_fallback(),
        Some(ModelId::MiniMaxM25CodingPlan)
    );

    // CLI models have no fallback
    assert_eq!(
        ModelId::Gpt5_4Codex.same_provider_fallback(),
        Some(ModelId::Gpt5_4MiniCodex)
    );
    assert_eq!(ModelId::Gpt5_4MiniCodex.same_provider_fallback(), None);
    assert_eq!(ModelId::CodexCli.same_provider_fallback(), None);
}

#[test]
fn test_openrouter_equivalent() {
    assert_eq!(
        ModelId::ClaudeOpus4_6.openrouter_equivalent(),
        Some(ModelId::OrClaudeOpus4_6)
    );
    assert_eq!(ModelId::Gpt5.openrouter_equivalent(), Some(ModelId::OrGpt5));
    assert_eq!(
        ModelId::DeepseekChat.openrouter_equivalent(),
        Some(ModelId::OrDeepseekV3_2)
    );
    assert_eq!(
        ModelId::Glm5Turbo.openrouter_equivalent(),
        Some(ModelId::OrGlm4_7)
    );
    assert_eq!(
        ModelId::KimiK2_5.openrouter_equivalent(),
        Some(ModelId::OrKimiK2_5)
    );
    assert_eq!(
        ModelId::MiniMaxM21.openrouter_equivalent(),
        Some(ModelId::OrMinimaxM2_1)
    );
    assert_eq!(
        ModelId::MiniMaxM25.openrouter_equivalent(),
        Some(ModelId::OrMinimaxM2_1)
    );
    assert_eq!(
        ModelId::MiniMaxM27.openrouter_equivalent(),
        Some(ModelId::OrMinimaxM2_1)
    );
    assert_eq!(
        ModelId::MiniMaxM27Highspeed.openrouter_equivalent(),
        Some(ModelId::OrMinimaxM2_1)
    );
    assert_eq!(
        ModelId::MiniMaxM25CodingPlanHighspeed.openrouter_equivalent(),
        Some(ModelId::OrMinimaxM2_1)
    );
    // OR models themselves have no OR equivalent
    assert_eq!(ModelId::OrClaudeOpus4_6.openrouter_equivalent(), None);
    // CLI models have no OR equivalent
    assert_eq!(ModelId::Gpt5_4Codex.openrouter_equivalent(), None);
    assert_eq!(ModelId::Gpt5_4MiniCodex.openrouter_equivalent(), None);
    assert_eq!(ModelId::CodexCli.openrouter_equivalent(), None);
}

#[test]
fn test_canonical_id() {
    // Test canonical ID generation
    assert_eq!(ModelId::Gpt5.canonical_id(), "openai:gpt-5");
    assert_eq!(
        ModelId::ClaudeSonnet4_5.canonical_id(),
        "anthropic:claude-sonnet-4-5"
    );
    assert_eq!(
        ModelId::DeepseekChat.canonical_id(),
        "deepseek:deepseek-chat"
    );
    assert_eq!(ModelId::Gemini3Pro.canonical_id(), "google:gemini-3-pro");
    assert_eq!(ModelId::OrGpt5.canonical_id(), "openrouter:or-gpt-5");
    assert_eq!(ModelId::Gpt5_4Codex.canonical_id(), "codex:gpt-5.4");
    assert_eq!(
        ModelId::Gpt5_4MiniCodex.canonical_id(),
        "codex:gpt-5.4-mini"
    );
    assert_eq!(ModelId::CodexCli.canonical_id(), "codex:gpt-5.3-codex");
    assert_eq!(
        ModelId::ClaudeCodeSonnet.canonical_id(),
        "claude-code:claude-code-sonnet"
    );
}

#[test]
fn test_from_canonical_id() {
    // Test parsing canonical IDs
    assert_eq!(
        ModelId::from_canonical_id("openai:gpt-5"),
        Some(ModelId::Gpt5)
    );
    assert_eq!(
        ModelId::from_canonical_id("anthropic:claude-sonnet-4-5"),
        Some(ModelId::ClaudeSonnet4_5)
    );
    assert_eq!(
        ModelId::from_canonical_id("deepseek:deepseek-chat"),
        Some(ModelId::DeepseekChat)
    );
    assert_eq!(
        ModelId::from_canonical_id("claude-code:claude-code-sonnet"),
        Some(ModelId::ClaudeCodeSonnet)
    );
    assert_eq!(
        ModelId::from_canonical_id("codex:gpt-5.3-codex"),
        Some(ModelId::CodexCli)
    );
    assert_eq!(
        ModelId::from_canonical_id("codex:gpt-5.4"),
        Some(ModelId::Gpt5_4Codex)
    );
    assert_eq!(
        ModelId::from_canonical_id("codex:gpt-5.4-mini"),
        Some(ModelId::Gpt5_4MiniCodex)
    );
    assert_eq!(
        ModelId::from_canonical_id("codex:gpt-5.4-codex"),
        Some(ModelId::Gpt5_4Codex)
    );
    assert_eq!(
        ModelId::from_canonical_id("codex:gpt-5.4-mini-codex"),
        Some(ModelId::Gpt5_4MiniCodex)
    );
    assert_eq!(
        ModelId::from_canonical_id("anthropic:claude-code-sonnet"),
        Some(ModelId::ClaudeCodeSonnet)
    );
    assert_eq!(
        ModelId::from_canonical_id("openai:gpt-5.3-codex"),
        Some(ModelId::CodexCli)
    );
    assert_eq!(
        ModelId::from_canonical_id("openai:gpt-5.4"),
        Some(ModelId::Gpt5_4Codex)
    );

    // Test legacy model-only strings (fallback)
    assert_eq!(ModelId::from_canonical_id("gpt-5"), Some(ModelId::Gpt5));
    assert_eq!(
        ModelId::from_canonical_id("claude-sonnet-4-5"),
        Some(ModelId::ClaudeSonnet4_5)
    );

    // Test invalid IDs
    assert_eq!(ModelId::from_canonical_id("unknown:model"), None);
    assert_eq!(ModelId::from_canonical_id("invalid-model"), None);
}

#[test]
fn test_canonical_id_round_trip() {
    // Test round-trip: canonical_id -> from_canonical_id
    for model in ModelId::all() {
        let canonical = model.canonical_id();
        let parsed = ModelId::from_canonical_id(&canonical);
        assert_eq!(
            parsed,
            Some(*model),
            "Round-trip failed for {} -> {}",
            model.as_str(),
            canonical
        );
    }
}

#[test]
fn test_model_ref_from_model_is_consistent() {
    let model_ref = ModelRef::from_model(ModelId::Gpt5);
    assert_eq!(model_ref.provider, Provider::OpenAI);
    assert_eq!(model_ref.model, ModelId::Gpt5);
    assert_eq!(model_ref.canonical_id(), "openai:gpt-5");
    assert!(model_ref.validate().is_ok());
}

#[test]
fn test_model_ref_validate_rejects_provider_mismatch() {
    let model_ref = ModelRef {
        provider: Provider::Anthropic,
        model: ModelId::Gpt5,
    };
    let error = model_ref
        .validate()
        .expect_err("provider mismatch should fail");
    assert_eq!(error.field, "model_ref");
    assert!(error.message.contains("does not match"));
}

#[test]
fn test_model_ref_validate_accepts_legacy_cli_provider_pairs() {
    let claude_code_ref = ModelRef {
        provider: Provider::Anthropic,
        model: ModelId::ClaudeCodeSonnet,
    };
    assert!(claude_code_ref.validate().is_ok());
    assert_eq!(
        claude_code_ref.normalized(),
        ModelRef {
            provider: Provider::ClaudeCode,
            model: ModelId::ClaudeCodeSonnet,
        }
    );

    let codex_ref = ModelRef {
        provider: Provider::OpenAI,
        model: ModelId::Gpt5_4Codex,
    };
    assert!(codex_ref.validate().is_ok());
    assert_eq!(
        codex_ref.normalized(),
        ModelRef {
            provider: Provider::Codex,
            model: ModelId::Gpt5_4Codex,
        }
    );
}

#[test]
fn test_model_ref_try_from_wire_normalizes_legacy_provider_pairs() {
    let wire = WireModelRef {
        provider: "anthropic".to_string(),
        model: "claude-code-sonnet".to_string(),
    };

    let model_ref = ModelRef::try_from(wire).expect("wire model ref should parse");
    assert_eq!(
        model_ref,
        ModelRef {
            provider: Provider::ClaudeCode,
            model: ModelId::ClaudeCodeSonnet,
        }
    );
}

#[test]
fn test_model_ref_into_wire_uses_canonical_values() {
    let wire: WireModelRef = ModelRef {
        provider: Provider::Anthropic,
        model: ModelId::ClaudeCodeSonnet,
    }
    .into();

    assert_eq!(wire.provider, "claude-code");
    assert_eq!(wire.model, "claude-code-sonnet");
}

#[test]
fn test_model_ref_try_from_wire_rejects_unknown_provider() {
    let error = ModelRef::try_from(WireModelRef {
        provider: "unknown".to_string(),
        model: "gpt-5".to_string(),
    })
    .expect_err("unknown provider should fail");

    assert_eq!(error.field, "model_ref.provider");
    assert!(error.message.contains("unknown provider"));
}

#[test]
fn test_model_ref_try_from_wire_rejects_unknown_model() {
    let error = ModelRef::try_from(WireModelRef {
        provider: "openai".to_string(),
        model: "missing-model".to_string(),
    })
    .expect_err("unknown model should fail");

    assert_eq!(error.field, "model_ref.model");
    assert!(error.message.contains("unknown model"));
}

#[test]
fn test_provider_canonical_str() {
    // Test provider canonical strings
    assert_eq!(Provider::OpenAI.as_canonical_str(), "openai");
    assert_eq!(Provider::Anthropic.as_canonical_str(), "anthropic");
    assert_eq!(Provider::DeepSeek.as_canonical_str(), "deepseek");
    assert_eq!(Provider::Google.as_canonical_str(), "google");
    assert_eq!(Provider::OpenRouter.as_canonical_str(), "openrouter");
    assert_eq!(Provider::ClaudeCode.as_canonical_str(), "claude-code");
    assert_eq!(Provider::Codex.as_canonical_str(), "codex");
    assert_eq!(
        Provider::ZaiCodingPlan.as_canonical_str(),
        "zai-coding-plan"
    );
    assert_eq!(
        Provider::MiniMaxCodingPlan.as_canonical_str(),
        "minimax-coding-plan"
    );
}

#[test]
fn test_provider_from_canonical_str() {
    // Test parsing provider canonical strings and supported aliases
    assert_eq!(
        Provider::from_canonical_str("openai"),
        Some(Provider::OpenAI)
    );
    assert_eq!(Provider::from_canonical_str("gpt"), Some(Provider::OpenAI));
    assert_eq!(
        Provider::from_canonical_str("anthropic"),
        Some(Provider::Anthropic)
    );
    assert_eq!(
        Provider::from_canonical_str("claude-code"),
        Some(Provider::ClaudeCode)
    );
    assert_eq!(Provider::from_canonical_str("codex"), Some(Provider::Codex));
    assert_eq!(
        Provider::from_canonical_str("openai-codex"),
        Some(Provider::Codex)
    );
    assert_eq!(
        Provider::from_canonical_str("deepseek"),
        Some(Provider::DeepSeek)
    );
    assert_eq!(
        Provider::from_canonical_str("google"),
        Some(Provider::Google)
    );
    assert_eq!(
        Provider::from_canonical_str("gemini"),
        Some(Provider::Google)
    );
    assert_eq!(
        Provider::from_canonical_str("zhipu-coding-plan"),
        Some(Provider::ZaiCodingPlan)
    );
    assert_eq!(Provider::from_canonical_str("invalid"), None);
}

#[test]
fn test_provider_model_provider_round_trip() {
    for provider in Provider::all().iter().copied() {
        let shared = provider.as_model_provider();
        assert_eq!(Provider::from_model_provider(shared), provider);
    }
}

#[test]
fn test_normalize_model_id() {
    assert_eq!(
        ModelId::normalize_model_id("MiniMax-M2.5"),
        Some("minimax-m2-5".to_string())
    );
    assert_eq!(
        ModelId::normalize_model_id("MiniMax-M2.7"),
        Some("minimax-m2-7".to_string())
    );
    assert_eq!(
        ModelId::normalize_model_id("MiniMax-M2.7-highspeed"),
        Some("minimax-m2-7-highspeed".to_string())
    );
    assert_eq!(
        ModelId::normalize_model_id("gpt-5.1"),
        Some("gpt-5-1".to_string())
    );
    assert_eq!(
        ModelId::normalize_model_id("openai:gpt-5"),
        Some("gpt-5".to_string())
    );
    assert_eq!(
        ModelId::normalize_model_id("claude-sonnet-4-20250514"),
        Some("claude-sonnet-4-5".to_string())
    );
    assert_eq!(ModelId::normalize_model_id(""), None);
}

#[test]
fn test_normalize_model_id_for_provider_avoids_minimax_collision() {
    assert_eq!(
        ModelId::normalize_model_id_for_provider(Provider::MiniMaxCodingPlan, "MiniMax-M2.5"),
        Some("minimax-coding-plan-m2-5".to_string())
    );
    assert_eq!(
        ModelId::normalize_model_id_for_provider(
            Provider::MiniMaxCodingPlan,
            "MiniMax-M2.5-highspeed"
        ),
        Some("minimax-coding-plan-m2-5-highspeed".to_string())
    );
    assert_eq!(
        ModelId::normalize_model_id_for_provider(Provider::MiniMax, "MiniMax-M2.5"),
        Some("minimax-m2-5".to_string())
    );
}

#[test]
fn test_flagship_model() {
    assert_eq!(
        Provider::Anthropic.flagship_model(),
        ModelId::ClaudeSonnet4_5
    );
    assert_eq!(Provider::OpenAI.flagship_model(), ModelId::Gpt5);
    assert_eq!(Provider::DeepSeek.flagship_model(), ModelId::DeepseekChat);
    assert_eq!(Provider::Google.flagship_model(), ModelId::Gemini3Pro);
    assert_eq!(Provider::MiniMax.flagship_model(), ModelId::MiniMaxM27);
    assert_eq!(Provider::Zai.flagship_model(), ModelId::Glm5);
    assert_eq!(
        Provider::ZaiCodingPlan.flagship_model(),
        ModelId::Glm5_1CodingPlan
    );
    assert_eq!(
        Provider::MiniMaxCodingPlan.flagship_model(),
        ModelId::MiniMaxM27CodingPlan
    );
    assert_eq!(
        Provider::ClaudeCode.flagship_model(),
        ModelId::ClaudeCodeOpus
    );
    assert_eq!(Provider::Codex.flagship_model(), ModelId::Gpt5_4Codex);
    assert_eq!(
        Provider::OpenRouter.flagship_model(),
        ModelId::OrClaudeOpus4_6
    );
}

#[test]
fn test_provider_catalog_completeness() {
    assert_eq!(Provider::all().len(), catalog::PROVIDER_CATALOGS.len());

    for provider in Provider::all().iter().copied() {
        let provider_catalog = catalog::provider_catalog(provider)
            .unwrap_or_else(|| panic!("missing provider catalog for {provider:?}"));
        assert_eq!(provider_catalog.provider, provider);
        assert_eq!(provider_catalog.flagship.provider(), provider);
        assert!(!provider_catalog.models.is_empty());
    }
}

#[test]
fn test_catalog_lookup_round_trips_model_ids() {
    for model in ModelId::all() {
        let descriptor = catalog::descriptor(*model)
            .unwrap_or_else(|| panic!("missing descriptor for {model:?}"));
        assert_eq!(descriptor.id, *model);
        assert_eq!(descriptor.provider, model.provider());
        assert_eq!(
            catalog::lookup_by_name(model.as_serialized_str()),
            Some(*model)
        );
        let resolved = catalog::lookup_for_provider(model.provider(), model.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "missing provider lookup for {} via {}",
                    model.as_serialized_str(),
                    model.as_str()
                )
            });
        assert_eq!(resolved.provider(), model.provider());
        assert_eq!(resolved.as_str(), model.as_str());
    }
}

#[test]
fn test_minimax_m25_serialization_consistency() {
    // as_serialized_str() must match the serde rename
    let json_str = serde_json::to_string(&ModelId::MiniMaxM25).unwrap();
    let expected = format!("\"{}\"", ModelId::MiniMaxM25.as_serialized_str());
    assert_eq!(json_str, expected);
}

#[test]
fn test_from_api_name_trimmed_input() {
    // Whitespace around model name should still resolve
    assert_eq!(
        ModelId::from_api_name("  Claude-Sonnet-4-5-20250514  "),
        Some(ModelId::ClaudeSonnet4_5)
    );
}
