use crate::models::ModelId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSwitchRecord {
    pub from_model: ModelId,
    pub to_model: ModelId,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResolution {
    pub requested_model: ModelId,
    pub effective_model: Option<ModelId>,
    pub switch_chain: Vec<ModelSwitchRecord>,
    pub provider: crate::models::Provider,
    pub attempt_count: u32,
}

impl ExecutionResolution {
    pub fn new(requested_model: ModelId) -> Self {
        Self {
            provider: requested_model.provider(),
            requested_model,
            effective_model: None,
            switch_chain: Vec::new(),
            attempt_count: 0,
        }
    }

    pub fn with_effective_model(mut self, model: ModelId) -> Self {
        self.effective_model = Some(model);
        self
    }

    pub fn with_attempt_count(mut self, attempt_count: u32) -> Self {
        self.attempt_count = attempt_count;
        self
    }

    pub fn push_switch(&mut self, from_model: ModelId, to_model: ModelId, reason: Option<String>) {
        self.switch_chain.push(ModelSwitchRecord {
            from_model,
            to_model,
            reason,
        });
    }
}
