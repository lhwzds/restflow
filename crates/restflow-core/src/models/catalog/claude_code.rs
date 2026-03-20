use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

const OPUS_ALIASES: &[&str] = &["opus"];
const SONNET_ALIASES: &[&str] = &["sonnet"];
const HAIKU_ALIASES: &[&str] = &["haiku"];

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::ClaudeCodeOpus,
        Provider::ClaudeCode,
        "opus",
        "Claude Code Opus",
        true,
    )
    .with_aliases(OPUS_ALIASES),
    ModelDescriptor::new(
        ModelId::ClaudeCodeSonnet,
        Provider::ClaudeCode,
        "sonnet",
        "Claude Code Sonnet",
        true,
    )
    .with_aliases(SONNET_ALIASES),
    ModelDescriptor::new(
        ModelId::ClaudeCodeHaiku,
        Provider::ClaudeCode,
        "haiku",
        "Claude Code Haiku",
        true,
    )
    .with_aliases(HAIKU_ALIASES),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::ClaudeCode, ModelId::ClaudeCodeOpus, MODELS);
