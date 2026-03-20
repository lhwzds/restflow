use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(ModelId::Grok4, Provider::XAI, "grok-4", "Grok 4", true)
        .with_same_provider_fallback(ModelId::Grok3Mini)
        .with_openrouter_equivalent(ModelId::OrGrok4),
    ModelDescriptor::new(
        ModelId::Grok3Mini,
        Provider::XAI,
        "grok-3-mini",
        "Grok 3 Mini",
        true,
    )
    .with_openrouter_equivalent(ModelId::OrGrok4),
];

pub const CATALOG: ProviderCatalog = ProviderCatalog::new(Provider::XAI, ModelId::Grok4, MODELS);
