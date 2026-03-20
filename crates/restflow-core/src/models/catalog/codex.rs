use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::Gpt5_4Codex,
        Provider::Codex,
        "gpt-5.4",
        "GPT-5.4",
        false,
    )
    .with_same_provider_fallback(ModelId::Gpt5_4MiniCodex),
    ModelDescriptor::new(
        ModelId::Gpt5_4MiniCodex,
        Provider::Codex,
        "gpt-5.4-mini",
        "GPT-5.4 Mini",
        false,
    ),
    ModelDescriptor::new(
        ModelId::Gpt5Codex,
        Provider::Codex,
        "gpt-5-codex",
        "Codex GPT-5",
        false,
    ),
    ModelDescriptor::new(
        ModelId::Gpt5_1Codex,
        Provider::Codex,
        "gpt-5.1-codex",
        "Codex GPT-5.1",
        false,
    ),
    ModelDescriptor::new(
        ModelId::Gpt5_2Codex,
        Provider::Codex,
        "gpt-5.2-codex",
        "Codex GPT-5.2",
        false,
    ),
    ModelDescriptor::new(
        ModelId::CodexCli,
        Provider::Codex,
        "gpt-5.3-codex",
        "Codex GPT-5.3",
        false,
    ),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Codex, ModelId::Gpt5_4Codex, MODELS);
