use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

const SCOUT_ALIASES: &[&str] = &["groq-scout", "llama-4-scout"];
const MAVERICK_ALIASES: &[&str] = &["groq-maverick", "llama-4-maverick"];

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::GroqLlama4Scout,
        Provider::Groq,
        "meta-llama/llama-4-scout-17b-16e-instruct",
        "Llama 4 Scout",
        true,
    )
    .with_aliases(SCOUT_ALIASES),
    ModelDescriptor::new(
        ModelId::GroqLlama4Maverick,
        Provider::Groq,
        "meta-llama/llama-4-maverick-17b-128e-instruct",
        "Llama 4 Maverick",
        true,
    )
    .with_aliases(MAVERICK_ALIASES),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Groq, ModelId::GroqLlama4Maverick, MODELS);
