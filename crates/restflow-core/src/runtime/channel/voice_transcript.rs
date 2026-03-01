use crate::models::chat_session::{ChatMessageMedia, ChatMessageTranscript};
use crate::models::{
    ChatMessage, ChatRole, ChatSession, ToolCallCompletion, ToolTrace, ToolTraceEvent,
};
use std::collections::HashMap;

const TRANSCRIBE_TOOL_NAME: &str = "transcribe";
const VOICE_MEDIA_TYPE_LINE: &str = "media_type: voice";
const FILE_PATH_PREFIX: &str = "local_file_path: ";
const TRANSCRIPT_MARKER: &str = "\n\n[Transcript]\n";
const VOICE_HEADER_PREFIX: &str = "[Voice message";

/// Populate structured voice metadata from legacy message content blocks.
///
/// This keeps existing stored message text compatible while progressively
/// hydrating `ChatMessage.media` and `ChatMessage.transcript`.
pub(crate) fn hydrate_voice_message_metadata(message: &mut ChatMessage) -> bool {
    if message.role != ChatRole::User {
        return false;
    }

    let mut changed = false;
    if message.media.is_none()
        && let Some(file_path) = extract_voice_file_path(&message.content)
    {
        let duration = extract_voice_duration_sec(&message.content);
        message.media = Some(ChatMessageMedia::voice(file_path, duration));
        changed = true;
    }

    if let Some(transcript_text) = extract_transcript_from_message_content(&message.content) {
        let should_update = message
            .transcript
            .as_ref()
            .is_none_or(|existing| existing.text.trim() != transcript_text);
        if should_update {
            message.transcript = Some(ChatMessageTranscript::new(transcript_text, None));
            changed = true;
        }
    }

    changed
}

/// Enrich a voice message content with transcript text extracted from tool traces.
///
/// Returns `Some(updated_content)` only when:
/// - the message is a voice media-context message,
/// - a `transcribe` tool call for the same `file_path` exists in this turn,
/// - transcript text can be extracted from that tool result.
pub(crate) fn enrich_voice_message_with_transcript(
    message_content: &str,
    traces: &[ToolTrace],
) -> Option<String> {
    let voice_path = extract_voice_file_path(message_content)?;
    let transcript = find_matching_transcript(traces, &voice_path)?;
    let updated = upsert_transcript_block(message_content, &transcript);
    if updated == message_content {
        None
    } else {
        Some(updated)
    }
}

/// Replace the latest user message matching `original_content`.
///
/// This is used by chat-session execution paths where the user message has already
/// been persisted before tool execution, and transcript is backfilled after the turn.
pub(crate) fn replace_latest_user_message_content(
    session: &mut ChatSession,
    original_content: &str,
    updated_content: &str,
) -> bool {
    if original_content == updated_content {
        return false;
    }

    let Some(index) = session
        .messages
        .iter()
        .rposition(|message| message.role == ChatRole::User && message.content == original_content)
    else {
        return false;
    };

    let message = &mut session.messages[index];
    message.content = updated_content.to_string();
    hydrate_voice_message_metadata(message);
    true
}

fn extract_voice_file_path(content: &str) -> Option<String> {
    let mut is_voice_message = false;
    let mut file_path: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line == VOICE_MEDIA_TYPE_LINE {
            is_voice_message = true;
            continue;
        }

        if let Some(path) = line.strip_prefix(FILE_PATH_PREFIX) {
            let normalized = path.trim();
            if !normalized.is_empty() {
                file_path = Some(normalized.to_string());
            }
        }
    }

    if is_voice_message { file_path } else { None }
}

fn extract_voice_duration_sec(content: &str) -> Option<u32> {
    let first_line = content.lines().next()?.trim();
    if !first_line.starts_with(VOICE_HEADER_PREFIX) {
        return None;
    }
    let (_, tail) = first_line.split_once(',')?;
    let seconds = tail.trim().strip_suffix("s]")?.trim();
    seconds.parse::<u32>().ok()
}

fn extract_transcript_from_message_content(content: &str) -> Option<String> {
    let (_, body) = content.split_once(TRANSCRIPT_MARKER)?;
    let transcript = body.trim();
    if transcript.is_empty() {
        None
    } else {
        Some(transcript.to_string())
    }
}

