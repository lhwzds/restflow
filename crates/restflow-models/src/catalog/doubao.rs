use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
    ModelId::DoubaoPro,
    Provider::Doubao,
    "doubao-pro-256k",
    "Doubao Pro",
    true,
)];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Doubao, ModelId::DoubaoPro, MODELS);
