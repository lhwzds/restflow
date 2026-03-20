use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::Qwen3Max,
        Provider::Qwen,
        "qwen3-max",
        "Qwen3 Max",
        true,
    )
    .with_openrouter_equivalent(ModelId::OrQwen3Coder),
    ModelDescriptor::new(
        ModelId::Qwen3Plus,
        Provider::Qwen,
        "qwen3-plus",
        "Qwen3 Plus",
        true,
    )
    .with_openrouter_equivalent(ModelId::OrQwen3Coder),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Qwen, ModelId::Qwen3Max, MODELS);
