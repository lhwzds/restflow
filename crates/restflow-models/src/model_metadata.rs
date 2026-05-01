use serde::{Deserialize, Serialize};
use specta::Type;
use ts_rs::TS;

use crate::{ModelId, Provider};

/// Model metadata containing provider, temperature support, and display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelMetadata {
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: &'static str,
}

/// Serializable model metadata for transferring to frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct ModelMetadataDTO {
    pub model: ModelId,
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: String,
}
