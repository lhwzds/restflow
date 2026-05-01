//! # codocia
//!
//! Model owns provider and model identity for the core.
//!
//! ## Owns
//! - provider identity
//! - model identity
//! - canonical model construction
//! - model catalog
//! - model selection
//!
//! ## Must Not
//! - read secrets
//! - call model providers
//! - depend on daemon state
//!
//! ## Inputs
//! - provider IDs
//! - model IDs
//! - model specs
//!
//! ## Outputs
//! - Model
//! - Provider
//! - ModelCatalog
//!
//! ## Used By
//! - agent
//! - auth
//!
//! ## Verify
//! - cargo check -p model

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
}

impl Provider {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Model {
    pub provider: Provider,
    pub id: String,
}

impl Model {
    pub fn new(provider: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            provider: Provider {
                id: provider.into(),
            },
            id: id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSpec {
    pub model: Model,
    pub name: String,
    pub description: Option<String>,
    pub client_model: Option<String>,
    pub client_kind: Option<String>,
    pub base_url: Option<String>,
}

impl ModelSpec {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            model: Model::new(provider, model),
            name: name.into(),
            description: None,
            client_model: None,
            client_kind: None,
            base_url: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_client_model(mut self, client_model: impl Into<String>) -> Self {
        self.client_model = Some(client_model.into());
        self
    }

    pub fn with_client_kind(mut self, client_kind: impl Into<String>) -> Self {
        self.client_kind = Some(client_kind.into());
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCatalog {
    specs: BTreeMap<(String, String), ModelSpec>,
}

impl ModelCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, spec: ModelSpec) {
        let key = (spec.model.provider.id.clone(), spec.model.id.clone());
        self.specs.insert(key, spec);
    }

    pub fn clear(&mut self) {
        self.specs.clear();
    }

    pub fn get(&self, provider: &str, model: &str) -> Option<&ModelSpec> {
        self.specs.get(&(provider.to_string(), model.to_string()))
    }

    pub fn list(&self) -> Vec<&ModelSpec> {
        self.specs.values().collect()
    }

    pub fn list_provider(&self, provider: &str) -> Vec<&ModelSpec> {
        self.specs
            .values()
            .filter(|spec| spec.model.provider.id == provider)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSelection {
    pub current: Model,
}

impl ModelSelection {
    pub fn new(current: Model) -> Self {
        Self { current }
    }

    pub fn switch(&mut self, model: Model) {
        self.current = model;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_lists_models_by_provider() {
        let mut catalog = ModelCatalog::new();
        catalog.insert(ModelSpec::new("openai", "gpt-5.5", "GPT-5.5"));
        catalog.insert(ModelSpec::new("openai", "gpt-5.4", "GPT-5.4"));
        catalog.insert(ModelSpec::new("deepseek", "deepseek-chat", "DeepSeek Chat"));

        assert_eq!(catalog.list().len(), 3);
        assert_eq!(catalog.list_provider("openai").len(), 2);
        assert_eq!(
            catalog.get("deepseek", "deepseek-chat").unwrap().name,
            "DeepSeek Chat"
        );
    }

    #[test]
    fn selection_switches_current_model() {
        let mut selection = ModelSelection::new(Model::new("openai", "gpt-5.4"));

        selection.switch(Model::new("openai", "gpt-5.5"));

        assert_eq!(selection.current.id, "gpt-5.5");
    }
}
