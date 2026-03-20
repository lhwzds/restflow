use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::GroqLlama4Scout,
        Provider::Groq,
        "meta-llama/llama-4-scout-17b-16e-instruct",
        "Llama 4 Scout",
        true,
    ),
    ModelDescriptor::new(
        ModelId::GroqLlama4Maverick,
        Provider::Groq,
        "meta-llama/llama-4-maverick-17b-128e-instruct",
        "Llama 4 Maverick",
        true,
    ),
];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Groq, ModelId::GroqLlama4Maverick, MODELS);
