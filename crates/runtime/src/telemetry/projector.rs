use anyhow::Result;

use crate::models::ExecutionTraceEvent;
use crate::storage::ExecutionTraceStorage;

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
