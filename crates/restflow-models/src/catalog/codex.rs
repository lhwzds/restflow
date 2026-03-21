use super::{ModelDescriptor, ProviderCatalog};
use crate::{ClientKind, ModelId, Provider};

const GPT_5_4_ALIASES: &[&str] = &["gpt-5.4-codex"];
const GPT_5_4_MINI_ALIASES: &[&str] = &["gpt-5.4-mini-codex"];

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::Gpt5_4Codex,
        Provider::Codex,
        "gpt-5.4",
        "GPT-5.4",
        false,
    )
    .with_aliases(GPT_5_4_ALIASES)
    .with_client_kind(ClientKind::CodexCli)
    .with_same_provider_fallback(ModelId::Gpt5_4MiniCodex),
    ModelDescriptor::new(
        ModelId::Gpt5_4MiniCodex,
        Provider::Codex,
        "gpt-5.4-mini",
        "GPT-5.4 Mini",
        false,
    )
    .with_aliases(GPT_5_4_MINI_ALIASES)
    .with_client_kind(ClientKind::CodexCli),
    ModelDescriptor::new(
        ModelId::Gpt5Codex,
        Provider::Codex,
        "gpt-5-codex",
        "Codex GPT-5",
        false,
    )
    .with_client_kind(ClientKind::CodexCli),
    ModelDescriptor::new(
        ModelId::Gpt5_1Codex,
        Provider::Codex,
        "gpt-5.1-codex",
        "Codex GPT-5.1",
        false,
    )
    .with_client_kind(ClientKind::CodexCli),
    ModelDescriptor::new(
        ModelId::Gpt5_2Codex,
        Provider::Codex,
        "gpt-5.2-codex",
        "Codex GPT-5.2",
        false,
    )
    .with_client_kind(ClientKind::CodexCli),
    ModelDescriptor::new(
        ModelId::CodexCli,
        Provider::Codex,
        "gpt-5.3-codex",
        "Codex GPT-5.3",
        false,
    )
    .with_client_kind(ClientKind::CodexCli),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Codex, ModelId::Gpt5_4Codex, MODELS);
