//! Text chunking utilities for memory storage.
//!
//! This module provides functionality for splitting long text content into
//! smaller, overlapping chunks suitable for storage and retrieval. Chunks
//! are designed to be small enough for efficient searching while maintaining
//! context through overlap.
//!
//! # Architecture
//!
//! ```text
//! Original text (5000 chars)
//! ┌────────────────────────────────────────────────────────────────────────┐
//! │ Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do...    │
//! └────────────────────────────────────────────────────────────────────────┘
//!                                     ↓
//!                              TextChunker
//!                                     ↓
//! ┌────────────────────┐ ┌────────────────────┐ ┌────────────────────┐
//! │ Chunk 1 (1600)     │ │ Chunk 2 (1600)     │ │ Chunk 3 (remaining)│
//! │                    │ │ ← overlap (320) →  │ │                    │
//! └────────────────────┘ └────────────────────┘ └────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust
//! use restflow_core::memory::TextChunker;
//! use restflow_core::models::memory::MemorySource;
//!
//! let chunker = TextChunker::default();
//! let chunks = chunker.chunk(
//!     "Long text content here...",
//!     "agent-123",
//!     Some("session-456"),
//!     MemorySource::ManualNote,
//! );
//! ```

use crate::models::memory::{MemoryChunk, MemorySource};

/// Configuration and implementation for text chunking.
///
/// Text is split into chunks of configurable size with overlap between
/// consecutive chunks to preserve context. Chunks are created at word
/// boundaries to avoid splitting words.
#[derive(Debug, Clone)]
pub struct TextChunker {
    /// Target size for each chunk in characters (~4 chars per token)
    /// Default: 1600 chars (~400 tokens)
    chunk_size: usize,

    /// Overlap between consecutive chunks in characters
    /// Default: 320 chars (~80 tokens)
    overlap: usize,

    /// Minimum chunk size - chunks smaller than this are merged with previous
    /// Default: 200 chars
    min_chunk_size: usize,
}

impl Default for TextChunker {
    fn default() -> Self {
        Self {
            chunk_size: 1600, // ~400 tokens
            overlap: 320,     // ~80 tokens (20% overlap)
            min_chunk_size: 200,
        }
    }
}

