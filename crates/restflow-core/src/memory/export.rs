//! Markdown exporter for memory content.
//!
//! This module provides functionality to export memory chunks and sessions
//! to human-readable Markdown format for archival and external use.
//!
//! # Features
//!
//! - Export individual sessions to Markdown
//! - Export all memories for an agent
//! - Metadata preserved as HTML comments
//! - Chronological ordering of chunks
//! - Configurable formatting options
//!
//! # Example
//!
//! ```ignore
//! use restflow_core::memory::MemoryExporter;
//!
//! let exporter = MemoryExporter::new(storage);
//!
//! // Export a single session
//! let markdown = exporter.export_session("session-001")?;
//!
//! // Export all memories for an agent
//! let markdown = exporter.export_agent("agent-001")?;
//!
//! // Export with custom options
//! let markdown = exporter
//!     .with_options(ExportOptions::default().include_metadata(false))
//!     .export_agent("agent-001")?;
//! ```

use crate::models::memory::{MemoryChunk, MemorySession, MemorySource};
use crate::storage::MemoryStorage;
use anyhow::Result;
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// Options for customizing the export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    /// Include metadata as HTML comments
    pub include_metadata: bool,
    /// Include creation timestamps
    pub include_timestamps: bool,
    /// Include source information
    pub include_source: bool,
    /// Include tags
    pub include_tags: bool,
    /// Include session headers
    pub include_session_headers: bool,
    /// Date format string (strftime format)
    pub date_format: String,
    /// Separator between chunks
    pub chunk_separator: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_metadata: true,
            include_timestamps: true,
            include_source: true,
            include_tags: true,
            include_session_headers: true,
            date_format: "%Y-%m-%d %H:%M:%S UTC".to_string(),
            chunk_separator: "\n---\n\n".to_string(),
        }
    }
}

impl ExportOptions {
    /// Create minimal export options (content only).
    pub fn minimal() -> Self {
        Self {
            include_metadata: false,
            include_timestamps: false,
            include_source: false,
            include_tags: false,
            include_session_headers: false,
            ..Default::default()
        }
    }

    /// Create compact export options (timestamps only).
    pub fn compact() -> Self {
        Self {
            include_metadata: false,
            include_timestamps: true,
            include_source: false,
            include_tags: false,
            include_session_headers: true,
            ..Default::default()
        }
    }

    /// Set whether to include metadata.
    pub fn include_metadata(mut self, include: bool) -> Self {
        self.include_metadata = include;
        self
    }

    /// Set whether to include timestamps.
    pub fn include_timestamps(mut self, include: bool) -> Self {
        self.include_timestamps = include;
        self
    }

    /// Set whether to include source information.
    pub fn include_source(mut self, include: bool) -> Self {
        self.include_source = include;
        self
    }

    /// Set whether to include tags.
    pub fn include_tags(mut self, include: bool) -> Self {
        self.include_tags = include;
        self
    }

    /// Set whether to include session headers.
    pub fn include_session_headers(mut self, include: bool) -> Self {
        self.include_session_headers = include;
        self
    }

    /// Set the date format.
    pub fn date_format(mut self, format: &str) -> Self {
        self.date_format = format.to_string();
        self
    }

    /// Set the chunk separator.
    pub fn chunk_separator(mut self, separator: &str) -> Self {
        self.chunk_separator = separator.to_string();
        self
    }
}

/// Result of an export operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    /// The exported Markdown content
    pub markdown: String,
    /// Number of chunks exported
    pub chunk_count: u32,
    /// Number of sessions included
    pub session_count: u32,
    /// Agent ID
    pub agent_id: String,
    /// Suggested filename
    pub suggested_filename: String,
}

/// Memory exporter for generating Markdown output.
#[derive(Clone)]
pub struct MemoryExporter {
    storage: MemoryStorage,
    options: ExportOptions,
}

impl MemoryExporter {
    /// Create a new exporter with default options.
    pub fn new(storage: MemoryStorage) -> Self {
        Self {
            storage,
            options: ExportOptions::default(),
        }
    }

    /// Create a new exporter with custom options.
    pub fn with_options(storage: MemoryStorage, options: ExportOptions) -> Self {
        Self { storage, options }
    }

