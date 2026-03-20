use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::Gemini25Pro,
        Provider::Google,
        "gemini-2.5-pro",
        "Gemini 2.5 Pro",
        true,
    )
    .with_same_provider_fallback(ModelId::Gemini25Flash)
    .with_openrouter_equivalent(ModelId::OrGemini3Pro),
    ModelDescriptor::new(
        ModelId::Gemini25Flash,
        Provider::Google,
        "gemini-2.5-flash",
        "Gemini 2.5 Flash",
        true,
    )
    .with_same_provider_fallback(ModelId::Gemini3Flash)
    .with_openrouter_equivalent(ModelId::OrGemini3Pro),
    ModelDescriptor::new(
        ModelId::Gemini3Pro,
        Provider::Google,
        "gemini-3-pro-preview",
        "Gemini 3 Pro Preview",
        true,
    ),
    ModelDescriptor::new(
        ModelId::Gemini3Flash,
        Provider::Google,
        "gemini-3-flash-preview",
        "Gemini 3 Flash Preview",
        true,
    ),
    ModelDescriptor::new(
        ModelId::GeminiCli,
        Provider::Google,
        "gemini-2.5-pro",
        "Gemini CLI",
        false,
    ),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Google, ModelId::Gemini3Pro, MODELS);
