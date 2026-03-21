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
    ModelId::from_api_name(input)
        .or_else(|| ModelId::from_canonical_id(input))
        .ok_or_else(|| anyhow::anyhow!("Unknown model: {input}"))
}

pub fn parse_provider(input: &str) -> Result<Provider> {
    let provider = ModelProvider::parse_alias(input)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {input}"))?;
    Ok(Provider::from(provider))
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
    provider.as_canonical_str()
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
    fn parse_model_accepts_minimax_coding_plan_highspeed_variants() {
        assert_eq!(
            parse_model("minimax-coding-plan-m2-5-highspeed").unwrap(),
            ModelId::MiniMaxM25CodingPlanHighspeed
        );
        assert_eq!(
            parse_model("minimax-m2.5-highspeed").unwrap(),
            ModelId::MiniMaxM25CodingPlanHighspeed
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

    #[test]
    fn parse_model_uses_shared_catalog_aliases() {
        assert_eq!(
            parse_model("claude-code-opus").unwrap(),
            ModelId::ClaudeCodeOpus
        );
        assert_eq!(parse_model("gemini-pro").unwrap(), ModelId::Gemini25Pro);
        assert_eq!(parse_model("openrouter").unwrap(), ModelId::OpenRouterAuto);
    }
}
