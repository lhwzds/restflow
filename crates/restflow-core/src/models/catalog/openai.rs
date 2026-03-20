use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(ModelId::Gpt5, Provider::OpenAI, "gpt-5", "GPT-5", false)
        .with_same_provider_fallback(ModelId::Gpt5Mini)
        .with_openrouter_equivalent(ModelId::OrGpt5),
    ModelDescriptor::new(
        ModelId::Gpt5Mini,
        Provider::OpenAI,
        "gpt-5-mini",
        "GPT-5 Mini",
        false,
    )
    .with_same_provider_fallback(ModelId::Gpt5Nano)
    .with_openrouter_equivalent(ModelId::OrGpt5),
    ModelDescriptor::new(
        ModelId::Gpt5Nano,
        Provider::OpenAI,
        "gpt-5-nano",
        "GPT-5 Nano",
        false,
    ),
    ModelDescriptor::new(
        ModelId::Gpt5Pro,
        Provider::OpenAI,
        "gpt-5-pro",
        "GPT-5 Pro",
        false,
    )
    .with_same_provider_fallback(ModelId::Gpt5)
    .with_openrouter_equivalent(ModelId::OrGpt5),
    ModelDescriptor::new(
        ModelId::Gpt5_1,
        Provider::OpenAI,
        "gpt-5.1",
        "GPT-5.1",
        false,
    ),
    ModelDescriptor::new(
        ModelId::Gpt5_2,
        Provider::OpenAI,
        "gpt-5.2",
        "GPT-5.2",
        false,
    ),
    ModelDescriptor::new(
        ModelId::OpenCodeCli,
        Provider::OpenAI,
        "opencode",
        "OpenCode CLI",
        false,
    ),
];

pub const CATALOG: ProviderCatalog = ProviderCatalog::new(Provider::OpenAI, ModelId::Gpt5, MODELS);
