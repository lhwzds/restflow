//! Markdown-aware message chunking for Telegram.
//!
//! Telegram limits text messages to 4096 characters. This module splits long
//! messages at safe boundaries while preserving Markdown code fence state,
//! so code blocks are never broken mid-fence.

/// Telegram hard limit for text messages.
#[allow(dead_code)]
const TELEGRAM_MAX_LEN: usize = 4096;

/// Default split threshold (leaves headroom for fence re-opening markup).
const DEFAULT_MAX_LEN: usize = 4000;

/// Split `text` into chunks of at most `max_len` characters that respect
/// Markdown fenced code blocks.
///
/// Rules:
/// 1. Never split inside a code block without closing/reopening the fence.
/// 2. Prefer splitting at paragraph boundaries (`\n\n`).
/// 3. Fall back to line boundaries (`\n`), then hard cut.
/// 4. When a split occurs inside a fence, the current chunk gets a closing
///    `` ``` `` and the next chunk gets an opening `` ```lang ``.
pub fn chunk_markdown(text: &str, max_len: Option<usize>) -> Vec<String> {
    let limit = max_len.unwrap_or(DEFAULT_MAX_LEN);

    if text.is_empty() {
        return Vec::new();
    }
    if text.len() <= limit {
        return vec![text.to_string()];
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut remaining = text;
    let mut in_fence = false;
    let mut fence_lang: Option<String> = None;

    while !remaining.is_empty() {
        if remaining.len() <= limit {
            chunks.push(remaining.to_string());
            break;
        }

        // Determine the best split point within `limit` chars.
        // Find a safe UTF-8 character boundary
        let safe_limit = remaining
            .char_indices()
            .map(|(i, _)| i)
            .take_while(|&i| i <= limit)
            .last()
            .unwrap_or(0);
        let candidate = &remaining[..safe_limit];
        let split_at = find_split_point(candidate, safe_limit, in_fence);

        let chunk_text = &remaining[..split_at];

        // Track fence state across the chunk we are about to emit.
        let (new_in_fence, new_fence_lang) = scan_fence_state(chunk_text, in_fence, &fence_lang);

        if new_in_fence {
            // We are splitting inside a code fence — close it in this chunk
            // and reopen it in the next.
            let mut closed = chunk_text.to_string();
            closed.push_str("\n```");
            chunks.push(closed);

            // Advance past the split point.
            remaining = skip_leading_newlines(&remaining[split_at..]);

            // The next chunk will reopen the fence.
            let opener = match &new_fence_lang {
                Some(lang) => format!("```{}\n", lang),
                None => "```\n".to_string(),
            };
            remaining = &remaining[0..]; // no-op, but keeps logic clear
            // Prepend the opener to the remaining text by collecting into a
            // new owned string. We break the borrow here intentionally.
            let reopened = format!("{}{}", opener, remaining);
            // Recurse with the reopened text — this handles deeply nested
            // splits correctly without complex state tracking.
            let sub_chunks = chunk_markdown(&reopened, Some(limit));
            chunks.extend(sub_chunks);
            return chunks;
        }

        chunks.push(chunk_text.to_string());
        remaining = skip_leading_newlines(&remaining[split_at..]);

        // Carry fence state forward.
        in_fence = new_in_fence;
        fence_lang = new_fence_lang;
    }

    chunks
}

/// Find the best byte offset to split `candidate` (up to `limit` bytes).
fn find_split_point(candidate: &str, limit: usize, _in_fence: bool) -> usize {
    // 1. Try paragraph boundary (last `\n\n`).
    if let Some(pos) = candidate.rfind("\n\n")
        && pos > 0
    {
        return pos;
    }

    // 2. Try line boundary (last `\n`).
    if let Some(pos) = candidate.rfind('\n')
        && pos > 0
    {
        return pos;
    }

    // 3. Hard cut at limit.
    // Make sure we don't split in the middle of a multi-byte character.
    let mut cut = limit;
    while cut > 0 && !candidate.is_char_boundary(cut) {
        cut -= 1;
    }
    cut
}

/// Scan `text` for fence toggles starting from the given state.
/// Returns the final (in_fence, fence_lang) after processing all lines.
fn scan_fence_state(
    text: &str,
    mut in_fence: bool,
    current_lang: &Option<String>,
) -> (bool, Option<String>) {
    let mut lang = current_lang.clone();

    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(after_backticks) = trimmed.strip_prefix("```") {
            if in_fence {
                // Closing fence.
                in_fence = false;
                lang = None;
            } else {
                // Opening fence — capture optional language tag.
                in_fence = true;
                let after = after_backticks.trim();
                lang = if after.is_empty() {
                    None
                } else {
                    // Take first word as lang tag.
                    Some(after.split_whitespace().next().unwrap_or("").to_string())
                };
            }
        }
    }

    (in_fence, lang)
}