    /// Update the export options.
    pub fn set_options(&mut self, options: ExportOptions) {
        self.options = options;
    }

    /// Get the current export options.
    pub fn options(&self) -> &ExportOptions {
        &self.options
    }

    /// Export a single session to Markdown.
    pub fn export_session(&self, session_id: &str) -> Result<ExportResult> {
        let session = self
            .storage
            .get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        let chunks = self.storage.list_chunks_for_session(session_id)?;

        let mut markdown = String::new();

        // Add header
        markdown.push_str(&self.format_document_header(&session.agent_id, Some(&session)));

        // Add chunks
        for (i, chunk) in chunks.iter().enumerate() {
            if i > 0 {
                markdown.push_str(&self.options.chunk_separator);
            }
            markdown.push_str(&self.format_chunk(chunk));
        }

        // Add footer
        markdown.push_str(&self.format_document_footer(&session.agent_id, chunks.len() as u32));

        let suggested_filename = format!(
            "memory-{}-{}.md",
            session.agent_id,
            session_id.chars().take(8).collect::<String>()
        );

        Ok(ExportResult {
            markdown,
            chunk_count: chunks.len() as u32,
            session_count: 1,
            agent_id: session.agent_id,
            suggested_filename,
        })
    }

    /// Export all memories for an agent to Markdown.
    pub fn export_agent(&self, agent_id: &str) -> Result<ExportResult> {
        let sessions = self.storage.list_sessions(agent_id)?;
        let all_chunks = self.storage.list_chunks(agent_id)?;

        let mut markdown = String::new();

        // Add header
        markdown.push_str(&self.format_document_header(agent_id, None));

        if self.options.include_session_headers && !sessions.is_empty() {
            // Group chunks by session
            for session in &sessions {
                let session_chunks: Vec<_> = all_chunks
                    .iter()
                    .filter(|c| c.session_id.as_ref() == Some(&session.id))
                    .collect();

                if !session_chunks.is_empty() {
                    markdown.push_str(&self.format_session_header(session));

                    for (i, chunk) in session_chunks.iter().enumerate() {
                        if i > 0 {
                            markdown.push_str(&self.options.chunk_separator);
                        }
                        markdown.push_str(&self.format_chunk(chunk));
                    }

                    markdown.push_str("\n\n");
                }
            }

            // Add orphan chunks (no session)
            let orphan_chunks: Vec<_> = all_chunks
                .iter()
                .filter(|c| c.session_id.is_none())
                .collect();

            if !orphan_chunks.is_empty() {
                markdown.push_str("## Unsessioned Memories\n\n");

                for (i, chunk) in orphan_chunks.iter().enumerate() {
                    if i > 0 {
                        markdown.push_str(&self.options.chunk_separator);
                    }
                    markdown.push_str(&self.format_chunk(chunk));
                }

                markdown.push_str("\n\n");
            }
        } else {
            // No session grouping, just list all chunks chronologically
            // Sort by created_at ascending
            let mut sorted_chunks = all_chunks.clone();
            sorted_chunks.sort_by_key(|c| c.created_at);

            for (i, chunk) in sorted_chunks.iter().enumerate() {
                if i > 0 {
                    markdown.push_str(&self.options.chunk_separator);
                }
                markdown.push_str(&self.format_chunk(chunk));
            }
        }

        // Add footer
        markdown.push_str(&self.format_document_footer(agent_id, all_chunks.len() as u32));

        let suggested_filename = format!("memory-{}-all.md", agent_id);

        Ok(ExportResult {
            markdown,
            chunk_count: all_chunks.len() as u32,
            session_count: sessions.len() as u32,
            agent_id: agent_id.to_string(),
            suggested_filename,
        })
    }

    /// Format the document header.
    fn format_document_header(&self, agent_id: &str, session: Option<&MemorySession>) -> String {
        let mut header = String::new();

        let title = if let Some(s) = session {
            format!("# Memory Export: {}\n\n", s.name)
        } else {
            format!("# Memory Export: Agent {}\n\n", agent_id)
        };

        header.push_str(&title);

        if self.options.include_metadata {
            header.push_str("<!-- MEMORY EXPORT METADATA\n");
            header.push_str(&format!("agent_id: {}\n", agent_id));
            if let Some(s) = session {
                header.push_str(&format!("session_id: {}\n", s.id));
                header.push_str(&format!("session_name: {}\n", s.name));
                if let Some(ref desc) = s.description {
                    header.push_str(&format!("session_description: {}\n", desc));
                }
            }
            let export_time = Utc::now().format(&self.options.date_format);
            header.push_str(&format!("exported_at: {}\n", export_time));
            header.push_str("-->\n\n");
        }

        header
    }