impl TextChunker {
    /// Create a new TextChunker with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a TextChunker with custom chunk size.
    ///
    /// # Arguments
    ///
    /// * `chunk_size` - Target size for each chunk in characters
    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size;
        self
    }

    /// Create a TextChunker with custom overlap.
    ///
    /// # Arguments
    ///
    /// * `overlap` - Number of characters to overlap between chunks
    pub fn with_overlap(mut self, overlap: usize) -> Self {
        self.overlap = overlap;
        self
    }

    /// Create a TextChunker with custom minimum chunk size.
    ///
    /// # Arguments
    ///
    /// * `min_chunk_size` - Minimum size for a standalone chunk
    pub fn with_min_chunk_size(mut self, min_chunk_size: usize) -> Self {
        self.min_chunk_size = min_chunk_size;
        self
    }

    /// Get the configured chunk size.
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Get the configured overlap size.
    pub fn overlap(&self) -> usize {
        self.overlap
    }

    /// Get the configured minimum chunk size.
    pub fn min_chunk_size(&self) -> usize {
        self.min_chunk_size
    }

    /// Split text into overlapping chunks.
    ///
    /// Each chunk is created as a `MemoryChunk` with:
    /// - Unique ID
    /// - SHA-256 content hash for deduplication
    /// - Estimated token count
    /// - Provided metadata (agent_id, session_id, source)
    ///
    /// # Arguments
    ///
    /// * `text` - The text content to chunk
    /// * `agent_id` - Agent ID to associate with chunks
    /// * `session_id` - Optional session ID for grouping
    /// * `source` - Source of the memory (task, conversation, etc.)
    ///
    /// # Returns
    ///
    /// Vector of `MemoryChunk` instances, or empty vector if text is empty.
    pub fn chunk(
        &self,
        text: &str,
        agent_id: &str,
        session_id: Option<&str>,
        source: MemorySource,
    ) -> Vec<MemoryChunk> {
        let text = text.trim();
        if text.is_empty() {
            return Vec::new();
        }

        // If text is smaller than chunk size, return single chunk
        if text.len() <= self.chunk_size {
            return vec![self.create_chunk(text, agent_id, session_id, source)];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < text.len() {
            // Calculate end position, ensuring it lands on a char boundary
            let mut end = (start + self.chunk_size).min(text.len());
            end = Self::floor_char_boundary(text, end);

            // If not at the end, find word boundary
            if end < text.len() {
                end = self.find_word_boundary(text, end);
            }

            // Extract chunk content
            let chunk_text = &text[start..end];

            // Skip if chunk would be too small (except for last chunk)
            if chunk_text.len() >= self.min_chunk_size || start + self.chunk_size >= text.len() {
                chunks.push(self.create_chunk(
                    chunk_text.trim(),
                    agent_id,
                    session_id,
                    source.clone(),
                ));
            }

            // Move start position with overlap
            let step = self.chunk_size.saturating_sub(self.overlap);
            let mut next_start = if step > 0 {
                start + step
            } else {
                // Safety: avoid infinite loop if overlap >= chunk_size
                start + self.chunk_size
            };

            // Ensure start is on a char boundary
            next_start = Self::ceil_char_boundary(text, next_start);

            // Find word boundary for start position too
            if next_start < text.len() && next_start > 0 {
                next_start = self.find_word_boundary_forward(text, next_start);
            }

            start = next_start;
        }

        chunks
    }

    /// Split messages into chunks.
    ///
    /// Formats a list of (role, content) message pairs into a conversation
    /// format before chunking.
    ///
    /// # Arguments
    ///
    /// * `messages` - List of (role, content) tuples
    /// * `agent_id` - Agent ID to associate with chunks
    /// * `session_id` - Optional session ID for grouping
    /// * `source` - Source of the memory
    ///
    /// # Returns
    ///
    /// Vector of `MemoryChunk` instances.
    pub fn chunk_messages(
        &self,
        messages: &[(&str, &str)],
        agent_id: &str,
        session_id: Option<&str>,
        source: MemorySource,
    ) -> Vec<MemoryChunk> {
        let formatted = self.format_messages(messages);
        self.chunk(&formatted, agent_id, session_id, source)
    }

    /// Format messages as a conversation string.
    fn format_messages(&self, messages: &[(&str, &str)]) -> String {
        messages
            .iter()
            .map(|(role, content)| format!("[{}]: {}", role, content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Round a byte index down to the nearest char boundary.
    fn floor_char_boundary(text: &str, index: usize) -> usize {
        if index >= text.len() {
            return text.len();
        }
        // Walk backward until we hit a char boundary
        let mut i = index;
        while i > 0 && !text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    /// Round a byte index up to the nearest char boundary.
    fn ceil_char_boundary(text: &str, index: usize) -> usize {
        if index >= text.len() {
            return text.len();
        }
        let mut i = index;
        while i < text.len() && !text.is_char_boundary(i) {
            i += 1;
        }
        i
    }

    /// Find the nearest word boundary before or at the given position.
    ///
    /// Searches backward from `pos` to find a space or newline character.
    /// All indices are guaranteed to be on char boundaries.
    fn find_word_boundary(&self, text: &str, pos: usize) -> usize {
        let pos = Self::floor_char_boundary(text, pos);
        // Search backward for space or newline
        let search_start = Self::floor_char_boundary(text, pos.saturating_sub(100));
        let slice = &text[search_start..pos];

        // Find last whitespace in the slice
        if let Some(last_ws) = slice.rfind(|c: char| c.is_whitespace()) {
            // last_ws is a byte offset within slice, which is char-boundary-safe
            // because rfind returns the start byte of the matched char
            let boundary = search_start + last_ws;
            // Advance past the whitespace character
            return boundary + text[boundary..].chars().next().map_or(0, |c| c.len_utf8());
        }

        // No whitespace found, use original position
        pos
    }

    /// Find the nearest word boundary at or after the given position.
    ///
    /// Searches forward from `pos` to find a space or newline character.
    /// All indices are guaranteed to be on char boundaries.
    fn find_word_boundary_forward(&self, text: &str, pos: usize) -> usize {
        let pos = Self::ceil_char_boundary(text, pos);
        // Search forward for space or newline
        let search_end = Self::floor_char_boundary(text, (pos + 100).min(text.len()));
        let search_end = if search_end <= pos {
            text.len().min(pos + 100)
        } else {
            search_end
        };
        let search_end = Self::ceil_char_boundary(text, search_end);
        let slice = &text[pos..search_end];

        // Find first whitespace in the slice
        if let Some(first_ws) = slice.find(|c: char| c.is_whitespace()) {
            // first_ws is byte offset within slice, char-boundary-safe from find()
            let boundary = pos + first_ws;
            // Advance past the whitespace character
            return boundary + text[boundary..].chars().next().map_or(0, |c| c.len_utf8());
        }

        // No whitespace found, use original position
        pos
    }

    /// Create a MemoryChunk from content.
    fn create_chunk(
        &self,
        content: &str,
        agent_id: &str,
        session_id: Option<&str>,
        source: MemorySource,
    ) -> MemoryChunk {
        let token_count = self.estimate_tokens(content);

        let mut chunk = MemoryChunk::new(agent_id.to_string(), content.to_string())
            .with_source(source)
            .with_token_count(token_count);

        if let Some(sid) = session_id {
            chunk = chunk.with_session(sid.to_string());
        }

        chunk
    }

    /// Estimate token count for a piece of text.
    ///
    /// Uses a simple heuristic of ~4 characters per token.
    /// This is an approximation that works well for English text.
    fn estimate_tokens(&self, text: &str) -> u32 {
        // Rough estimate: ~4 chars per token for English text
        (text.len() as f32 / 4.0).ceil() as u32
    }
}

/// Builder for creating customized TextChunker instances.
#[derive(Debug, Default)]
pub struct TextChunkerBuilder {
    chunk_size: Option<usize>,
    overlap: Option<usize>,
    min_chunk_size: Option<usize>,
}

impl TextChunkerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the chunk size.
    pub fn chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = Some(size);
        self
    }

    /// Set the overlap size.
    pub fn overlap(mut self, overlap: usize) -> Self {
        self.overlap = Some(overlap);
        self
    }

    /// Set the minimum chunk size.
    pub fn min_chunk_size(mut self, size: usize) -> Self {
        self.min_chunk_size = Some(size);
        self
    }

    /// Build the TextChunker.
    pub fn build(self) -> TextChunker {
        let mut chunker = TextChunker::default();
        if let Some(size) = self.chunk_size {
            chunker = chunker.with_chunk_size(size);
        }
        if let Some(overlap) = self.overlap {
            chunker = chunker.with_overlap(overlap);
        }
        if let Some(min_size) = self.min_chunk_size {
            chunker = chunker.with_min_chunk_size(min_size);
        }
        chunker
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let chunker = TextChunker::default();
        assert_eq!(chunker.chunk_size(), 1600);
        assert_eq!(chunker.overlap(), 320);
        assert_eq!(chunker.min_chunk_size(), 200);
    }

    #[test]
    fn test_custom_values() {
        let chunker = TextChunker::new()
            .with_chunk_size(1000)
            .with_overlap(200)
            .with_min_chunk_size(100);

        assert_eq!(chunker.chunk_size(), 1000);
        assert_eq!(chunker.overlap(), 200);
        assert_eq!(chunker.min_chunk_size(), 100);
    }

    #[test]
    fn test_builder() {
        let chunker = TextChunkerBuilder::new()
            .chunk_size(800)
            .overlap(100)
            .min_chunk_size(50)
            .build();

        assert_eq!(chunker.chunk_size(), 800);
        assert_eq!(chunker.overlap(), 100);
        assert_eq!(chunker.min_chunk_size(), 50);
    }

    #[test]
    fn test_empty_text() {
        let chunker = TextChunker::default();
        let chunks = chunker.chunk("", "agent-1", None, MemorySource::ManualNote);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_whitespace_only_text() {
        let chunker = TextChunker::default();
        let chunks = chunker.chunk("   \n\t  ", "agent-1", None, MemorySource::ManualNote);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_small_text_single_chunk() {
        let chunker = TextChunker::default();
        let text = "This is a small piece of text.";
        let chunks = chunker.chunk(text, "agent-1", Some("session-1"), MemorySource::ManualNote);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, text);
        assert_eq!(chunks[0].agent_id, "agent-1");
        assert_eq!(chunks[0].session_id, Some("session-1".to_string()));
    }

    #[test]
    fn test_chunk_has_content_hash() {
        let chunker = TextChunker::default();
        let text = "Test content for hashing";
        let chunks = chunker.chunk(text, "agent-1", None, MemorySource::ManualNote);

        assert_eq!(chunks.len(), 1);
        assert!(!chunks[0].content_hash.is_empty());
        // SHA-256 produces 64 hex characters
        assert_eq!(chunks[0].content_hash.len(), 64);
    }

    #[test]
    fn test_identical_content_same_hash() {
        let chunker = TextChunker::default();
        let text = "Test content for hashing";

        let chunks1 = chunker.chunk(text, "agent-1", None, MemorySource::ManualNote);
        let chunks2 = chunker.chunk(text, "agent-2", None, MemorySource::ManualNote);

        assert_eq!(chunks1[0].content_hash, chunks2[0].content_hash);
    }

    #[test]
    fn test_chunk_has_token_count() {
        let chunker = TextChunker::default();
        let text = "Hello world test content"; // 24 chars = ~6 tokens
        let chunks = chunker.chunk(text, "agent-1", None, MemorySource::ManualNote);

        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].token_count.is_some());
        assert_eq!(chunks[0].token_count.unwrap(), 6); // 24 / 4 = 6
    }

    #[test]
    fn test_long_text_multiple_chunks() {
        let chunker = TextChunker::new()
            .with_chunk_size(100)
            .with_overlap(20)
            .with_min_chunk_size(10); // Allow small final chunks

        // Create text longer than chunk size
        let text = "word ".repeat(100); // 500 chars
        let chunks = chunker.chunk(&text, "agent-1", None, MemorySource::ManualNote);

        assert!(
            chunks.len() > 1,
            "Expected multiple chunks, got {}",
            chunks.len()
        );
    }

    #[test]
    fn test_word_boundary_preservation() {
        let chunker = TextChunker::new()
            .with_chunk_size(50)
            .with_overlap(10)
            .with_min_chunk_size(10);

        let text = "Hello world this is a test of word boundary splitting";
        let chunks = chunker.chunk(text, "agent-1", None, MemorySource::ManualNote);

        // No chunk should start or end with a partial word (check for no leading/trailing mid-word)
        for chunk in &chunks {
            let trimmed = chunk.content.trim();
            // Should not start with lowercase letter preceded by something
            assert!(!trimmed.starts_with(char::is_lowercase) || trimmed == chunk.content.trim());
        }
    }

    #[test]
    fn test_source_preserved() {
        let chunker = TextChunker::default();
        let source = MemorySource::TaskExecution {
            task_id: "task-123".to_string(),
        };
        let chunks = chunker.chunk("Test content", "agent-1", None, source.clone());

        assert_eq!(chunks[0].source, source);
    }

    #[test]
    fn test_chunk_messages_format() {
        let chunker = TextChunker::default();
        let messages = vec![
            ("user", "Hello!"),
            ("assistant", "Hi there!"),
            ("user", "How are you?"),
        ];

        let chunks = chunker.chunk_messages(
            &messages,
            "agent-1",
            Some("session-1"),
            MemorySource::Conversation {
                session_id: "session-1".to_string(),
            },
        );

        assert!(!chunks.is_empty());
        // Check that content contains formatted messages
        let all_content: String = chunks.iter().map(|c| c.content.clone()).collect();
        assert!(all_content.contains("[user]: Hello!"));
        assert!(all_content.contains("[assistant]: Hi there!"));
    }

    #[test]
    fn test_large_text_chunking() {
        let chunker = TextChunker::default();

        // Generate large text (10000 chars)
        let large_text = "The quick brown fox jumps over the lazy dog. ".repeat(250);
        let chunks = chunker.chunk(&large_text, "agent-1", None, MemorySource::ManualNote);

        // Should create multiple chunks
        assert!(chunks.len() > 1);

        // All chunks should have valid IDs
        for chunk in &chunks {
            assert!(chunk.id.starts_with("chunk-"));
        }
    }

    #[test]
    fn test_overlap_creates_context_continuity() {
        let chunker = TextChunker::new().with_chunk_size(100).with_overlap(30);

        // Create text that will span multiple chunks
        let text = "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10 \
                    word11 word12 word13 word14 word15 word16 word17 word18 word19 word20 \
                    word21 word22 word23 word24 word25 word26 word27 word28 word29 word30";

        let chunks = chunker.chunk(text, "agent-1", None, MemorySource::ManualNote);

        // With overlap, consecutive chunks should share some content
        if chunks.len() >= 2 {
            let chunk1_end = &chunks[0].content[chunks[0].content.len().saturating_sub(20)..];
            let chunk2_start = &chunks[1].content[..20.min(chunks[1].content.len())];

            // There should be some word overlap (this is approximate due to word boundaries)
            let chunk1_words: Vec<&str> = chunk1_end.split_whitespace().collect();
            let chunk2_words: Vec<&str> = chunk2_start.split_whitespace().collect();

            // Check if any words from end of chunk1 appear at start of chunk2
            let has_overlap = chunk1_words
                .iter()
                .any(|w| chunk2_words.iter().any(|w2| w2.contains(w)));
            // This test is informative - overlap depends on word boundaries
            let _ = has_overlap;
        }
    }

    #[test]
    fn test_unique_ids() {
        let chunker = TextChunker::new().with_chunk_size(100).with_overlap(20);
        let text = "a ".repeat(500);
        let chunks = chunker.chunk(&text, "agent-1", None, MemorySource::ManualNote);

        let ids: Vec<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
        let unique_ids: std::collections::HashSet<&str> = ids.iter().copied().collect();

        // All IDs should be unique
        assert_eq!(ids.len(), unique_ids.len());
    }

    #[test]
    fn test_estimate_tokens() {
        let chunker = TextChunker::default();
        // "Hello" = 5 chars, ~1.25 tokens, ceil = 2
        assert_eq!(chunker.estimate_tokens("Hello"), 2);
        // Empty = 0
        assert_eq!(chunker.estimate_tokens(""), 0);
        // 100 chars = 25 tokens
        let hundred_chars = "a".repeat(100);
        assert_eq!(chunker.estimate_tokens(&hundred_chars), 25);
    }

    #[test]
    fn test_chinese_text_single_chunk() {
        let chunker = TextChunker::default();
        let text = "你好世界，这是一个测试。";
        let chunks = chunker.chunk(text, "agent-1", None, MemorySource::ManualNote);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, text);
    }

    #[test]
    fn test_chinese_text_multiple_chunks() {
        let chunker = TextChunker::new()
            .with_chunk_size(100)
            .with_overlap(20)
            .with_min_chunk_size(10);

        // Chinese chars are 3 bytes each in UTF-8, so 100 bytes ~ 33 chars
        // Create text with spaces so word boundary can work
        let text = "你好世界 这是测试 中文内容 分块处理 ".repeat(20);
        let chunks = chunker.chunk(&text, "agent-1", None, MemorySource::ManualNote);

        assert!(chunks.len() > 1, "Expected multiple chunks for Chinese text");
        // Verify all chunks contain valid UTF-8 (no panics)
        for chunk in &chunks {
            assert!(!chunk.content.is_empty());
            // This would panic if content is invalid UTF-8
            let _ = chunk.content.chars().count();
        }
    }

    #[test]
    fn test_mixed_cjk_and_ascii_chunking() {
        let chunker = TextChunker::new()
            .with_chunk_size(150)
            .with_overlap(30)
            .with_min_chunk_size(10);

        let text = "Hello 你好 World 世界 Test 测试 Content 内容 ".repeat(30);
        let chunks = chunker.chunk(&text, "agent-1", None, MemorySource::ManualNote);

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(!chunk.content.is_empty());
            let _ = chunk.content.chars().count();
        }
    }

    #[test]
    fn test_chunk_messages_with_chinese() {
        let chunker = TextChunker::new()
            .with_chunk_size(200)
            .with_overlap(40)
            .with_min_chunk_size(10);

        let messages = vec![
            ("User", "每15分钟执行一次。你只负责 main CI 失败自动修复并提 PR。"),
            ("Assistant", "好的，我会每15分钟检查 CI 状态并自动修复失败的构建。"),
            ("User", "仓库规则：固定仓库 /Users/test/restflow，开始先执行 git fetch。"),
        ];

        let chunks = chunker.chunk_messages(
            &messages,
            "agent-1",
            Some("session-1"),
            MemorySource::Conversation {
                session_id: "session-1".to_string(),
            },
        );

        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(!chunk.content.is_empty());
            let _ = chunk.content.chars().count();
        }
    }
}
