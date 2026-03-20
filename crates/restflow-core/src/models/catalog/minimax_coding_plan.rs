use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

const M21_ALIASES: &[&str] = &["minimax-coding-plan-m2.1", "minimax-m2-1", "minimax-m2.1"];
const M25_ALIASES: &[&str] = &["minimax-coding-plan-m2.5", "minimax-m2-5", "minimax-m2.5"];
const M25_HIGHSPEED_ALIASES: &[&str] = &[
    "minimax-coding-plan-m2.5-highspeed",
    "minimax-m2-5-highspeed",
    "minimax-m2.5-highspeed",
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
];

pub const CATALOG: ProviderCatalog = ProviderCatalog::new(
    Provider::MiniMaxCodingPlan,
    ModelId::MiniMaxM25CodingPlan,
    MODELS,
);
