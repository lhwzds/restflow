use anyhow::{Result, bail};
use chrono::{DateTime, Local, TimeZone};
use restflow_core::models::AIModel;

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

pub fn parse_model(input: &str) -> Result<AIModel> {
    let normalized = input.trim().to_lowercase();
    let model = match normalized.as_str() {
        // OpenAI GPT-5 series
        "gpt-5" => AIModel::Gpt5,
        "gpt-5-mini" => AIModel::Gpt5Mini,
        "gpt-5-nano" => AIModel::Gpt5Nano,
        "gpt-5-pro" => AIModel::Gpt5Pro,
        "gpt-5.1" | "gpt-5-1" => AIModel::Gpt5_1,
        "gpt-5.2" | "gpt-5-2" => AIModel::Gpt5_2,
        // Anthropic Claude (direct API)
        "claude-opus-4-6" => AIModel::ClaudeOpus4_6,
        "claude-sonnet-4-5" => AIModel::ClaudeSonnet4_5,
        "claude-haiku-4-5" => AIModel::ClaudeHaiku4_5,
        // Claude Code CLI (accepts both full name and short alias)
        "claude-code-opus" | "opus" => AIModel::ClaudeCodeOpus,
        "claude-code-sonnet" | "sonnet" => AIModel::ClaudeCodeSonnet,
        "claude-code-haiku" | "haiku" => AIModel::ClaudeCodeHaiku,
        // Codex CLI (must use concrete model names)
        "gpt-5-codex" => AIModel::Gpt5Codex,
        "gpt-5.1-codex" => AIModel::Gpt5_1Codex,
        "gpt-5.2-codex" => AIModel::Gpt5_2Codex,
        "gpt-5.3-codex" => AIModel::CodexCli,
        // OpenCode CLI
        "opencode" | "opencode-cli" => AIModel::OpenCodeCli,
        // Gemini CLI
        "gemini-cli" => AIModel::GeminiCli,
        // DeepSeek
        "deepseek-chat" => AIModel::DeepseekChat,
        "deepseek-reasoner" => AIModel::DeepseekReasoner,
        // Google Gemini
        "gemini-2.5-pro" | "gemini-pro" => AIModel::Gemini25Pro,
        "gemini-2.5-flash" | "gemini-flash" => AIModel::Gemini25Flash,
        "gemini-3-pro" | "gemini-3-pro-preview" => AIModel::Gemini3Pro,
        "gemini-3-flash" | "gemini-3-flash-preview" => AIModel::Gemini3Flash,
        // Groq
        "groq-scout" | "llama-4-scout" => AIModel::GroqLlama4Scout,
        "groq-maverick" | "llama-4-maverick" => AIModel::GroqLlama4Maverick,
        // X.AI
        "grok-4" | "grok4" => AIModel::Grok4,
        "grok-3-mini" | "grok3-mini" => AIModel::Grok3Mini,
        // Qwen
        "qwen3-max" | "qwen-max" | "qwen" => AIModel::Qwen3Max,
        "qwen3-plus" | "qwen-plus" => AIModel::Qwen3Plus,
        // MiniMax
        "minimax-m2-1" => AIModel::MiniMaxM21,
        "minimax-m2-5" => AIModel::MiniMaxM25,
        // Zhipu
        "glm-5" | "glm5" => AIModel::Glm5,
        "glm-5-code" | "glm5-code" => AIModel::Glm5Code,
        "glm-4.7" | "glm-4-7" | "glm" => AIModel::Glm4_7,
        // Moonshot
        "kimi-k2.5" | "kimi-k2-5" | "kimi" | "moonshot" => AIModel::KimiK2_5,
        // Doubao
        "doubao-pro" | "doubao" => AIModel::DoubaoPro,
        // Yi
        "yi-lightning" | "yi" => AIModel::YiLightning,
        // Aggregators
        "openrouter" => AIModel::OpenRouterAuto,
        "siliconflow" => AIModel::SiliconFlowAuto,
        _ => {
            bail!("Unknown model: {input}")
        }
    };

    Ok(model)
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

pub fn preview_text(input: &str, max_len: usize) -> String {
    if input.len() <= max_len {
        return input.to_string();
    }

    let mut preview = input.chars().take(max_len).collect::<String>();
    preview.push('…');
    preview
}
