use serde::{Deserialize, Serialize};
use specta::Type;
use ts_rs::TS;

use super::{Provider, ValidationError, model_id::ModelId};

/// Model metadata containing provider, temperature support, and display name
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelMetadata {
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: &'static str,
}

/// Serializable model metadata for transferring to frontend
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct ModelMetadataDTO {
    pub model: ModelId,
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: String,
}

/// Provider + model pair used by API and persistence layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct ModelRef {
    pub provider: Provider,
    pub model: ModelId,
}

impl ModelRef {
    /// Build a consistent model reference from a model enum.
    pub fn from_model(model: ModelId) -> Self {
        Self {
            provider: model.provider(),
            model,
        }
    }

    /// Validate that provider and model provider metadata are consistent.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let normalized = self.normalized();
        let expected_provider = normalized.model.provider();
        if normalized.provider != expected_provider {
            return Err(ValidationError::new(
                "model_ref",
                format!(
                    "provider '{}' does not match model provider '{}'",
                    normalized.provider.as_canonical_str(),
                    expected_provider.as_canonical_str()
                ),
            ));
        }
        Ok(())
    }

    /// Return canonical ID in `provider:model` format.
    pub fn canonical_id(&self) -> String {
        let normalized = self.normalized();
        format!(
            "{}:{}",
            normalized.provider.as_canonical_str(),
            normalized.model.as_serialized_str()
        )
    }

    /// Normalize legacy provider/model combinations into canonical provider identities.
    pub fn normalized(&self) -> Self {
        Self {
            provider: ModelId::normalize_provider_for_model(self.model, self.provider),
            model: self.model,
        }
    }
}
