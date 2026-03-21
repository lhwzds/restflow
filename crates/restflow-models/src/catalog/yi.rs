use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

const YI_ALIASES: &[&str] = &["yi"];

pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
    ModelId::YiLightning,
    Provider::Yi,
    "yi-lightning",
    "Yi Lightning",
    true,
)
.with_aliases(YI_ALIASES)];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Yi, ModelId::YiLightning, MODELS);
