//! DeliverableStore adapter backed by DeliverableStorage.

use crate::models::{Deliverable, DeliverableType};
use chrono::Utc;
use restflow_tools::ToolError;
use restflow_traits::store::DeliverableStore;
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
        self.storage.save(&deliverable)?;
        serde_json::to_value(deliverable).map_err(ToolError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::store::DeliverableStore;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (DeliverableStoreAdapter, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let storage = crate::storage::DeliverableStorage::new(db).unwrap();
        (DeliverableStoreAdapter::new(storage), temp_dir)
    }

    #[test]
    fn test_save_deliverable_report() {
        let (adapter, _dir) = setup();
        let result = adapter
            .save_deliverable(
                "task-1",
                "exec-1",
                "report",
                "Test Report",
                "content",
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(result["title"], "Test Report");
        assert_eq!(result["task_id"], "task-1");
        assert_eq!(result["execution_id"], "exec-1");
        assert!(result["id"].as_str().is_some());
    }

    #[test]
    fn test_save_deliverable_with_all_fields() {
        let (adapter, _dir) = setup();
        let metadata = serde_json::json!({"key": "value"});
        let result = adapter
            .save_deliverable(
                "task-2",
                "exec-2",
                "data",
                "Data Export",
                "csv data here",
                Some("/tmp/data.csv"),
                Some("text/csv"),
                Some(metadata),
            )
            .unwrap();
        assert_eq!(result["file_path"], "/tmp/data.csv");
        assert_eq!(result["content_type"], "text/csv");
    }

    #[test]
    fn test_save_deliverable_all_types() {
        let (adapter, _dir) = setup();
        for dtype in &["report", "data", "file", "artifact"] {
            let result = adapter
                .save_deliverable("t", "e", dtype, "title", "body", None, None, None)
                .unwrap();
            assert!(result["id"].as_str().is_some());
        }
    }

    #[test]
    fn test_save_deliverable_unknown_type_fails() {
        let (adapter, _dir) = setup();
        let result = adapter.save_deliverable("t", "e", "unknown", "t", "c", None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_save_deliverable_invalid_metadata_fails() {
        let (adapter, _dir) = setup();
        let bad_metadata = serde_json::json!([1, 2, 3]);
        let result =
            adapter.save_deliverable("t", "e", "report", "t", "c", None, None, Some(bad_metadata));
        assert!(result.is_err());
    }
}
