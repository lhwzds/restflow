//! Model pricing and cost calculation for LLM API calls.

use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::RwLock;

/// Pricing per 1 million tokens (USD).
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub cost_per_1m_input: f64,
    pub cost_per_1m_output: f64,
}

#[derive(Default)]
struct DynamicPricingCache {
    loaded: bool,
    by_model: HashMap<String, ModelPricing>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevProvider {
    models: HashMap<String, ModelsDevModel>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevModel {
    id: Option<String>,
    cost: Option<ModelsDevCost>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevCost {
    input: f64,
    output: f64,
}

static DYNAMIC_PRICING_CACHE: Lazy<RwLock<DynamicPricingCache>> =
    Lazy::new(|| RwLock::new(DynamicPricingCache::default()));

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn pricing_candidates(model_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |value: String| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        let key = normalize(trimmed);
        if seen.insert(key) {
            candidates.push(trimmed.to_string());
        }
    };

    push(model_name.to_string());
    push(model_name.replace('.', "-"));
    push(model_name.replace('-', "."));

    if let Some(base) = model_name.strip_suffix("-preview") {
        push(base.to_string());
    }
    if let Some((_, tail)) = model_name.split_once('/') {
        push(tail.to_string());
        push(tail.replace('.', "-"));
        push(tail.replace('-', "."));
    }

    candidates
}

fn resolve_models_cache_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("RESTFLOW_MODELS_PATH")
        && !path.trim().is_empty()
    {
        return Some(PathBuf::from(path));
    }

    if let Ok(dir) = std::env::var("RESTFLOW_DIR")
        && !dir.trim().is_empty()
    {
        return Some(PathBuf::from(dir).join("cache").join("models.json"));
    }

    dirs::home_dir().map(|home| home.join(".restflow").join("cache").join("models.json"))
}

fn load_dynamic_pricing_cache() -> HashMap<String, ModelPricing> {
    let Some(path) = resolve_models_cache_path() else {
        return HashMap::new();
    };

    let Ok(raw) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };

    let Ok(root) = serde_json::from_str::<HashMap<String, ModelsDevProvider>>(&raw) else {
        return HashMap::new();
    };

    let mut by_model = HashMap::new();
    for provider in root.into_values() {
        for (model_key, model) in provider.models {
            let Some(cost) = model.cost else {
                continue;
            };

            let pricing = ModelPricing {
                cost_per_1m_input: cost.input,
                cost_per_1m_output: cost.output,
            };

            by_model.insert(normalize(&model_key), pricing);
            if let Some(id) = model.id.as_deref() {
                by_model.insert(normalize(id), pricing);
            }
        }
    }

    by_model
}

fn dynamic_pricing(model_name: &str) -> Option<ModelPricing> {
    {
        let cache = DYNAMIC_PRICING_CACHE.read().ok()?;
        if cache.loaded {
            for candidate in pricing_candidates(model_name) {
                if let Some(pricing) = cache.by_model.get(&normalize(&candidate)) {
                    return Some(*pricing);
                }
            }
            return None;
        }
    }

    {
        let mut cache = DYNAMIC_PRICING_CACHE.write().ok()?;
        if !cache.loaded {
            cache.by_model = load_dynamic_pricing_cache();
            cache.loaded = true;
        }
        for candidate in pricing_candidates(model_name) {
            if let Some(pricing) = cache.by_model.get(&normalize(&candidate)) {
                return Some(*pricing);
            }
        }
    }

    None
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

    if let Some(pricing) = dynamic_pricing(model_name) {
        return Some(pricing);
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

    // Anthropic
    if model_name.starts_with("claude-opus-4-6") || model_name.starts_with("claude-opus-4") {
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