fn parse_json_value(input: &str) -> Option<serde_json::Value> {
    if input.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(input).ok()
}

fn extract_file_path_from_payload(payload: Option<&str>) -> Option<String> {
    let payload = payload?;
    let value = parse_json_value(payload)?;
    value
        .get("file_path")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn extract_text_from_payload(payload: &str) -> Option<String> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(value) = parse_json_value(trimmed) {
        if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
            let normalized = text.trim();
            if !normalized.is_empty() {
                return Some(normalized.to_string());
            }
        }
        if let Some(text) = value.as_str() {
            let normalized = text.trim();
            if !normalized.is_empty() {
                return Some(normalized.to_string());
            }
        }
        return None;
    }

    Some(trimmed.to_string())
}

fn extract_transcript_from_completion(completion: &ToolCallCompletion) -> Option<String> {
    if let Some(path) = completion.output_ref.as_deref()
        && let Ok(content) = std::fs::read_to_string(path)
        && let Some(text) = extract_text_from_payload(&content)
    {
        return Some(text);
    }

    completion
        .output
        .as_deref()
        .and_then(extract_text_from_payload)
}

fn find_matching_transcript(traces: &[ToolTrace], voice_path: &str) -> Option<String> {
    let mut call_to_file_path: HashMap<String, String> = HashMap::new();

    for trace in traces {
        if trace.tool_name.as_deref() != Some(TRANSCRIBE_TOOL_NAME) {
            continue;
        }
        if trace.event_type != ToolTraceEvent::ToolCallStarted {
            continue;
        }
        let Some(tool_call_id) = trace.tool_call_id.as_deref() else {
            continue;
        };
        if let Some(path) = extract_file_path_from_payload(trace.input.as_deref()) {
            call_to_file_path.insert(tool_call_id.to_string(), path);
        }
    }

    for trace in traces {
        if trace.tool_name.as_deref() != Some(TRANSCRIBE_TOOL_NAME) {
            continue;
        }
        if trace.event_type != ToolTraceEvent::ToolCallCompleted || !trace.success.unwrap_or(false)
        {
            continue;
        }
        let Some(tool_call_id) = trace.tool_call_id.as_deref() else {
            continue;
        };

        let completion = ToolCallCompletion {
            output: trace.output.clone(),
            output_ref: trace.output_ref.clone(),
            success: trace.success.unwrap_or(false),
            duration_ms: trace.duration_ms,
            error: trace.error.clone(),
        };
        let Some(transcript) = extract_transcript_from_completion(&completion) else {
            continue;
        };

        let path = call_to_file_path
            .get(tool_call_id)
            .cloned()
            .or_else(|| extract_file_path_from_payload(trace.output.as_deref()));
        if path.as_deref() == Some(voice_path) {
            return Some(transcript);
        }
    }

    None
}

