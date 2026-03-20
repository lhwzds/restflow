use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
    ModelId::SiliconFlowAuto,
    Provider::SiliconFlow,
    "deepseek-ai/DeepSeek-V3",
    "SiliconFlow Auto",
    true,
)];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::SiliconFlow, ModelId::SiliconFlowAuto, MODELS);
