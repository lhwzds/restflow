use anyhow::Result;

use crate::models::{ExecutionTraceCategory, ExecutionTraceEvent};
use crate::storage::{
    ChatSessionStorage, ExecutionTraceStorage, ProviderHealthSnapshotStorage,
    StructuredExecutionLogStorage, TelemetryMetricSampleStorage,
};

pub trait TelemetryProjector: Send + Sync {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()>;
}

#[derive(Clone)]
pub struct ExecutionTraceProjector {
    storage: ExecutionTraceStorage,
}

impl ExecutionTraceProjector {
    pub fn new(storage: ExecutionTraceStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for ExecutionTraceProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        self.storage.store(event)
    }
}

#[derive(Clone)]
pub struct SessionProjectionProjector {
    storage: ChatSessionStorage,
}

impl SessionProjectionProjector {
    pub fn new(storage: ChatSessionStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for SessionProjectionProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        let Some(session_id) = event.session_id.as_deref() else {
            return Ok(());
        };
        let Some(mut session) = self.storage.get(session_id)? else {
            return Ok(());
        };

        let mut changed = false;

        if let Some(effective_model) = event.effective_model.as_deref()
            && session.metadata.last_model.as_deref() != Some(effective_model)
        {
            session.metadata.last_model = Some(effective_model.to_string());
            changed = true;
        }

        if let Some(llm_call) = event.llm_call.as_ref() {
            if let Some(prompt_tokens) = llm_call.input_tokens {
                session.prompt_tokens += i64::from(prompt_tokens);
                changed = true;
            }
            if let Some(completion_tokens) = llm_call.output_tokens {
                session.completion_tokens += i64::from(completion_tokens);
                changed = true;
            }
            if let Some(cost_usd) = llm_call.cost_usd {
                session.cost += cost_usd;
                changed = true;
            }
        }

        if !changed {
            return Ok(());
        }

        self.storage.update(&session)
    }
}

#[derive(Clone)]
pub struct MetricsProjector {
    storage: TelemetryMetricSampleStorage,
}

impl MetricsProjector {
    pub fn new(storage: TelemetryMetricSampleStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for MetricsProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        if event.category == ExecutionTraceCategory::MetricSample {
            self.storage.store(event)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct ProviderHealthProjector {
    storage: ProviderHealthSnapshotStorage,
}

impl ProviderHealthProjector {
    pub fn new(storage: ProviderHealthSnapshotStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for ProviderHealthProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        if event.category == ExecutionTraceCategory::ProviderHealth {
            self.storage.store(event)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct StructuredLogProjector {
    storage: StructuredExecutionLogStorage,
}

impl StructuredLogProjector {
    pub fn new(storage: StructuredExecutionLogStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for StructuredLogProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        if event.category == ExecutionTraceCategory::LogRecord {
            self.storage.store(event)?;
        }
        Ok(())
    }
}
