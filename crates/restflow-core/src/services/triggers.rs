use crate::models::ActiveTrigger;
use crate::storage::Storage;
use anyhow::{Context, Result};
use std::sync::Arc;
use uuid::Uuid;

// Trigger storage functions (simplified for Agent-centric architecture)

pub fn generate_test_execution_id() -> String {
    format!("test-{}", Uuid::new_v4())
}

pub fn list_active_triggers(storage: &Arc<Storage>) -> Result<Vec<ActiveTrigger>> {
    storage
        .triggers
        .list_active_triggers()
        .context("Failed to list active triggers")
}
