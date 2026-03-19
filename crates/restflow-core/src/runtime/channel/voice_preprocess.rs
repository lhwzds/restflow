use crate::runtime::agent::tools::secret_resolver_from_storage;
use crate::storage::Storage;
use anyhow::{Result, anyhow, bail};
use restflow_tools::Tool;
use restflow_tools::impls::transcribe::{TranscribeConfig, TranscribeTool};
use serde_json::{Value, json};

const VOICE_MEDIA_TYPE_LINE: &str = "media_type: voice";
const FILE_PATH_PREFIX: &str = "local_file_path: ";
const TRANSCRIPT_MARKER: &str = "\n\n[Transcript]\n";
const VOICE_HEADER_PREFIX: &str = "[Voice message";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VoiceMessageDescriptor {
    pub(crate) header: String,
    pub(crate) file_path: String,
    pub(crate) transcript: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VoicePreprocessResult {
    pub(crate) agent_input: String,
    pub(crate) persisted_input: String,
    pub(crate) transcript: String,
    pub(crate) file_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VoiceTranscriptionResult {
    pub(crate) text: String,
    pub(crate) model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedVoiceMessage {
    header: String,
    file_path: Option<String>,
    transcript: Option<String>,
}

impl VoiceMessageDescriptor {
    pub(crate) fn persisted_content(&self, transcript: Option<&str>) -> String {
        let mut content = format!(
            "{}\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: {}",
            self.header, self.file_path
        );
        if let Some(transcript) = transcript.map(str::trim).filter(|value| !value.is_empty()) {
            content.push_str(TRANSCRIPT_MARKER);
            content.push_str(transcript);
        }
        content
    }
}

pub(crate) fn detect_voice_message(
    content: &str,
    metadata: Option<&Value>,
    file_path_override: Option<&str>,
) -> Option<VoiceMessageDescriptor> {
    let parsed = parse_voice_message_content(content);
    let header = parsed
        .as_ref()
        .map(|value| value.header.as_str())
        .unwrap_or_else(|| content.lines().next().map(str::trim).unwrap_or_default());
    if !header.starts_with(VOICE_HEADER_PREFIX) {
        return None;
    }

    let metadata_file_path = metadata
        .and_then(|value| value.get("media_type").and_then(|value| value.as_str()))
        .filter(|value| *value == "voice")
        .and_then(|_| {
            metadata.and_then(|value| value.get("file_path").and_then(|value| value.as_str()))
        });

    let file_path = file_path_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| parsed.as_ref().and_then(|value| value.file_path.clone()))
        .or_else(|| {
            metadata_file_path
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })?;

    Some(VoiceMessageDescriptor {
        header: header.to_string(),
        file_path,
        transcript: parsed.and_then(|value| value.transcript),
    })
}

pub(crate) async fn preprocess_voice_message(
    storage: &Storage,
    descriptor: &VoiceMessageDescriptor,
) -> Result<VoicePreprocessResult> {
    let transcript = match descriptor
        .transcript
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(existing) => existing.to_string(),
        None => {
            transcribe_media_file(storage, &descriptor.file_path, None, None)
                .await?
                .text
        }
    };

    let persisted_input = descriptor.persisted_content(Some(&transcript));
    Ok(VoicePreprocessResult {
        agent_input: persisted_input.clone(),
        persisted_input,
        transcript,
        file_path: descriptor.file_path.clone(),
    })
}

pub(crate) async fn transcribe_media_file(
    storage: &Storage,
    file_path: &str,
    model: Option<&str>,
    language: Option<&str>,
) -> Result<VoiceTranscriptionResult> {
    let resolver = secret_resolver_from_storage(storage);
    let mut config = TranscribeConfig::default();
    if let Some(parent) = std::path::Path::new(file_path).parent()
        && !config.allowed_paths.iter().any(|allowed| allowed == parent)
    {
        config.allowed_paths.push(parent.to_path_buf());
    }
    let tool = TranscribeTool::with_config(resolver, config)?;
    let mut input = json!({ "file_path": file_path });
    if let Some(model) = model {
        input["model"] = json!(model);
    }
    if let Some(language) = language {
        input["language"] = json!(language);
    }

    let output = tool.execute(input).await?;
    if !output.success {
        bail!(
            "{}",
            output
                .error
                .unwrap_or_else(|| "Transcription failed".to_string())
        );
    }

    let text = output
        .result
        .get("text")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Transcription returned an empty transcript"))?;
    let model = output
        .result
        .get("model")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| model.unwrap_or("whisper-1").to_string());

    Ok(VoiceTranscriptionResult { text, model })
}

