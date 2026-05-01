use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::DeepseekChat,
        Provider::DeepSeek,
        "deepseek-chat",
        "DeepSeek Chat",
        true,
    )
    .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
    ModelDescriptor::new(
        ModelId::DeepseekReasoner,
        Provider::DeepSeek,
        "deepseek-reasoner",
        "DeepSeek Reasoner",
        true,
    )
    .with_same_provider_fallback(ModelId::DeepseekChat)
    .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
    ModelDescriptor::new(
        ModelId::DeepseekV4Pro,
        Provider::DeepSeek,
        "deepseek-v4-pro",
        "DeepSeek V4 Pro",
        true,
    )
    .with_same_provider_fallback(ModelId::DeepseekV4Flash)
    .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
    ModelDescriptor::new(
        ModelId::DeepseekV4Flash,
        Provider::DeepSeek,
        "deepseek-v4-flash",
        "DeepSeek V4 Flash",
        true,
    )
    .with_same_provider_fallback(ModelId::DeepseekChat)
    .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::DeepSeek, ModelId::DeepseekV4Pro, MODELS);
