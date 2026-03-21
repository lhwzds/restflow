use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

const SILICONFLOW_AUTO_ALIASES: &[&str] = &["siliconflow"];

pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
    ModelId::SiliconFlowAuto,
    Provider::SiliconFlow,
    "deepseek-ai/DeepSeek-V3",
    "SiliconFlow Auto",
    true,
)
.with_aliases(SILICONFLOW_AUTO_ALIASES)];

pub const CATALOG: ProviderCatalog =
    ProviderCatalog::new(Provider::SiliconFlow, ModelId::SiliconFlowAuto, MODELS);
