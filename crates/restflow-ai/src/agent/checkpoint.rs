//! Agent checkpoint snapshot utilities.
//!
//! This module provides a compact checkpoint payload for background durability
//! and restart recovery flows.

use crate::error::{AiError, Result};
use crate::llm::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Snapshot payload persisted at tool-call boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCheckpoint {
    /// JSON-encoded conversation messages up to the checkpoint boundary.
    pub messages: Vec<String>,
    /// Zero-based index of the tool call boundary.
    pub tool_call_index: usize,
    /// Referenced long-term memory chunk IDs.
    pub memory_refs: Vec<String>,
    /// Additional extensible metadata.
    pub metadata: HashMap<String, String>,
    /// Forward-compatible schema version.
    pub schema_version: u32,
}

impl AgentCheckpoint {
    /// Construct a checkpoint with the current schema version.
    pub fn new(messages: Vec<Message>, tool_call_index: usize) -> Result<Self> {
        let encoded_messages = messages
            .into_iter()
            .map(|message| {
                serde_json::to_string(&message)
                    .map_err(|e| AiError::Agent(format!("Failed to encode message: {e}")))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            messages: encoded_messages,
            tool_call_index,
            memory_refs: Vec::new(),
            metadata: HashMap::new(),
            schema_version: CURRENT_SCHEMA_VERSION,
        })
    }

    /// Decode checkpoint messages back to typed chat messages.
    pub fn decode_messages(&self) -> Result<Vec<Message>> {
        self.messages
            .iter()
            .map(|encoded| {
                serde_json::from_str::<Message>(encoded)
                    .map_err(|e| AiError::Agent(format!("Failed to decode message: {e}")))
            })
            .collect()
    }
}

/// Serialize checkpoint payload to compact postcard bytes.
pub fn checkpoint_save(checkpoint: &AgentCheckpoint) -> Result<Vec<u8>> {
    postcard::to_stdvec(checkpoint)
        .map_err(|e| AiError::Agent(format!("Failed to serialize checkpoint: {e}")))
}

/// Restore checkpoint payload from postcard bytes.
pub fn checkpoint_restore(bytes: &[u8]) -> Result<AgentCheckpoint> {
    let checkpoint: AgentCheckpoint = postcard::from_bytes(bytes)
        .map_err(|e| AiError::Agent(format!("Failed to deserialize checkpoint: {e}")))?;

    if checkpoint.schema_version == 0 || checkpoint.schema_version > CURRENT_SCHEMA_VERSION {
        return Err(AiError::Agent(format!(
            "Unsupported checkpoint schema version: {}",
            checkpoint.schema_version
        )));
    }

    Ok(checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_roundtrip_with_postcard() {
        let mut checkpoint =
            AgentCheckpoint::new(vec![Message::user("hello")], 3).expect("build checkpoint");
        checkpoint.memory_refs = vec!["mem-1".to_string(), "mem-2".to_string()];
        checkpoint
            .metadata
            .insert("mode".to_string(), "async".to_string());

        let bytes = checkpoint_save(&checkpoint).expect("serialize checkpoint");
        let restored = checkpoint_restore(&bytes).expect("restore checkpoint");

        assert_eq!(restored.tool_call_index, checkpoint.tool_call_index);
        assert_eq!(restored.memory_refs, checkpoint.memory_refs);
        assert_eq!(restored.metadata, checkpoint.metadata);
        assert_eq!(restored.schema_version, checkpoint.schema_version);
        let messages = restored.decode_messages().expect("decode messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "hello");
    }

    #[test]
    fn checkpoint_restore_rejects_unknown_schema() {
        let mut checkpoint = AgentCheckpoint::new(vec![], 0).expect("build checkpoint");
        checkpoint.schema_version = CURRENT_SCHEMA_VERSION + 1;

        let bytes = postcard::to_stdvec(&checkpoint).expect("serialize checkpoint");
        let err = checkpoint_restore(&bytes).expect_err("must reject future schema");
        assert!(format!("{err}").contains("Unsupported checkpoint schema version"));
    }
}
