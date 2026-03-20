use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
    ModelId::YiLightning,
    Provider::Yi,
    "yi-lightning",
    "Yi Lightning",
    true,
)];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Yi, ModelId::YiLightning, MODELS);