    /// Format a session header.
    fn format_session_header(&self, session: &MemorySession) -> String {
        let mut header = format!("## {}\n\n", session.name);

        if let Some(ref desc) = session.description {
            header.push_str(&format!("_{}_\n\n", desc));
        }

        if self.options.include_metadata {
            header.push_str(&format!(
                "<!-- session_id: {} | chunks: {} | tokens: {} -->\n\n",
                session.id, session.chunk_count, session.total_tokens
            ));
        }

        header
    }

    /// Format a single memory chunk.
    fn format_chunk(&self, chunk: &MemoryChunk) -> String {
        let mut output = String::new();

        // Add timestamp if enabled
        if self.options.include_timestamps {
            let timestamp = Utc
                .timestamp_millis_opt(chunk.created_at)
                .single()
                .map(|dt| dt.format(&self.options.date_format).to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            output.push_str(&format!("**{}**\n\n", timestamp));
        }

        // Add content
        output.push_str(&chunk.content);
        output.push('\n');

        // Add tags if enabled
        if self.options.include_tags && !chunk.tags.is_empty() {
            let tags = chunk
                .tags
                .iter()
                .map(|t| format!("`{}`", t))
                .collect::<Vec<_>>()
                .join(" ");
            output.push_str(&format!("\n🏷️ {}\n", tags));
        }

        // Add source if enabled
        if self.options.include_source {
            let source_str = match &chunk.source {
                MemorySource::TaskExecution { task_id } => {
                    format!("📋 Task: {}", task_id)
                }
                MemorySource::Conversation { session_id } => {
                    format!("💬 Conversation: {}", session_id)
                }
                MemorySource::ManualNote => "📝 Manual Note".to_string(),
                MemorySource::AgentGenerated { tool_name } => {
                    format!("🤖 Generated by: {}", tool_name)
                }
            };
            output.push_str(&format!("\n{}\n", source_str));
        }

        // Add metadata comment if enabled
        if self.options.include_metadata {
            output.push_str(&format!(
                "\n<!-- chunk_id: {} | tokens: {} | hash: {} -->\n",
                chunk.id,
                chunk.token_count.unwrap_or(0),
                &chunk.content_hash[..16]
            ));
        }

        output
    }

    /// Format the document footer.
    fn format_document_footer(&self, agent_id: &str, chunk_count: u32) -> String {
        let mut footer = String::new();

        footer.push_str("\n---\n\n");
        footer.push_str(&format!(
            "_Exported {} memory chunks for agent `{}`_\n",
            chunk_count, agent_id
        ));

        if self.options.include_metadata {
            footer.push_str(&format!(
                "\n<!-- END OF MEMORY EXPORT | total_chunks: {} -->\n",
                chunk_count
            ));
        }

        footer
    }

    /// Get the underlying storage.
    pub fn storage(&self) -> &MemoryStorage {
        &self.storage
    }
}

/// Builder for MemoryExporter with fluent configuration.
pub struct MemoryExporterBuilder {
    storage: MemoryStorage,
    options: ExportOptions,
}

impl MemoryExporterBuilder {
    /// Create a new builder with the given storage.
    pub fn new(storage: MemoryStorage) -> Self {
        Self {
            storage,
            options: ExportOptions::default(),
        }
    }

    /// Use minimal export options.
    pub fn minimal(mut self) -> Self {
        self.options = ExportOptions::minimal();
        self
    }

    /// Use compact export options.
    pub fn compact(mut self) -> Self {
        self.options = ExportOptions::compact();
        self
    }

    /// Include metadata in export.
    pub fn include_metadata(mut self, include: bool) -> Self {
        self.options.include_metadata = include;
        self
    }

    /// Include timestamps in export.
    pub fn include_timestamps(mut self, include: bool) -> Self {
        self.options.include_timestamps = include;
        self
    }

