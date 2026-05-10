use types::{ChatMessage, ChatMessageMedia, ChatMessageTranscript, ChatRole};

const VOICE_MEDIA_TYPE_LINE: &str = "media_type: voice";
const FILE_PATH_PREFIX: &str = "local_file_path: ";
const TRANSCRIPT_MARKER: &str = "\n\n[Transcript]\n";
const VOICE_HEADER_PREFIX: &str = "[Voice message";

/// Populate structured voice metadata from legacy message content blocks.
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
