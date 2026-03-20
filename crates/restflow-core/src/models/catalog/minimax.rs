use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

const M21_ALIASES: &[&str] = &["minimax-m2.1"];
const M25_ALIASES: &[&str] = &["minimax-m2.5"];
const M27_ALIASES: &[&str] = &["minimax-m2.7"];
const M27_HIGHSPEED_ALIASES: &[&str] = &["minimax-m2.7-highspeed"];

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::MiniMaxM21,
        Provider::MiniMax,
        "MiniMax-M2.1",
        "MiniMax M2.1",
        true,
    )
    .with_aliases(M21_ALIASES)
    .with_canonical_family("minimax-m2-1")
    .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
    ModelDescriptor::new(
        ModelId::MiniMaxM25,
        Provider::MiniMax,
        "MiniMax-M2.5",
        "MiniMax M2.5",
        true,
    )
    .with_aliases(M25_ALIASES)
    .with_canonical_family("minimax-m2-5")
    .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
    ModelDescriptor::new(
        ModelId::MiniMaxM27,
        Provider::MiniMax,
        "MiniMax-M2.7",
        "MiniMax M2.7",
        true,
    )
    .with_aliases(M27_ALIASES)
    .with_canonical_family("minimax-m2-7")
    .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
    ModelDescriptor::new(
        ModelId::MiniMaxM27Highspeed,
        Provider::MiniMax,
        "MiniMax-M2.7-highspeed",
        "MiniMax M2.7 Highspeed",
        true,
    )
    .with_aliases(M27_HIGHSPEED_ALIASES)
    .with_canonical_family("minimax-m2-7-highspeed")
    .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::MiniMax, ModelId::MiniMaxM27, MODELS);
