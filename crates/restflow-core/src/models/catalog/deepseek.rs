use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

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
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::DeepSeek, ModelId::DeepseekChat, MODELS);
