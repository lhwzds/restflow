use restflow_contracts::request::WireModelRef;
use serde::{Deserialize, Serialize};
use specta::Type;
use ts_rs::TS;

use super::{ModelId, Provider, ValidationError};

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

impl TryFrom<WireModelRef> for ModelRef {
    type Error = ValidationError;

    fn try_from(value: WireModelRef) -> Result<Self, Self::Error> {
        let provider = Provider::from_canonical_str(&value.provider).ok_or_else(|| {
            ValidationError::new(
                "model_ref.provider",
                format!("unknown provider '{}'", value.provider),
            )
        })?;
        let model = ModelId::for_provider_and_model(provider, &value.model)
            .or_else(|| ModelId::from_api_name(&value.model))
            .or_else(|| ModelId::from_canonical_id(&value.model))
            .or_else(|| ModelId::from_serialized_str(&value.model))
            .ok_or_else(|| {
                ValidationError::new(
                    "model_ref.model",
                    format!("unknown model '{}'", value.model),
                )
            })?;

        let model_ref = Self { provider, model }.normalized();
        model_ref.validate()?;
        Ok(model_ref)
    }
}

impl From<ModelRef> for WireModelRef {
    fn from(value: ModelRef) -> Self {
        let normalized = value.normalized();
        Self {
            provider: normalized.provider.as_canonical_str().to_string(),
            model: normalized.model.as_serialized_str().to_string(),
        }
    }
}
