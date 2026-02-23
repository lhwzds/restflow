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
    pub cache_read_per_1m: Option<f64>,
    pub cache_write_per_1m: Option<f64>,
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

/// Canonical providers whose pricing should take precedence over third-party resellers.
const CANONICAL_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "deepseek",
    "google",
    "azure",
    "amazon-bedrock",
];

fn is_canonical_provider(provider_name: &str) -> bool {
    let name = provider_name.to_ascii_lowercase();
    CANONICAL_PROVIDERS.iter().any(|&p| name == p)
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

    // Track which keys were set by canonical providers so we don't overwrite them.
    let mut canonical_keys: HashSet<String> = HashSet::new();
    let mut by_model = HashMap::new();

    // Two-pass: canonical providers first, then others fill gaps.
    for (provider_name, provider) in &root {
        if !is_canonical_provider(provider_name) {
            continue;
        }
        for (model_key, model) in &provider.models {
            let Some(ref cost) = model.cost else {
                continue;
            };
            if cost.input == 0.0 && cost.output == 0.0 {
                continue;
            }
            let pricing = ModelPricing {
                cost_per_1m_input: cost.input,
                cost_per_1m_output: cost.output,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            };
            let key = normalize(model_key);
            by_model.insert(key.clone(), pricing);
            canonical_keys.insert(key);
            if let Some(id) = model.id.as_deref() {
                let id_key = normalize(id);
                by_model.insert(id_key.clone(), pricing);
                canonical_keys.insert(id_key);
            }
        }
    }

    // Second pass: non-canonical providers fill in models not yet covered.
    for (provider_name, provider) in &root {
        if is_canonical_provider(provider_name) {
            continue;
        }
        for (model_key, model) in &provider.models {
            let Some(ref cost) = model.cost else {
                continue;
            };
            if cost.input == 0.0 && cost.output == 0.0 {
                continue;
            }
            let pricing = ModelPricing {
                cost_per_1m_input: cost.input,
                cost_per_1m_output: cost.output,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            };
            let key = normalize(model_key);
            if !canonical_keys.contains(&key) {
                by_model.entry(key).or_insert(pricing);
            }
            if let Some(id) = model.id.as_deref() {
                let id_key = normalize(id);
                if !canonical_keys.contains(&id_key) {
                    by_model.entry(id_key).or_insert(pricing);
                }
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
            cache_read_per_1m: None,
            cache_write_per_1m: None,
        });
    }
    if model_name.starts_with("gpt-5-mini") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.4,
            cost_per_1m_output: 1.6,
            cache_read_per_1m: None,
            cache_write_per_1m: None,
        });
    }
    if model_name.starts_with("gpt-5-nano") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.1,
            cost_per_1m_output: 0.4,
            cache_read_per_1m: None,
            cache_write_per_1m: None,
        });
    }
    if model_name.starts_with("gpt-5") || model_name == "gpt-5" {
        return Some(ModelPricing {
            cost_per_1m_input: 1.25,
            cost_per_1m_output: 10.0,
            cache_read_per_1m: None,
            cache_write_per_1m: None,
        });
    }

    // Anthropic
    if model_name.starts_with("claude-opus-4-6") || model_name.starts_with("claude-opus-4") {
        return Some(ModelPricing {
            cost_per_1m_input: 15.0,
            cost_per_1m_output: 75.0,
            cache_read_per_1m: None,
            cache_write_per_1m: None,
        });
    }
    if model_name.starts_with("claude-sonnet-4") {
        return Some(ModelPricing {
            cost_per_1m_input: 3.0,
            cost_per_1m_output: 15.0,
            cache_read_per_1m: None,
            cache_write_per_1m: None,
        });
    }
    if model_name.starts_with("claude-haiku-4") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.8,
            cost_per_1m_output: 4.0,
            cache_read_per_1m: None,
            cache_write_per_1m: None,
        });
    }

    // DeepSeek
    if model_name.starts_with("deepseek-reasoner") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.55,
            cost_per_1m_output: 2.19,
            cache_read_per_1m: None,
            cache_write_per_1m: None,
        });
    }
    if model_name.starts_with("deepseek-chat") || model_name.starts_with("deepseek") {
        return Some(ModelPricing {
            cost_per_1m_input: 0.27,
            cost_per_1m_output: 1.10,
            cache_read_per_1m: None,
            cache_write_per_1m: None,
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
        // Hardcoded fallback: input=3.0, output=15.0
        // Dynamic cache may override if models.json exists; canonical provider pricing is preferred.
        assert!(
            pricing.cost_per_1m_input > 0.0 && pricing.cost_per_1m_input <= 5.0,
            "Sonnet input price {} out of expected range",
            pricing.cost_per_1m_input
        );
        assert!(
            pricing.cost_per_1m_output > 0.0 && pricing.cost_per_1m_output <= 20.0,
            "Sonnet output price {} out of expected range",
            pricing.cost_per_1m_output
        );
    }

    #[test]
    fn test_pricing_openai_gpt5() {
        let pricing = get_pricing("gpt-5").unwrap();
        // Hardcoded fallback: input=1.25, output=10.0
        // Dynamic cache may override if models.json exists; canonical provider pricing is preferred.
        assert!(
            pricing.cost_per_1m_input > 0.0 && pricing.cost_per_1m_input <= 5.0,
            "GPT-5 input price {} out of expected range",
            pricing.cost_per_1m_input
        );
        assert!(
            pricing.cost_per_1m_output > 0.0 && pricing.cost_per_1m_output <= 20.0,
            "GPT-5 output price {} out of expected range",
            pricing.cost_per_1m_output
        );
    }

    #[test]
    fn test_pricing_cli_models_none() {
        assert!(get_pricing("opus").is_none());
        assert!(get_pricing("sonnet").is_none());
        assert!(get_pricing("gpt-5.3-codex").is_none());
    }

    #[test]
    fn test_calculate_cost() {
        // Use the actual pricing returned by get_pricing (may come from dynamic cache)
        let pricing = get_pricing("claude-sonnet-4-20250514").unwrap();
        let expected = (1000.0 / 1_000_000.0) * pricing.cost_per_1m_input
            + (500.0 / 1_000_000.0) * pricing.cost_per_1m_output;
        let cost = calculate_cost("claude-sonnet-4-20250514", 1000, 500).unwrap();
        assert!(
            (cost - expected).abs() < 1e-10,
            "cost={cost}, expected={expected}"
        );
    }

    #[test]
    fn test_calculate_cost_zero_tokens() {
        let cost = calculate_cost("claude-sonnet-4-20250514", 0, 0).unwrap();
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_canonical_provider_priority() {
        // Verify the canonical provider list is reasonable
        assert!(is_canonical_provider("anthropic"));
        assert!(is_canonical_provider("openai"));
        assert!(is_canonical_provider("deepseek"));
        assert!(is_canonical_provider("google"));
        assert!(!is_canonical_provider("aihubmix"));
        assert!(!is_canonical_provider("jiekou"));
    }

    #[test]
    fn test_pricing_candidates_generation() {
        let candidates = pricing_candidates("claude-sonnet-4-20250514");
        assert!(candidates.contains(&"claude-sonnet-4-20250514".to_string()));

        let candidates = pricing_candidates("gpt-5");
        assert!(candidates.contains(&"gpt-5".to_string()));
    }
}
