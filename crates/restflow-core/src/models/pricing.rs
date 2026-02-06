use super::ai_model::AIModel;

/// Pricing per 1 million tokens (USD).
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub cost_per_1m_input: f64,
    pub cost_per_1m_output: f64,
}

impl AIModel {
    /// Get pricing for this model.
    /// Returns None for CLI-based models where cost is tracked externally.
    pub fn pricing(&self) -> Option<ModelPricing> {
        match self {
            // OpenAI
            Self::Gpt5 => Some(ModelPricing {
                cost_per_1m_input: 2.0,
                cost_per_1m_output: 8.0,
            }),
            Self::Gpt5Mini => Some(ModelPricing {
                cost_per_1m_input: 0.4,
                cost_per_1m_output: 1.6,
            }),
            Self::Gpt5Nano => Some(ModelPricing {
                cost_per_1m_input: 0.1,
                cost_per_1m_output: 0.4,
            }),
            Self::Gpt5Pro => Some(ModelPricing {
                cost_per_1m_input: 10.0,
                cost_per_1m_output: 40.0,
            }),
            Self::O4Mini => Some(ModelPricing {
                cost_per_1m_input: 1.1,
                cost_per_1m_output: 4.4,
            }),
            Self::O3 => Some(ModelPricing {
                cost_per_1m_input: 2.0,
                cost_per_1m_output: 8.0,
            }),
            Self::O3Mini => Some(ModelPricing {
                cost_per_1m_input: 1.1,
                cost_per_1m_output: 4.4,
            }),

            // Anthropic
            Self::ClaudeOpus4_1 => Some(ModelPricing {
                cost_per_1m_input: 15.0,
                cost_per_1m_output: 75.0,
            }),
            Self::ClaudeSonnet4_5 => Some(ModelPricing {
                cost_per_1m_input: 3.0,
                cost_per_1m_output: 15.0,
            }),
            Self::ClaudeHaiku4_5 => Some(ModelPricing {
                cost_per_1m_input: 0.8,
                cost_per_1m_output: 4.0,
            }),

            // DeepSeek
            Self::DeepseekChat => Some(ModelPricing {
                cost_per_1m_input: 0.27,
                cost_per_1m_output: 1.10,
            }),
            Self::DeepseekReasoner => Some(ModelPricing {
                cost_per_1m_input: 0.55,
                cost_per_1m_output: 2.19,
            }),

            // CLI-based (cost tracked externally)
            Self::ClaudeCodeOpus
            | Self::ClaudeCodeSonnet
            | Self::ClaudeCodeHaiku
            | Self::CodexCli
            | Self::OpenCodeCli => None,
        }
    }
}

/// Calculate cost in USD from token usage and model.
pub fn calculate_cost(model: AIModel, input_tokens: u32, output_tokens: u32) -> Option<f64> {
    let pricing = model.pricing()?;
    let cost = (input_tokens as f64 / 1_000_000.0) * pricing.cost_per_1m_input
        + (output_tokens as f64 / 1_000_000.0) * pricing.cost_per_1m_output;
    Some(cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pricing_anthropic_sonnet() {
        let pricing = AIModel::ClaudeSonnet4_5.pricing().unwrap();
        assert_eq!(pricing.cost_per_1m_input, 3.0);
        assert_eq!(pricing.cost_per_1m_output, 15.0);
    }

    #[test]
    fn test_pricing_cli_models_none() {
        assert!(AIModel::ClaudeCodeOpus.pricing().is_none());
        assert!(AIModel::CodexCli.pricing().is_none());
        assert!(AIModel::OpenCodeCli.pricing().is_none());
    }

    #[test]
    fn test_calculate_cost() {
        let cost = calculate_cost(AIModel::ClaudeSonnet4_5, 1000, 500).unwrap();
        assert!((cost - 0.0105).abs() < 1e-6);
    }

    #[test]
    fn test_calculate_cost_zero_tokens() {
        let cost = calculate_cost(AIModel::ClaudeSonnet4_5, 0, 0).unwrap();
        assert_eq!(cost, 0.0);
    }
}
