use super::{ModelDescriptor, ProviderCatalog};
use crate::{ClientKind, ModelId, Provider};

const GEMINI_25_PRO_ALIASES: &[&str] = &["gemini-pro"];
const GEMINI_25_FLASH_ALIASES: &[&str] = &["gemini-flash"];
const GEMINI_3_PRO_ALIASES: &[&str] = &["gemini-3-pro"];
const GEMINI_3_FLASH_ALIASES: &[&str] = &["gemini-3-flash"];
const GEMINI_CLI_ALIASES: &[&str] = &["gemini-cli"];

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::Gemini25Pro,
        Provider::Google,
        "gemini-2.5-pro",
        "Gemini 2.5 Pro",
        true,
    )
    .with_aliases(GEMINI_25_PRO_ALIASES)
    .with_same_provider_fallback(ModelId::Gemini25Flash)
    .with_openrouter_equivalent(ModelId::OrGemini3Pro),
    ModelDescriptor::new(
        ModelId::Gemini25Flash,
        Provider::Google,
        "gemini-2.5-flash",
        "Gemini 2.5 Flash",
        true,
    )
    .with_aliases(GEMINI_25_FLASH_ALIASES)
    .with_same_provider_fallback(ModelId::Gemini3Flash)
    .with_openrouter_equivalent(ModelId::OrGemini3Pro),
    ModelDescriptor::new(
        ModelId::Gemini3Pro,
        Provider::Google,
        "gemini-3-pro-preview",
        "Gemini 3 Pro Preview",
        true,
    )
    .with_aliases(GEMINI_3_PRO_ALIASES),
    ModelDescriptor::new(
        ModelId::Gemini3Flash,
        Provider::Google,
        "gemini-3-flash-preview",
        "Gemini 3 Flash Preview",
        true,
    )
    .with_aliases(GEMINI_3_FLASH_ALIASES),
    ModelDescriptor::new(
        ModelId::GeminiCli,
        Provider::Google,
        "gemini-2.5-pro",
        "Gemini CLI",
        false,
    )
    .with_aliases(GEMINI_CLI_ALIASES)
    .with_client_kind(ClientKind::GeminiCli),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Google, ModelId::Gemini3Pro, MODELS);
