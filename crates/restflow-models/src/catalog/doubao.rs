use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

const DOUBAO_ALIASES: &[&str] = &["doubao-pro", "doubao"];

pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
    ModelId::DoubaoPro,
    Provider::Doubao,
    "doubao-pro-256k",
    "Doubao Pro",
    true,
)
.with_aliases(DOUBAO_ALIASES)];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Doubao, ModelId::DoubaoPro, MODELS);
