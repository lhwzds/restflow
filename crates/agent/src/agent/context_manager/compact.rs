use crate::error::Result;
use crate::llm::{CompletionRequest, LlmClient, Message, Role};

use super::config::ContextManagerConfig;
use super::constants::{COMPACT_MIN_REDUCTION, HANDOFF_PROMPT, SUMMARY_TRUNCATE_CHARS};
use super::token::{estimate_message_tokens, estimate_tokens, middle_truncate};

/// Statistics from a compact operation.
#[derive(Debug, Clone, Default)]
pub struct CompactStats {
    pub messages_replaced: usize,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub summary_length: usize,
}

/// Check whether compaction should be triggered.
pub fn should_compact(estimated_tokens: usize, config: &ContextManagerConfig) -> bool {
    if config.context_window == 0 {
        return false;
    }
    let threshold = (config.context_window as f64 * config.compact_trigger_ratio) as usize;
    estimated_tokens > threshold
}

/// Format conversation transcript for the summarization LLM call.
pub(crate) fn format_conversation_for_summary(messages: &[Message]) -> String {
    let mut out = String::new();
    for msg in messages {
        let role_label = match msg.role {
            Role::System => "SYSTEM",
            Role::User => "USER",
            Role::Assistant => "ASSISTANT",
            Role::Tool => "TOOL",
        };

        let content = if msg.content.len() > SUMMARY_TRUNCATE_CHARS {
            middle_truncate(&msg.content, SUMMARY_TRUNCATE_CHARS)
        } else {
            msg.content.clone()
        };

        out.push_str(&format!("[{role_label}] {content}\n\n"));

        if let Some(calls) = &msg.tool_calls {
            for call in calls {
                let args_str = call.arguments.to_string();
                let args_display = if args_str.len() > 200 {
                    middle_truncate(&args_str, 200)
                } else {
                    args_str
                };
                out.push_str(&format!("  -> tool_call: {}({args_display})\n", call.name));
            }
        }
    }
    out
}

/// Find split point: preserve recent ~compact_preserve_tokens of messages,
/// aligned to a safe message boundary (never split between an assistant with
/// tool_calls and its corresponding tool results).
pub(crate) fn find_compact_split(messages: &[Message], preserve_tokens: usize) -> usize {
    if messages.is_empty() {
        return 0;
    }

    // Accumulate tokens from the end until we reach preserve_tokens.
    let mut accumulated = 0;
    let mut split = messages.len();

    for i in (0..messages.len()).rev() {
        accumulated += estimate_message_tokens(&messages[i]);
        if accumulated >= preserve_tokens {
            split = i;
            break;
        }
    }

    // Never remove the system prompt (index 0).
    if split <= 1 {
        return 1;
    }

    // Align to safe boundary: if split lands on a Tool message, walk forward
    // past all consecutive Tool messages to avoid orphaning them from their
    // assistant+tool_calls parent.
    while split < messages.len() && messages[split].role == Role::Tool {
        split += 1;
    }

    // Also check: if messages[split-1] is an Assistant with tool_calls,
    // we must include those tool results too — walk forward.
    if split > 0
        && let Some(calls) = &messages[split - 1].tool_calls
        && !calls.is_empty()
    {
        while split < messages.len() && messages[split].role == Role::Tool {
            split += 1;
        }
    }

    split
}

/// Generate a handoff summary and replace old messages.
///
/// Returns `CompactStats` with `messages_replaced == 0` if there's nothing to
/// compact, or if the LLM returns an empty summary (safety: don't replace
/// real history with nothing).
pub async fn compact(
    messages: &mut Vec<Message>,
    config: &ContextManagerConfig,
    llm: &dyn LlmClient,
) -> Result<CompactStats> {
    let tokens_before = estimate_tokens(messages);
    let split = find_compact_split(messages, config.compact_preserve_tokens);

    // Nothing to compact if split is at 1 (only system prompt) or beyond end.
    if split <= 1 || split >= messages.len() {
        return Ok(CompactStats {
            messages_replaced: 0,
            tokens_before,
            tokens_after: tokens_before,
            summary_length: 0,
        });
    }

    // Extract old messages for summarization (skip system prompt at [0]).
    let old_messages = &messages[1..split];
    let transcript = format_conversation_for_summary(old_messages);

    // Ask LLM for handoff summary.
    let summary_request = CompletionRequest::new(vec![
        Message::system(HANDOFF_PROMPT),
        Message::user(transcript),
    ]);

    let response = llm.complete(summary_request).await?;
    let summary = response.content.unwrap_or_default();

    // Safety: don't replace real messages with an empty summary.
    if summary.trim().is_empty() {
        tracing::warn!("LLM returned empty summary, skipping compaction");
        return Ok(CompactStats {
            messages_replaced: 0,
            tokens_before,
            tokens_after: tokens_before,
            summary_length: 0,
        });
    }

    // Rebuild messages: system + summary + preserved tail.
    let system_msg = messages[0].clone();
    let preserved = messages[split..].to_vec();

    let summary_msg = Message::user(format!("[Session Summary]\n\n{summary}"));
    let summary_length = summary.len();

    messages.clear();
    messages.push(system_msg);
    messages.push(summary_msg);
    messages.extend(preserved);

    let tokens_after = estimate_tokens(messages);

    Ok(CompactStats {
        messages_replaced: split - 1, // excluding system prompt
        tokens_before,
        tokens_after,
        summary_length,
    })
}

/// Check whether compaction was effective. If the reduction ratio is too small,
/// the caller should activate a cooldown to prevent compaction loops.
pub fn compact_was_effective(stats: &CompactStats) -> bool {
    if stats.tokens_before == 0 || stats.messages_replaced == 0 {
        return false;
    }
    let ratio = stats.tokens_after as f64 / stats.tokens_before as f64;
    ratio < COMPACT_MIN_REDUCTION
}
