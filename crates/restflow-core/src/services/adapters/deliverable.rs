//! DeliverableStore adapter backed by DeliverableStorage.

use crate::models::{Deliverable, DeliverableType};
use chrono::Utc;
use restflow_ai::tools::DeliverableStore;
use restflow_tools::ToolError;
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone)]
pub struct DeliverableStoreAdapter {
    storage: crate::storage::DeliverableStorage,
}

impl DeliverableStoreAdapter {
    pub fn new(storage: crate::storage::DeliverableStorage) -> Self {
        Self { storage }
    }

    fn parse_deliverable_type(value: &str) -> restflow_tools::Result<DeliverableType> {
        match value.trim().to_lowercase().as_str() {
            "report" => Ok(DeliverableType::Report),
            "data" => Ok(DeliverableType::Data),
            "file" => Ok(DeliverableType::File),
            "artifact" => Ok(DeliverableType::Artifact),
            other => Err(ToolError::Tool(format!(
                "Unknown deliverable type: {}. Supported: report, data, file, artifact",
                other
            ))),
        }
    }
}

impl DeliverableStore for DeliverableStoreAdapter {
    fn save_deliverable(
        &self,
        task_id: &str,
        execution_id: &str,
        deliverable_type: &str,
        title: &str,
        content: &str,
        file_path: Option<&str>,
        content_type: Option<&str>,
        metadata: Option<Value>,
    ) -> restflow_tools::Result<Value> {
        let deliverable_type = Self::parse_deliverable_type(deliverable_type)?;
        let now_ms = Utc::now().timestamp_millis();
        let metadata = metadata
            .map(serde_json::from_value::<std::collections::BTreeMap<String, String>>)
            .transpose()
            .map_err(|e| ToolError::Tool(format!("Invalid metadata object: {}", e)))?;
        let deliverable = Deliverable {
            id: Uuid::new_v4().to_string(),
            task_id: task_id.trim().to_string(),
            execution_id: execution_id.trim().to_string(),
            deliverable_type,
            title: title.trim().to_string(),
            content: content.to_string(),
            file_path: file_path
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            content_type: content_type
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            size_bytes: content.len(),
            created_at: now_ms,
            metadata,
        };
        self.storage
            .save(&deliverable)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(deliverable).map_err(ToolError::from)
    }
}
