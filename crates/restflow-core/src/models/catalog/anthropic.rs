use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

const OPUS_ALIASES: &[&str] = &["claude-opus-4-6-20260205", "claude-opus-4-6-20250514"];
const SONNET_ALIASES: &[&str] = &["claude-sonnet-4-5-20250514", "claude-sonnet-4-20250514"];
const HAIKU_ALIASES: &[&str] = &["claude-haiku-4-5-20250514", "claude-haiku-4-20250514"];
const OPUS_PREFIX_ALIASES: &[&str] = &["claude-opus-4-6", "claude-opus-4"];
const SONNET_PREFIX_ALIASES: &[&str] = &["claude-sonnet-4"];
const HAIKU_PREFIX_ALIASES: &[&str] = &["claude-haiku-4"];

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::ClaudeOpus4_6,
        Provider::Anthropic,
        "claude-opus-4-6",
        "Claude Opus 4.6",
        true,
    )
    .with_aliases(OPUS_ALIASES)
    .with_prefix_aliases(OPUS_PREFIX_ALIASES)
    .with_same_provider_fallback(ModelId::ClaudeSonnet4_5)
    .with_openrouter_equivalent(ModelId::OrClaudeOpus4_6),
    ModelDescriptor::new(
        ModelId::ClaudeSonnet4_5,
        Provider::Anthropic,
        "claude-sonnet-4-5",
        "Claude Sonnet 4.5",
        true,
    )
    .with_aliases(SONNET_ALIASES)
    .with_prefix_aliases(SONNET_PREFIX_ALIASES)
    .with_same_provider_fallback(ModelId::ClaudeHaiku4_5)
    .with_openrouter_equivalent(ModelId::OrClaudeOpus4_6),
    ModelDescriptor::new(
        ModelId::ClaudeHaiku4_5,
        Provider::Anthropic,
        "claude-haiku-4-5",
        "Claude Haiku 4.5",
        true,
    )
    .with_aliases(HAIKU_ALIASES)
    .with_prefix_aliases(HAIKU_PREFIX_ALIASES),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Anthropic, ModelId::ClaudeSonnet4_5, MODELS);