fn upsert_transcript_block(message_content: &str, transcript: &str) -> String {
    let transcript = transcript.trim();
    if transcript.is_empty() {
        return message_content.to_string();
    }

    if let Some((prefix, _)) = message_content.split_once(TRANSCRIPT_MARKER) {
        format!("{prefix}{TRANSCRIPT_MARKER}{transcript}")
    } else {
        format!("{message_content}{TRANSCRIPT_MARKER}{transcript}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ToolTrace;
    use crate::models::chat_session::ChatMediaType;
    use serde_json::json;
    use tempfile::tempdir;

    fn voice_message(path: &str) -> String {
        format!(
            "[Voice message, 6s]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: {path}\ninstruction: Use the transcribe tool with this file_path before answering."
        )
    }

    #[test]
    fn hydrate_voice_metadata_from_content() {
        let mut message = ChatMessage::user(voice_message("/tmp/voice-a.webm"));
        let changed = hydrate_voice_message_metadata(&mut message);
        assert!(changed);
        assert_eq!(
            message.media.as_ref().map(|m| m.media_type),
            Some(ChatMediaType::Voice)
        );
        assert_eq!(
            message.media.as_ref().map(|m| m.file_path.as_str()),
            Some("/tmp/voice-a.webm")
        );
        assert_eq!(message.media.as_ref().and_then(|m| m.duration_sec), Some(6));
        assert!(message.transcript.is_none());
    }

    #[test]
    fn hydrate_transcript_from_content_block() {
        let mut message = ChatMessage::user(format!(
            "{}\n\n[Transcript]\nhello from transcript",
            voice_message("/tmp/voice-a.webm")
        ));
        let changed = hydrate_voice_message_metadata(&mut message);
        assert!(changed);
        assert_eq!(
            message.transcript.as_ref().map(|t| t.text.as_str()),
            Some("hello from transcript")
        );
    }

    #[test]
    fn enriches_voice_message_with_matching_transcript() {
        let input = voice_message("/tmp/voice-a.webm");
        let start = ToolTrace::tool_call_started(
            "session-1",
            "turn-1",
            "call-1",
            "transcribe",
            Some(json!({"file_path": "/tmp/voice-a.webm"}).to_string()),
        );
        let done = ToolTrace::tool_call_completed(
            "session-1",
            "turn-1",
            "call-1",
            "transcribe",
            ToolCallCompletion {
                output: Some(json!({"text": "hello from audio"}).to_string()),
                output_ref: None,
                success: true,
                duration_ms: Some(20),
                error: None,
            },
        );

        let updated =
            enrich_voice_message_with_transcript(&input, &[start, done]).expect("should enrich");
        assert!(updated.contains("[Transcript]"));
        assert!(updated.contains("hello from audio"));
    }

    #[test]
    fn does_not_enrich_when_file_path_does_not_match() {
        let input = voice_message("/tmp/voice-a.webm");
        let start = ToolTrace::tool_call_started(
            "session-1",
            "turn-1",
            "call-1",
            "transcribe",
            Some(json!({"file_path": "/tmp/voice-b.webm"}).to_string()),
        );
        let done = ToolTrace::tool_call_completed(
            "session-1",
            "turn-1",
            "call-1",
            "transcribe",
            ToolCallCompletion {
                output: Some(json!({"text": "other audio"}).to_string()),
                output_ref: None,
                success: true,
                duration_ms: Some(20),
                error: None,
            },
        );

        let updated = enrich_voice_message_with_transcript(&input, &[start, done]);
        assert!(updated.is_none());
    }

    #[test]
    fn enriches_with_output_ref_when_output_is_not_embedded() {
        let temp_dir = tempdir().expect("tempdir");
        let output_path = temp_dir.path().join("transcribe-output.json");
        std::fs::write(&output_path, json!({"text": "from output ref"}).to_string())
            .expect("write output ref");

        let input = voice_message("/tmp/voice-a.webm");
        let start = ToolTrace::tool_call_started(
            "session-1",
            "turn-1",
            "call-1",
            "transcribe",
            Some(json!({"file_path": "/tmp/voice-a.webm"}).to_string()),
        );
        let done = ToolTrace::tool_call_completed(
            "session-1",
            "turn-1",
            "call-1",
            "transcribe",
            ToolCallCompletion {
                output: None,
                output_ref: Some(output_path.to_string_lossy().to_string()),
                success: true,
                duration_ms: Some(20),
                error: None,
            },
        );

        let updated =
            enrich_voice_message_with_transcript(&input, &[start, done]).expect("should enrich");
        assert!(updated.contains("from output ref"));
    }

    #[test]
    fn replace_latest_matching_user_message_hydrates_metadata() {
        let original = voice_message("/tmp/voice-a.webm");
        let updated = format!("{original}\n\n[Transcript]\nhello");

        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::user("older message"));
        session.add_message(ChatMessage::user(original.clone()));

        let changed = replace_latest_user_message_content(&mut session, &original, &updated);
        assert!(changed);
        let message = session.messages.last().expect("last message should exist");
        assert_eq!(message.content, updated);
        assert_eq!(
            message.media.as_ref().map(|m| m.file_path.as_str()),
            Some("/tmp/voice-a.webm")
        );
        assert_eq!(
            message.transcript.as_ref().map(|t| t.text.as_str()),
            Some("hello")
        );
    }
}
