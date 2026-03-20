use anyhow::{Result, bail};
use chrono::{DateTime, Local, TimeZone};
use restflow_core::models::{ModelId, Provider};
use restflow_traits::ModelProvider;

pub fn format_timestamp(timestamp: Option<i64>) -> String {
    let Some(ts) = timestamp else {
        return "-".to_string();
    };

    let datetime: DateTime<Local> = match Local.timestamp_millis_opt(ts).single() {
        Some(dt) => dt,
        None => return "-".to_string(),
    };

    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn parse_model(input: &str) -> Result<ModelId> {
    let normalized = input.trim().to_lowercase();
    let model = match normalized.as_str() {
        // OpenAI GPT-5 series
        "gpt-5" => ModelId::Gpt5,
        "gpt-5-mini" => ModelId::Gpt5Mini,
        "gpt-5-nano" => ModelId::Gpt5Nano,
        "gpt-5-pro" => ModelId::Gpt5Pro,
        "gpt-5.1" | "gpt-5-1" => ModelId::Gpt5_1,
        "gpt-5.2" | "gpt-5-2" => ModelId::Gpt5_2,
        // Anthropic Claude (direct API)
        "claude-opus-4-6" => ModelId::ClaudeOpus4_6,
        "claude-sonnet-4-5" => ModelId::ClaudeSonnet4_5,
        "claude-haiku-4-5" => ModelId::ClaudeHaiku4_5,
        // Claude Code CLI (accepts both full name and short alias)
        "claude-code-opus" | "opus" => ModelId::ClaudeCodeOpus,
        "claude-code-sonnet" | "sonnet" => ModelId::ClaudeCodeSonnet,
        "claude-code-haiku" | "haiku" => ModelId::ClaudeCodeHaiku,
        // Codex CLI (must use concrete model names)
        "gpt-5-codex" => ModelId::Gpt5Codex,
        "gpt-5.4" | "gpt-5.4-codex" => ModelId::Gpt5_4Codex,
        "gpt-5.4-mini" | "gpt-5.4-mini-codex" => ModelId::Gpt5_4MiniCodex,
        "gpt-5.1-codex" => ModelId::Gpt5_1Codex,
        "gpt-5.2-codex" => ModelId::Gpt5_2Codex,
        "gpt-5.3-codex" => ModelId::CodexCli,
        // OpenCode CLI
        "opencode" | "opencode-cli" => ModelId::OpenCodeCli,
        // Gemini CLI
        "gemini-cli" => ModelId::GeminiCli,
        // DeepSeek
        "deepseek-chat" => ModelId::DeepseekChat,
        "deepseek-reasoner" => ModelId::DeepseekReasoner,
        // Google Gemini
        "gemini-2.5-pro" | "gemini-pro" => ModelId::Gemini25Pro,
        "gemini-2.5-flash" | "gemini-flash" => ModelId::Gemini25Flash,
        "gemini-3-pro" | "gemini-3-pro-preview" => ModelId::Gemini3Pro,
        "gemini-3-flash" | "gemini-3-flash-preview" => ModelId::Gemini3Flash,
        // Groq
        "groq-scout" | "llama-4-scout" => ModelId::GroqLlama4Scout,
        "groq-maverick" | "llama-4-maverick" => ModelId::GroqLlama4Maverick,
        // X.AI
        "grok-4" | "grok4" => ModelId::Grok4,
        "grok-3-mini" | "grok3-mini" => ModelId::Grok3Mini,
        // Qwen
        "qwen3-max" | "qwen-max" | "qwen" => ModelId::Qwen3Max,
        "qwen3-plus" | "qwen-plus" => ModelId::Qwen3Plus,
        // MiniMax
        "minimax-m2-1" => ModelId::MiniMaxM21,
        "minimax-m2-5" => ModelId::MiniMaxM25,
        "minimax-m2-7" | "minimax-m2.7" => ModelId::MiniMaxM27,
        "minimax-m2-7-highspeed" | "minimax-m2.7-highspeed" => ModelId::MiniMaxM27Highspeed,
        "minimax-coding-plan-m2-1" => ModelId::MiniMaxM21CodingPlan,
        "minimax-coding-plan-m2-5" => ModelId::MiniMaxM25CodingPlan,
        // Zai
        "glm-5" | "glm5" => ModelId::Glm5,
        "glm-5-turbo" | "glm5-turbo" => ModelId::Glm5Turbo,
        "glm-5-code" | "glm5-code" => ModelId::Glm5Code,
        "glm-4.7" | "glm-4-7" | "glm" => ModelId::Glm4_7,
        "zai-coding-plan-glm-5" => ModelId::Glm5CodingPlan,
        "zai-coding-plan-glm-5-turbo" => ModelId::Glm5TurboCodingPlan,
        "zai-coding-plan-glm-5-code" => ModelId::Glm5CodeCodingPlan,
        "zai-coding-plan-glm-4-7" => ModelId::Glm4_7CodingPlan,
        // Moonshot
        "kimi-k2.5" | "kimi-k2-5" | "kimi" | "moonshot" => ModelId::KimiK2_5,
        // Doubao
        "doubao-pro" | "doubao" => ModelId::DoubaoPro,
        // Yi
        "yi-lightning" | "yi" => ModelId::YiLightning,
        // Aggregators
        "openrouter" => ModelId::OpenRouterAuto,
        "siliconflow" => ModelId::SiliconFlowAuto,
        _ => ModelId::from_api_name(input)
            .ok_or_else(|| anyhow::anyhow!("Unknown model: {input}"))?,
    };

    Ok(model)
}

pub fn parse_provider(input: &str) -> Result<Provider> {
    let provider = ModelProvider::parse_alias(input)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {input}"))?;
    Ok(match provider {
        ModelProvider::OpenAI => Provider::OpenAI,
        ModelProvider::Anthropic => Provider::Anthropic,
        ModelProvider::ClaudeCode => Provider::ClaudeCode,
        ModelProvider::Codex => Provider::Codex,
        ModelProvider::DeepSeek => Provider::DeepSeek,
        ModelProvider::Google => Provider::Google,
        ModelProvider::Groq => Provider::Groq,
        ModelProvider::OpenRouter => Provider::OpenRouter,
        ModelProvider::XAI => Provider::XAI,
        ModelProvider::Qwen => Provider::Qwen,
        ModelProvider::Zai => Provider::Zai,
        ModelProvider::ZaiCodingPlan => Provider::ZaiCodingPlan,
        ModelProvider::Moonshot => Provider::Moonshot,
        ModelProvider::Doubao => Provider::Doubao,
        ModelProvider::Yi => Provider::Yi,
        ModelProvider::SiliconFlow => Provider::SiliconFlow,
        ModelProvider::MiniMax => Provider::MiniMax,
        ModelProvider::MiniMaxCodingPlan => Provider::MiniMaxCodingPlan,
    })
}

pub fn parse_model_for_provider(provider: Provider, input: &str) -> Result<ModelId> {
    if let Some(model) = ModelId::for_provider_and_model(provider, input) {
        return Ok(model);
    }

    if let Some(parsed) = ModelId::from_api_name(input) {
        if parsed.provider() == provider {
            return Ok(parsed);
        }
        if let Some(remapped) = parsed.remap_provider(provider) {
            return Ok(remapped);
        }
    }

    bail!(
        "Model '{}' does not belong to provider '{}'",
        input,
        provider_label(provider)
    )
}

fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAI => "openai",
        Provider::Anthropic => "anthropic",
        Provider::ClaudeCode => "claude-code",
        Provider::Codex => "codex",
        Provider::DeepSeek => "deepseek",
        Provider::Google => "google",
        Provider::Groq => "groq",
        Provider::OpenRouter => "openrouter",
        Provider::XAI => "xai",
        Provider::Qwen => "qwen",
        Provider::Zai => "zai",
        Provider::ZaiCodingPlan => "zai-coding-plan",
        Provider::Moonshot => "moonshot",
        Provider::Doubao => "doubao",
        Provider::Yi => "yi",
        Provider::SiliconFlow => "siliconflow",
        Provider::MiniMax => "minimax",
        Provider::MiniMaxCodingPlan => "minimax-coding-plan",
    }
}

