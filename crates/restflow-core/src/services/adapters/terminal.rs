//! TerminalStore adapter backed by TerminalSessionStorage.

use crate::models::TerminalSession;
use crate::storage::TerminalSessionStorage;
use chrono::Utc;
use restflow_ai::tools::TerminalStore;
use restflow_tools::ToolError;
use serde_json::{Value, json};
use uuid::Uuid;

pub struct TerminalStoreAdapter {
    storage: TerminalSessionStorage,
}

impl TerminalStoreAdapter {
    pub fn new(storage: TerminalSessionStorage) -> Self {
        Self { storage }
    }
}

impl TerminalStore for TerminalStoreAdapter {
    fn create_session(
        &self,
        name: Option<&str>,
        working_dir: Option<&str>,
        startup_cmd: Option<&str>,
    ) -> restflow_tools::Result<Value> {
        let id = format!("terminal-{}", Uuid::new_v4());
        let default_name = self
            .storage
            .get_next_name()
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        let mut session =
            TerminalSession::new(id, name.unwrap_or(&default_name).to_string());
        session.set_config(
            working_dir.map(|s| s.to_string()),
            startup_cmd.map(|s| s.to_string()),
        );
        self.storage
            .create(&session)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(serde_json::to_value(session)?)
    }

    fn list_sessions(&self) -> restflow_tools::Result<Value> {
        let sessions = self
            .storage
            .list()
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(serde_json::to_value(sessions)?)
    }

    fn send_input(&self, session_id: &str, data: &str) -> restflow_tools::Result<Value> {
        let mut session = self
            .storage
            .get(session_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?
            .ok_or_else(|| {
                ToolError::Tool(format!("Terminal session not found: {}", session_id))
            })?;

        let mut history = session.history.clone().unwrap_or_default();
        history.push_str(&format!("\n$ {}", data));
        session.update_history(history);
        self.storage
            .update(session_id, &session)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        Ok(json!({
            "session_id": session_id,
            "accepted": true,
            "live_runtime": false
        }))
    }

    fn read_output(&self, session_id: &str) -> restflow_tools::Result<Value> {
        let session = self
            .storage
            .get(session_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?
            .ok_or_else(|| {
                ToolError::Tool(format!("Terminal session not found: {}", session_id))
            })?;
        Ok(json!({
            "session_id": session_id,
            "output": session.history.unwrap_or_default(),
            "live_runtime": false
        }))
    }

    fn close_session(&self, session_id: &str) -> restflow_tools::Result<Value> {
        let mut session = self
            .storage
            .get(session_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?
            .ok_or_else(|| {
                ToolError::Tool(format!("Terminal session not found: {}", session_id))
            })?;
        session.status = crate::models::TerminalStatus::Stopped;
        session.stopped_at = Some(Utc::now().timestamp_millis());
        self.storage
            .update(session_id, &session)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(json!({
            "session_id": session_id,
            "closed": true
        }))
    }
}
