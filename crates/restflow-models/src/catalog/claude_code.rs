use super::{ModelDescriptor, ProviderCatalog};
use crate::{ClientKind, ModelId, Provider};

const OPUS_ALIASES: &[&str] = &["claude-code-opus", "opus"];
const SONNET_ALIASES: &[&str] = &["claude-code-sonnet", "sonnet"];
const HAIKU_ALIASES: &[&str] = &["claude-code-haiku", "haiku"];

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::ClaudeCodeOpus,
        Provider::ClaudeCode,
        "opus",
        "Claude Code Opus",
        true,
    )
    .with_aliases(OPUS_ALIASES)
    .with_client_kind(ClientKind::ClaudeCodeCli),
    ModelDescriptor::new(
        ModelId::ClaudeCodeSonnet,
        Provider::ClaudeCode,
        "sonnet",
        "Claude Code Sonnet",
        true,
    )
    .with_aliases(SONNET_ALIASES)
    .with_client_kind(ClientKind::ClaudeCodeCli),
    ModelDescriptor::new(
        ModelId::ClaudeCodeHaiku,
        Provider::ClaudeCode,
        "haiku",
        "Claude Code Haiku",
        true,
    )
    .with_aliases(HAIKU_ALIASES)
    .with_client_kind(ClientKind::ClaudeCodeCli),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::ClaudeCode, ModelId::ClaudeCodeOpus, MODELS);