    /// Include source information in export.
    pub fn include_source(mut self, include: bool) -> Self {
        self.options.include_source = include;
        self
    }

    /// Include tags in export.
    pub fn include_tags(mut self, include: bool) -> Self {
        self.options.include_tags = include;
        self
    }

    /// Include session headers in export.
    pub fn include_session_headers(mut self, include: bool) -> Self {
        self.options.include_session_headers = include;
        self
    }

    /// Set the date format.
    pub fn date_format(mut self, format: &str) -> Self {
        self.options.date_format = format.to_string();
        self
    }

    /// Set the chunk separator.
    pub fn chunk_separator(mut self, separator: &str) -> Self {
        self.options.chunk_separator = separator.to_string();
        self
    }

    /// Build the exporter.
    pub fn build(self) -> MemoryExporter {
        MemoryExporter::with_options(self.storage, self.options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::tempdir;

    fn create_test_exporter() -> MemoryExporter {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = MemoryStorage::new(db).unwrap();
        MemoryExporter::new(storage)
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    #[test]
    fn test_export_options_default() {
        let options = ExportOptions::default();
        assert!(options.include_metadata);
        assert!(options.include_timestamps);
        assert!(options.include_source);
        assert!(options.include_tags);
        assert!(options.include_session_headers);
    }

    #[test]
    fn test_export_options_minimal() {
        let options = ExportOptions::minimal();
        assert!(!options.include_metadata);
        assert!(!options.include_timestamps);
        assert!(!options.include_source);
        assert!(!options.include_tags);
        assert!(!options.include_session_headers);
    }

    #[test]
    fn test_export_options_compact() {
        let options = ExportOptions::compact();
        assert!(!options.include_metadata);
        assert!(options.include_timestamps);
        assert!(!options.include_source);
        assert!(!options.include_tags);
        assert!(options.include_session_headers);
    }

    #[test]
    fn test_export_options_builder_pattern() {
        let options = ExportOptions::default()
            .include_metadata(false)
            .include_timestamps(true)
            .date_format("%Y-%m-%d")
            .chunk_separator("\n\n");

        assert!(!options.include_metadata);
        assert!(options.include_timestamps);
        assert_eq!(options.date_format, "%Y-%m-%d");
        assert_eq!(options.chunk_separator, "\n\n");
    }

    #[test]
    fn test_exporter_builder() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = MemoryStorage::new(db).unwrap();

        let exporter = MemoryExporterBuilder::new(storage)
            .minimal()
            .include_timestamps(true)
            .build();

        assert!(exporter.options().include_timestamps);
        assert!(!exporter.options().include_metadata);
    }

    #[test]
    fn test_export_session_not_found() {
        let exporter = create_test_exporter();

        let result = exporter.export_session("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_export_session_basic() {
        let exporter = create_test_exporter();

        // Create session
        let session = MemorySession::new("agent-001".to_string(), "Test Session".to_string())
            .with_description("A test session".to_string());
        exporter.storage().create_session(&session).unwrap();

        // Create chunks
        let chunk1 = MemoryChunk::new("agent-001".to_string(), "First memory".to_string())
            .with_session(session.id.clone())
            .with_created_at(now_ms() - 1000);
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Second memory".to_string())
            .with_session(session.id.clone())
            .with_created_at(now_ms());

        exporter.storage().store_chunk(&chunk1).unwrap();
        exporter.storage().store_chunk(&chunk2).unwrap();

        // Export
        let result = exporter.export_session(&session.id).unwrap();

        assert_eq!(result.chunk_count, 2);
        assert_eq!(result.session_count, 1);
        assert_eq!(result.agent_id, "agent-001");
        assert!(result.markdown.contains("Test Session"));
        assert!(result.markdown.contains("First memory"));
        assert!(result.markdown.contains("Second memory"));
        assert!(result.suggested_filename.starts_with("memory-agent-001-"));
    }

    #[test]
    fn test_export_agent_basic() {
        let exporter = create_test_exporter();

        // Create chunks without session
        let chunk1 =
            MemoryChunk::new("agent-001".to_string(), "Memory one".to_string())
                .with_created_at(now_ms() - 2000);
        let chunk2 =
            MemoryChunk::new("agent-001".to_string(), "Memory two".to_string())
                .with_created_at(now_ms() - 1000);
        let chunk3 =
            MemoryChunk::new("agent-001".to_string(), "Memory three".to_string())
                .with_created_at(now_ms());

        exporter.storage().store_chunk(&chunk1).unwrap();
        exporter.storage().store_chunk(&chunk2).unwrap();
        exporter.storage().store_chunk(&chunk3).unwrap();

        // Export
        let result = exporter.export_agent("agent-001").unwrap();

        assert_eq!(result.chunk_count, 3);
        assert_eq!(result.agent_id, "agent-001");
        assert!(result.markdown.contains("Memory one"));
        assert!(result.markdown.contains("Memory two"));
        assert!(result.markdown.contains("Memory three"));
        assert_eq!(result.suggested_filename, "memory-agent-001-all.md");
    }

    #[test]
    fn test_export_agent_with_sessions() {
        let mut exporter = create_test_exporter();
        exporter.set_options(ExportOptions::default().include_session_headers(true));

        // Create sessions
        let session1 = MemorySession::new("agent-001".to_string(), "Session One".to_string());
        let session2 = MemorySession::new("agent-001".to_string(), "Session Two".to_string());
        exporter.storage().create_session(&session1).unwrap();
        exporter.storage().create_session(&session2).unwrap();

        // Create chunks for each session
        let chunk1 = MemoryChunk::new("agent-001".to_string(), "In session one".to_string())
            .with_session(session1.id.clone());
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "In session two".to_string())
            .with_session(session2.id.clone());

        exporter.storage().store_chunk(&chunk1).unwrap();
        exporter.storage().store_chunk(&chunk2).unwrap();

        // Export
        let result = exporter.export_agent("agent-001").unwrap();

        assert_eq!(result.chunk_count, 2);
        assert_eq!(result.session_count, 2);
        assert!(result.markdown.contains("## Session One"));
        assert!(result.markdown.contains("## Session Two"));
        assert!(result.markdown.contains("In session one"));
        assert!(result.markdown.contains("In session two"));
    }

    #[test]
    fn test_export_includes_tags() {
        let exporter = create_test_exporter();

        let session = MemorySession::new("agent-001".to_string(), "Tagged Session".to_string());
        exporter.storage().create_session(&session).unwrap();

        let chunk = MemoryChunk::new("agent-001".to_string(), "Tagged content".to_string())
            .with_session(session.id.clone())
            .with_tags(vec!["rust".to_string(), "async".to_string()]);

        exporter.storage().store_chunk(&chunk).unwrap();

        let result = exporter.export_session(&session.id).unwrap();

        assert!(result.markdown.contains("`rust`"));
        assert!(result.markdown.contains("`async`"));
        assert!(result.markdown.contains("🏷️"));
    }

    #[test]
    fn test_export_includes_source() {
        let exporter = create_test_exporter();

        let session = MemorySession::new("agent-001".to_string(), "Source Session".to_string());
        exporter.storage().create_session(&session).unwrap();

        let chunk = MemoryChunk::new("agent-001".to_string(), "Task output".to_string())
            .with_session(session.id.clone())
            .with_source(MemorySource::TaskExecution {
                task_id: "task-123".to_string(),
            });

        exporter.storage().store_chunk(&chunk).unwrap();

        let result = exporter.export_session(&session.id).unwrap();

        assert!(result.markdown.contains("📋 Task: task-123"));
    }

    #[test]
    fn test_export_includes_metadata_comments() {
        let exporter = create_test_exporter();

        let session = MemorySession::new("agent-001".to_string(), "Metadata Session".to_string());
        exporter.storage().create_session(&session).unwrap();

        let chunk = MemoryChunk::new("agent-001".to_string(), "With metadata".to_string())
            .with_session(session.id.clone())
            .with_token_count(50);

        exporter.storage().store_chunk(&chunk).unwrap();

        let result = exporter.export_session(&session.id).unwrap();

        assert!(result.markdown.contains("<!-- MEMORY EXPORT METADATA"));
        assert!(result.markdown.contains("agent_id: agent-001"));
        assert!(result.markdown.contains("<!-- chunk_id:"));
        assert!(result.markdown.contains("tokens: 50"));
    }

    #[test]
    fn test_export_minimal_no_metadata() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = MemoryStorage::new(db).unwrap();

        let exporter = MemoryExporterBuilder::new(storage).minimal().build();

        let session = MemorySession::new("agent-001".to_string(), "Minimal Session".to_string());
        exporter.storage().create_session(&session).unwrap();

        let chunk = MemoryChunk::new("agent-001".to_string(), "Just content".to_string())
            .with_session(session.id.clone())
            .with_tags(vec!["tag".to_string()]);

        exporter.storage().store_chunk(&chunk).unwrap();

        let result = exporter.export_session(&session.id).unwrap();

        assert!(result.markdown.contains("Just content"));
        assert!(!result.markdown.contains("<!-- "));
        assert!(!result.markdown.contains("`tag`"));
    }

    #[test]
    fn test_export_chronological_order() {
        let mut exporter = create_test_exporter();
        exporter.set_options(ExportOptions::minimal());

        let now = now_ms();

        // Create chunks in reverse order
        let chunk3 = MemoryChunk::new("agent-001".to_string(), "Third".to_string())
            .with_created_at(now);
        let chunk1 = MemoryChunk::new("agent-001".to_string(), "First".to_string())
            .with_created_at(now - 2000);
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Second".to_string())
            .with_created_at(now - 1000);

        exporter.storage().store_chunk(&chunk3).unwrap();
        exporter.storage().store_chunk(&chunk1).unwrap();
        exporter.storage().store_chunk(&chunk2).unwrap();

        let result = exporter.export_agent("agent-001").unwrap();

        // Find positions in the markdown
        let first_pos = result.markdown.find("First").unwrap();
        let second_pos = result.markdown.find("Second").unwrap();
        let third_pos = result.markdown.find("Third").unwrap();

        assert!(first_pos < second_pos);
        assert!(second_pos < third_pos);
    }

    #[test]
    fn test_export_empty_agent() {
        let exporter = create_test_exporter();

        let result = exporter.export_agent("agent-empty").unwrap();

        assert_eq!(result.chunk_count, 0);
        assert_eq!(result.session_count, 0);
        assert!(result.markdown.contains("Exported 0 memory chunks"));
    }

    #[test]
    fn test_format_source_types() {
        let exporter = create_test_exporter();

        // Create a session for testing
        let session = MemorySession::new("agent-001".to_string(), "Sources".to_string());
        exporter.storage().create_session(&session).unwrap();

        // Test different source types
        let sources = [
            MemorySource::TaskExecution {
                task_id: "task-1".to_string(),
            },
            MemorySource::Conversation {
                session_id: "conv-1".to_string(),
            },
            MemorySource::ManualNote,
            MemorySource::AgentGenerated {
                tool_name: "save_memory".to_string(),
            },
        ];

        for (i, source) in sources.iter().enumerate() {
            let chunk = MemoryChunk::new("agent-001".to_string(), format!("Content {}", i))
                .with_session(session.id.clone())
                .with_source(source.clone());
            exporter.storage().store_chunk(&chunk).unwrap();
        }

        let result = exporter.export_session(&session.id).unwrap();

        assert!(result.markdown.contains("📋 Task: task-1"));
        assert!(result.markdown.contains("💬 Conversation: conv-1"));
        assert!(result.markdown.contains("📝 Manual Note"));
        assert!(result.markdown.contains("🤖 Generated by: save_memory"));
    }

    #[test]
    fn test_export_result_fields() {
        let exporter = create_test_exporter();

        let session = MemorySession::new("agent-001".to_string(), "Test".to_string());
        exporter.storage().create_session(&session).unwrap();

        let chunk = MemoryChunk::new("agent-001".to_string(), "Content".to_string())
            .with_session(session.id.clone());
        exporter.storage().store_chunk(&chunk).unwrap();

        let result = exporter.export_session(&session.id).unwrap();

        assert_eq!(result.chunk_count, 1);
        assert_eq!(result.session_count, 1);
        assert_eq!(result.agent_id, "agent-001");
        assert!(!result.markdown.is_empty());
        assert!(result.suggested_filename.ends_with(".md"));
    }
}
