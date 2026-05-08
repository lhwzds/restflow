use restflow_traits::request::WireModelRef;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::{ModelId, Provider, ValidationError};

/// Provider + model pair used by API and persistence layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
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
        let expected_provider = self.model.provider();
        if self.provider != expected_provider {
            return Err(ValidationError::new(
                "model_ref",
                format!(
                    "provider '{}' does not match model provider '{}'",
                    self.provider.as_canonical_str(),
                    expected_provider.as_canonical_str()
                ),
            ));
        }
        Ok(())
    }

    /// Return canonical ID in `provider:model` format.
    pub fn canonical_id(&self) -> String {
        format!(
            "{}:{}",
            self.provider.as_canonical_str(),
            self.model.as_serialized_str()
        )
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
        let model = ModelId::for_provider_and_model(provider, &value.model).ok_or_else(|| {
            ValidationError::new(
                "model_ref.model",
                format!("unknown model '{}'", value.model),
            )
        })?;

        let model_ref = Self { provider, model };
        model_ref.validate()?;
        Ok(model_ref)
    }
}

impl From<ModelRef> for WireModelRef {
    fn from(value: ModelRef) -> Self {
        Self {
            provider: value.provider.as_canonical_str().to_string(),
            model: value.model.as_serialized_str().to_string(),
        }
    }
}