pub fn slugify(input: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !last_dash && !output.is_empty()
        {
            output.push('-');
            last_dash = true;
        }
    }

    if output.ends_with('-') {
        output.pop();
    }

    if output.is_empty() {
        "skill".to_string()
    } else {
        output
    }
}

pub fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

pub fn preview_text(input: &str, max_len: usize) -> String {
    if input.len() <= max_len {
        return input.to_string();
    }

    let mut preview = input.chars().take(max_len).collect::<String>();
    preview.push('…');
    preview
}

#[cfg(test)]
mod tests {
    use super::{parse_model, parse_provider};
    use restflow_core::models::{ModelId, Provider};

    #[test]
    fn parse_provider_accepts_shared_aliases() {
        assert_eq!(parse_provider("gpt").unwrap(), Provider::OpenAI);
        assert_eq!(parse_provider("zhipu").unwrap(), Provider::Zai);
        assert_eq!(parse_provider("claude-code").unwrap(), Provider::ClaudeCode);
        assert_eq!(parse_provider("openai-codex").unwrap(), Provider::Codex);
        assert_eq!(
            parse_provider("minimax-coding").unwrap(),
            Provider::MiniMaxCodingPlan
        );
    }

    #[test]
    fn parse_model_accepts_glm5_turbo_variants() {
        assert_eq!(parse_model("glm-5-turbo").unwrap(), ModelId::Glm5Turbo);
        assert_eq!(parse_model("glm5-turbo").unwrap(), ModelId::Glm5Turbo);
        assert_eq!(
            parse_model("zai-coding-plan-glm-5-turbo").unwrap(),
            ModelId::Glm5TurboCodingPlan
        );
    }

    #[test]
    fn parse_model_accepts_minimax_m27_variants() {
        assert_eq!(parse_model("minimax-m2-7").unwrap(), ModelId::MiniMaxM27);
        assert_eq!(parse_model("minimax-m2.7").unwrap(), ModelId::MiniMaxM27);
        assert_eq!(
            parse_model("minimax-m2-7-highspeed").unwrap(),
            ModelId::MiniMaxM27Highspeed
        );
    }

    #[test]
    fn parse_model_accepts_codex_gpt54_variants() {
        assert_eq!(parse_model("gpt-5.4").unwrap(), ModelId::Gpt5_4Codex);
        assert_eq!(parse_model("gpt-5.4-codex").unwrap(), ModelId::Gpt5_4Codex);
        assert_eq!(
            parse_model("gpt-5.4-mini").unwrap(),
            ModelId::Gpt5_4MiniCodex
        );
        assert_eq!(
            parse_model("gpt-5.4-mini-codex").unwrap(),
            ModelId::Gpt5_4MiniCodex
        );
    }
}
