use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

const KIMI_ALIASES: &[&str] = &["kimi-k2-5", "kimi", "moonshot"];

pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
    ModelId::KimiK2_5,
    Provider::Moonshot,
    "kimi-k2.5",
    "Kimi K2.5",
    true,
)
.with_aliases(KIMI_ALIASES)
.with_openrouter_equivalent(ModelId::OrKimiK2_5)];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::Moonshot, ModelId::KimiK2_5, MODELS);
