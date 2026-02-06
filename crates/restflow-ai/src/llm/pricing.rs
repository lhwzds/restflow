//! Model pricing and cost calculation for LLM API calls.

/// Pricing per 1 million tokens (USD).
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub cost_per_1m_input: f64,
    pub cost_per_1m_output: f64,
}

/// Get pricing for a model by API name.
/// Returns None for CLI-based models where cost is tracked externally.
pub fn get_pricing(model_name: &str) -> Option<ModelPricing> {
    // Match model name prefixes to handle versioned model names
    // e.g., "claude-sonnet-4-20250514" should match ClaudeSonnet4_5

    // CLI-based models (cost tracked externally) - check first to avoid prefix matching
    // codex-cli, claude-code CLI aliases
    if model_name.contains("codex") || model_name == "gpt-5.3-codex" {
        return None;
    }
    if model_name == "opus" || model_name == "sonnet" || model_name == "haiku" {
        return None;
    }

    // OpenAI
    if model_name.starts_with("gpt-5-pro") {
        return Some(ModelPricing {
            cost_per_1m_input: 10.0,
            cost_per_1m_output: 40.0,
        });
    }
    if model_name.starts_with("gpt-5-mini") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.4,
            cost_per_1m_output: 1.6,
        });
    }
    if model_name.starts_with("gpt-5-nano") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.1,
            cost_per_1m_output: 0.4,
        });
    }
    if model_name.starts_with("gpt-5") || model_name == "gpt-5" {
        return Some(ModelPricing {
            cost_per_1m_input: 2.0,
            cost_per_1m_output: 8.0,
        });
    }
    if model_name.starts_with("o4-mini") {
        return Some(ModelPricing {
            cost_per_1m_input: 1.1,
            cost_per_1m_output: 4.4,
        });
    }
    if model_name.starts_with("o3-mini") {
        return Some(ModelPricing {
            cost_per_1m_input: 1.1,
            cost_per_1m_output: 4.4,
        });
    }
    if model_name.starts_with("o3") {
        return Some(ModelPricing {
            cost_per_1m_input: 2.0,
            cost_per_1m_output: 8.0,
        });
    }

    // Anthropic
    if model_name.starts_with("claude-opus-4-1") || model_name.starts_with("claude-opus-4") {
        return Some(ModelPricing {
            cost_per_1m_input: 15.0,
            cost_per_1m_output: 75.0,
        });
    }
    if model_name.starts_with("claude-sonnet-4") {
        return Some(ModelPricing {
            cost_per_1m_input: 3.0,
            cost_per_1m_output: 15.0,
        });
    }
    if model_name.starts_with("claude-haiku-4") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.8,
            cost_per_1m_output: 4.0,
        });
    }

    // DeepSeek
    if model_name.starts_with("deepseek-reasoner") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.55,
            cost_per_1m_output: 2.19,
        });
    }
    if model_name.starts_with("deepseek-chat") || model_name.starts_with("deepseek") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.27,
            cost_per_1m_output: 1.10,
        });
    }

    // Unknown model - return None
    None
}

/// Calculate cost in USD from token usage and model name.
pub fn calculate_cost(model_name: &str, input_tokens: u32, output_tokens: u32) -> Option<f64> {
    let pricing = get_pricing(model_name)?;
    let cost = (input_tokens as f64 / 1_000_000.0) * pricing.cost_per_1m_input
        + (output_tokens as f64 / 1_000_000.0) * pricing.cost_per_1m_output;
    Some(cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pricing_anthropic_sonnet() {
        let pricing = get_pricing("claude-sonnet-4-20250514").unwrap();
        assert_eq!(pricing.cost_per_1m_input, 3.0);
        assert_eq!(pricing.cost_per_1m_output, 15.0);
    }

    #[test]
    fn test_pricing_openai_gpt5() {
        let pricing = get_pricing("gpt-5").unwrap();
        assert_eq!(pricing.cost_per_1m_input, 2.0);
        assert_eq!(pricing.cost_per_1m_output, 8.0);
    }

    #[test]
    fn test_pricing_cli_models_none() {
        assert!(get_pricing("opus").is_none());
        assert!(get_pricing("sonnet").is_none());
        assert!(get_pricing("gpt-5.3-codex").is_none());
    }

    #[test]
    fn test_calculate_cost() {
        // 1000 input + 500 output on Sonnet 4.5
        // = (1000/1M * 3.0) + (500/1M * 15.0) = 0.003 + 0.0075 = 0.0105
        let cost = calculate_cost("claude-sonnet-4-20250514", 1000, 500).unwrap();
        assert!((cost - 0.0105).abs() < 1e-6);
    }

    #[test]
    fn test_calculate_cost_zero_tokens() {
        let cost = calculate_cost("claude-sonnet-4-20250514", 0, 0).unwrap();
        assert_eq!(cost, 0.0);
    }
}