/// Skip leading newlines at the split boundary so the next chunk doesn't
/// start with blank lines left over from a paragraph or line split.
fn skip_leading_newlines(s: &str) -> &str {
    s.trim_start_matches(['\n', '\r'])
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_text_no_split() {
        let text = "Hello, world!";
        let chunks = chunk_markdown(text, Some(100));
        assert_eq!(chunks, vec!["Hello, world!"]);
    }

    #[test]
    fn test_empty_input() {
        let chunks = chunk_markdown("", Some(100));
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_exact_limit() {
        let text = "a".repeat(100);
        let chunks = chunk_markdown(&text, Some(100));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_split_at_paragraph_boundary() {
        let text = format!("{}\n\n{}", "a".repeat(50), "b".repeat(50));
        // Limit of 60 forces a split — should prefer the \n\n at position 50.
        let chunks = chunk_markdown(&text, Some(60));
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "a".repeat(50));
        assert_eq!(chunks[1], "b".repeat(50));
    }

    #[test]
    fn test_split_at_line_boundary() {
        let text = format!("{}\n{}", "a".repeat(50), "b".repeat(50));
        let chunks = chunk_markdown(&text, Some(60));
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "a".repeat(50));
        assert_eq!(chunks[1], "b".repeat(50));
    }

    #[test]
    fn test_code_block_preserved() {
        // A code block that fits in one chunk should not be split.
        let text = "```rust\nfn main() {}\n```";
        let chunks = chunk_markdown(text, Some(100));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_code_block_spanning_chunks() {
        // Force a split inside a code block.
        let code_lines: Vec<String> = (0..20).map(|i| format!("let x{} = {};", i, i)).collect();
        let text = format!("```rust\n{}\n```", code_lines.join("\n"));
        // Use a small limit to force splitting.
        let chunks = chunk_markdown(&text, Some(80));

        assert!(chunks.len() >= 2, "Expected multiple chunks");
        // First chunk should end with closing fence.
        assert!(
            chunks[0].ends_with("```"),
            "First chunk should close the fence: {:?}",
            chunks[0]
        );
        // Second chunk should open with a fence (with lang tag).
        assert!(
            chunks[1].starts_with("```rust"),
            "Second chunk should reopen the fence: {:?}",
            chunks[1]
        );
        // Last chunk should close the fence.
        assert!(
            chunks.last().unwrap().ends_with("```")
                || chunks.last().unwrap().contains("```"),
            "Last chunk should contain a closing fence"
        );
    }

    #[test]
    fn test_code_block_with_lang_tag() {
        let text = format!(
            "Before\n\n```python\n{}\n```\n\nAfter",
            (0..30)
                .map(|i| format!("print({})", i))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let chunks = chunk_markdown(&text, Some(120));

        // Find the chunk that reopens the fence — it should have the lang tag.
        let reopened = chunks.iter().find(|c| c.starts_with("```python"));
        assert!(
            reopened.is_some(),
            "Should reopen fence with python lang tag. Chunks: {:?}",
            chunks
        );
    }

    #[test]
    fn test_long_single_line() {
        // A single long line with no newlines forces a hard cut.
        let text = "x".repeat(250);
        let chunks = chunk_markdown(&text, Some(100));
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[1].len(), 100);
        assert_eq!(chunks[2].len(), 50);
    }

    #[test]
    fn test_multiple_code_blocks() {
        let text = "```js\nconsole.log(1);\n```\n\nSome text\n\n```py\nprint(2)\n```";
        let chunks = chunk_markdown(text, Some(200));
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_inline_backticks_not_treated_as_fence() {
        let text = "Use `code` inline and ``double`` too.\n\nMore text here.";
        let chunks = chunk_markdown(text, Some(200));
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_default_limit() {
        // Verify default limit is used when None is passed.
        let text = "a".repeat(DEFAULT_MAX_LEN + 1);
        let chunks = chunk_markdown(&text, None);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn test_utf8_chinese_char_boundary() {
        // Test chunking with Chinese characters at potential split boundaries
        // Chinese chars are 3 bytes each in UTF-8
        let chinese = "这是一个包含中文字符的测试）。";
        let text = chinese.repeat(100); // ~1500 bytes

        // Set limit to a value that would split a multi-byte char if not handled correctly
        let chunks = chunk_markdown(&text, Some(500));

        // Should not panic and should produce valid UTF-8 chunks
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            // Verify each chunk is valid UTF-8
            assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
        }

        // Verify content is preserved when joined
        let rejoined = chunks.join("");
        assert_eq!(rejoined.chars().count(), text.chars().count());
    }

    #[test]
    fn test_utf8_mixed_content() {
        // Mix English, Chinese, and emojis
        let text = format!(
            "English text {}中文内容{} emoji 🚀🎉 {}",
            "x".repeat(200),
            "测试".repeat(50),
            "y".repeat(200)
        );

        let chunks = chunk_markdown(&text, Some(300));

        // Should not panic
        assert!(!chunks.is_empty());

        // All chunks should be valid UTF-8
        for chunk in &chunks {
            assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
        }
    }
}
