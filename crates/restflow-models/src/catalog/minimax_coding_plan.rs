use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

const M21_ALIASES: &[&str] = &["minimax-coding-plan-m2.1", "minimax-m2-1", "minimax-m2.1"];
const M25_ALIASES: &[&str] = &[
    "minimax-coding-plan-m2.5",
    "minimax-m2-5",
    "minimax-m2.5",
    "minimax-coding-plan",
    "minimax-coding",
    "coding-plan-minimax",
    "minimax/coding-plan",
];
const M25_HIGHSPEED_ALIASES: &[&str] = &[
    "minimax-coding-plan-m2.5-highspeed",
    "minimax-m2-5-highspeed",
    "minimax-m2.5-highspeed",
];
const M27_ALIASES: &[&str] = &["minimax-coding-plan-m2.7", "minimax-m2-7", "minimax-m2.7"];
const M27_HIGHSPEED_ALIASES: &[&str] = &[
    "minimax-coding-plan-m2.7-highspeed",
    "minimax-m2-7-highspeed",
    "minimax-m2.7-highspeed",
];

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::MiniMaxM21CodingPlan,
        Provider::MiniMaxCodingPlan,
        "MiniMax-M2.1",
        "MiniMax M2.1 (Coding Plan)",
        true,
    )
    .with_aliases(M21_ALIASES)
    .with_canonical_family("minimax-m2-1")
    .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
    ModelDescriptor::new(
        ModelId::MiniMaxM25CodingPlan,
        Provider::MiniMaxCodingPlan,
        "MiniMax-M2.5",
        "MiniMax M2.5 (Coding Plan)",
        true,
    )
    .with_aliases(M25_ALIASES)
    .with_canonical_family("minimax-m2-5")
    .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
    ModelDescriptor::new(
        ModelId::MiniMaxM25CodingPlanHighspeed,
        Provider::MiniMaxCodingPlan,
        "MiniMax-M2.5-highspeed",
        "MiniMax M2.5 Highspeed (Coding Plan)",
        true,
    )
    .with_aliases(M25_HIGHSPEED_ALIASES)
    .with_canonical_family("minimax-m2-5-highspeed")
    .with_same_provider_fallback(ModelId::MiniMaxM25CodingPlan)
    .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
    ModelDescriptor::new(
        ModelId::MiniMaxM27CodingPlan,
        Provider::MiniMaxCodingPlan,
        "MiniMax-M2.7",
        "MiniMax M2.7 (Coding Plan)",
        true,
    )
    .with_aliases(M27_ALIASES)
    .with_canonical_family("minimax-m2-7")
    .with_same_provider_fallback(ModelId::MiniMaxM25CodingPlan)
    .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
    ModelDescriptor::new(
        ModelId::MiniMaxM27CodingPlanHighspeed,
        Provider::MiniMaxCodingPlan,
        "MiniMax-M2.7-highspeed",
        "MiniMax M2.7 Highspeed (Coding Plan)",
        true,
    )
    .with_aliases(M27_HIGHSPEED_ALIASES)
    .with_canonical_family("minimax-m2-7-highspeed")
    .with_same_provider_fallback(ModelId::MiniMaxM27CodingPlan)
    .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
];

// Keep the best-quality flagship distinct from the conservative default model.
// Provider metadata owns the default selection (M2.5), while the catalog
// flagship remains the recommended top-end coding-plan model (M2.7).
pub const CATALOG: ProviderCatalog = ProviderCatalog::new(
    Provider::MiniMaxCodingPlan,
    ModelId::MiniMaxM27CodingPlan,
    MODELS,
);

#[cfg(test)]
mod tests {
    use super::CATALOG;
    use crate::{ModelId, provider_meta};
    use restflow_traits::ModelProvider;

    #[test]
    fn default_model_is_intentionally_distinct_from_flagship() {
        let provider_meta = provider_meta(ModelProvider::MiniMaxCodingPlan);

        assert_eq!(
            provider_meta.default_model_id,
            ModelId::MiniMaxM25CodingPlan
        );
        assert_eq!(CATALOG.flagship, ModelId::MiniMaxM27CodingPlan);
        assert_ne!(provider_meta.default_model_id, CATALOG.flagship);
    }

    #[test]
    fn catalog_contains_default_and_flagship_models() {
        assert!(
            CATALOG
                .models
                .iter()
                .any(|descriptor| descriptor.id == ModelId::MiniMaxM25CodingPlan)
        );
        assert!(
            CATALOG
                .models
                .iter()
                .any(|descriptor| descriptor.id == ModelId::MiniMaxM27CodingPlan)
        );
    }
}
