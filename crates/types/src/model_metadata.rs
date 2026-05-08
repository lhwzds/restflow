use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{ModelId, Provider};

/// Model metadata containing provider, temperature support, and display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelMetadata {
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: &'static str,
}

/// Serializable model metadata for runtime clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelMetadataDTO {
    pub model: ModelId,
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: String,
}
