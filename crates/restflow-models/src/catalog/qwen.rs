use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

const QWEN3_MAX_ALIASES: &[&str] = &["qwen-max", "qwen"];
const QWEN3_PLUS_ALIASES: &[&str] = &["qwen-plus"];

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::Qwen3Max,
        Provider::Qwen,
        "qwen3-max",
        "Qwen3 Max",
        true,
    )
    .with_aliases(QWEN3_MAX_ALIASES)
    .with_openrouter_equivalent(ModelId::OrQwen3Coder),
    ModelDescriptor::new(
        ModelId::Qwen3Plus,
        Provider::Qwen,
        "qwen3-plus",
        "Qwen3 Plus",
        true,
    )
    .with_aliases(QWEN3_PLUS_ALIASES)
    .with_openrouter_equivalent(ModelId::OrQwen3Coder),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Qwen, ModelId::Qwen3Max, MODELS);