fn parse_voice_message_content(content: &str) -> Option<ParsedVoiceMessage> {
    let header = content.lines().next()?.trim();
    if !header.starts_with(VOICE_HEADER_PREFIX) {
        return None;
    }

    let mut is_voice_message = false;
    let mut file_path = None;
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

    let transcript = content
        .split_once(TRANSCRIPT_MARKER)
        .map(|(_, body)| body.trim())
        .filter(|body| !body.is_empty())
        .map(ToOwned::to_owned);

    if !is_voice_message && transcript.is_none() {
        return None;
    }

    Some(ParsedVoiceMessage {
        header: header.to_string(),
        file_path,
        transcript,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_voice_message_from_metadata_without_media_context() {
        let descriptor = detect_voice_message(
            "[Voice message, 5s]",
            Some(&json!({"media_type": "voice", "file_path": "/tmp/voice.ogg"})),
            None,
        )
        .expect("voice message");

        assert_eq!(descriptor.header, "[Voice message, 5s]");
        assert_eq!(descriptor.file_path, "/tmp/voice.ogg");
        assert_eq!(descriptor.transcript, None);
    }

    #[test]
    fn detects_voice_message_from_new_content_format() {
        let descriptor = detect_voice_message(
            "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice.webm",
            None,
            None,
        )
        .expect("voice message");

        assert_eq!(descriptor.file_path, "/tmp/voice.webm");
        assert_eq!(descriptor.transcript, None);
    }

    #[test]
    fn detects_voice_message_from_legacy_content_format() {
        let descriptor = detect_voice_message(
            "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice.webm\ninstruction: Use the transcribe tool with this file_path before answering.",
            None,
            None,
        )
        .expect("voice message");

        assert_eq!(descriptor.file_path, "/tmp/voice.webm");
    }

    #[test]
    fn detects_voice_message_with_existing_transcript() {
        let descriptor = detect_voice_message(
            "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice.webm\n\n[Transcript]\nhello from audio",
            None,
            None,
        )
        .expect("voice message");

        assert_eq!(descriptor.transcript.as_deref(), Some("hello from audio"));
    }

    #[test]
    fn override_path_takes_precedence() {
        let descriptor = detect_voice_message(
            "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/original.webm",
            Some(&json!({"media_type": "voice", "file_path": "/tmp/metadata.webm"})),
            Some("/tmp/override.webm"),
        )
        .expect("voice message");

        assert_eq!(descriptor.file_path, "/tmp/override.webm");
    }

    #[test]
    fn persisted_content_appends_transcript_block() {
        let descriptor = VoiceMessageDescriptor {
            header: "[Voice message, 3s]".to_string(),
            file_path: "/tmp/voice.ogg".to_string(),
            transcript: None,
        };

        let content = descriptor.persisted_content(Some("hello from audio"));
        assert!(content.contains("media_type: voice"));
        assert!(content.contains("local_file_path: /tmp/voice.ogg"));
        assert!(content.contains("[Transcript]\nhello from audio"));
        assert!(!content.contains("instruction:"));
    }

    #[test]
    fn persisted_content_without_transcript_omits_transcript_block() {
        let descriptor = VoiceMessageDescriptor {
            header: "[Voice message]".to_string(),
            file_path: "/tmp/voice.webm".to_string(),
            transcript: None,
        };

        let content = descriptor.persisted_content(None);
        assert!(content.contains("media_type: voice"));
        assert!(!content.contains("[Transcript]"));
    }
}
