//! Voice recording Tauri commands
//!
//! Provides three commands:
//! - `transcribe_audio`: Voice-to-text via daemon's transcribe tool
//! - `transcribe_audio_stream`: Streaming voice-to-text via OpenAI API directly
//! - `save_voice_message`: Save audio file for AI transcribe tool usage

use crate::state::AppState;
use base64::Engine;
use futures::StreamExt;
use reqwest::multipart;
use restflow_core::daemon::{IpcRequest, IpcResponse};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};
use tracing::{debug, error, warn};
use ts_rs::TS;

const TS_EXPORT_TO_WEB_TYPES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/types/generated/"
);

/// Event name for transcribe stream events
pub const VOICE_TRANSCRIBE_STREAM_EVENT: &str = "voice:transcribe-stream";

/// A streaming transcription event emitted during audio transcription
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = TS_EXPORT_TO_WEB_TYPES)]
pub struct TranscribeStreamEvent {
    /// Unique ID for this transcription
    pub transcribe_id: String,
    /// Event timestamp (Unix ms)
    pub timestamp: i64,
    /// Event payload
    pub kind: TranscribeStreamKind,
}

/// Types of transcribe stream events
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = TS_EXPORT_TO_WEB_TYPES)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscribeStreamKind {
    /// Stream started
    Started { model: String },
    /// Text delta received
    Delta { text: String },
    /// Transcription completed
    Completed { full_text: String, duration_ms: u64 },
    /// Transcription failed
    Failed {
        error: String,
        partial_text: Option<String>,
    },
}

/// Result returned by the transcribe_audio command
#[derive(Debug, Clone, Serialize)]
pub struct TranscribeResult {
    pub text: String,
    pub model: String,
}

/// Voice-to-text: decode base64 audio, save to temp file, call daemon transcribe tool
#[tauri::command]
pub async fn transcribe_audio(
    state: State<'_, AppState>,
    audio_base64: String,
    model: Option<String>,
    language: Option<String>,
) -> Result<TranscribeResult, String> {
    let file_path = save_audio_to_temp(&audio_base64)?;
    debug!(path = %file_path, "Saved audio for transcription");

    let model_name = model.unwrap_or_else(|| "gpt-4o-mini-transcribe".to_string());

    let mut input = serde_json::json!({
        "file_path": file_path,
        "model": model_name,
    });
    if let Some(lang) = &language {
        input["language"] = serde_json::json!(lang);
    }

    // Call daemon's transcribe tool via IPC
    let mut daemon = state.daemon.lock().await;
    let client = daemon.ensure_connected().await.map_err(|e| e.to_string())?;
    let response = client
        .request(IpcRequest::ExecuteTool {
            name: "transcribe".to_string(),
            input,
        })
        .await
        .map_err(|e| e.to_string())?;

    // Clean up temp file (best effort)
    let _ = std::fs::remove_file(&file_path);

    match response {
        IpcResponse::Success(value) => {
            // The tool returns { success, result, error }
            let success = value
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !success {
                let error = value
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Transcription failed");
                return Err(error.to_string());
            }

            // Extract text from tool result
            let result = value.get("result").cloned().unwrap_or_default();
            let text = result
                .as_str()
                .map(String::from)
                .or_else(|| {
                    result
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .unwrap_or_default();

            Ok(TranscribeResult {
                text,
                model: model_name,
            })
        }
        IpcResponse::Error { code, message } => Err(format!("IPC error {}: {}", code, message)),
        IpcResponse::Pong => Err("Unexpected Pong response".to_string()),
    }
}

/// Save audio as a file for AI to process with transcribe tool
#[tauri::command]
pub async fn save_voice_message(audio_base64: String) -> Result<String, String> {
    let file_path = save_audio_to_temp(&audio_base64)?;
    debug!(path = %file_path, "Saved voice message file");
    Ok(file_path)
}

/// Streaming voice-to-text: calls OpenAI API directly with stream=true,
/// emits SSE text deltas via Tauri events. Returns the transcribe_id immediately.
#[tauri::command]
pub async fn transcribe_audio_stream(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    audio_base64: String,
    model: Option<String>,
    language: Option<String>,
) -> Result<String, String> {
    let file_path = save_audio_to_temp(&audio_base64)?;
    debug!(path = %file_path, "Saved audio for streaming transcription");

    let model_name = model.unwrap_or_else(|| "gpt-4o-mini-transcribe".to_string());
    let transcribe_id = uuid::Uuid::new_v4().to_string();

    // Get the OpenAI API key via executor
    let api_key = state
        .executor()
        .get_secret("OPENAI_API_KEY".to_string())
        .await
        .map_err(|e| format!("Failed to get API key: {}", e))?
        .ok_or_else(|| "OPENAI_API_KEY not configured".to_string())?;

    let tid = transcribe_id.clone();
    let model_clone = model_name.clone();

    // Spawn background task to handle streaming
    tokio::spawn(async move {
        let start = std::time::Instant::now();

        // Emit started event
        let _ = app.emit(
            VOICE_TRANSCRIBE_STREAM_EVENT,
            TranscribeStreamEvent {
                transcribe_id: tid.clone(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                kind: TranscribeStreamKind::Started {
                    model: model_clone.clone(),
                },
            },
        );

        let result =
            run_streaming_transcription(&app, &tid, &file_path, &api_key, &model_clone, language)
                .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Clean up temp file
        let _ = std::fs::remove_file(&file_path);

        match result {
            Ok(full_text) => {
                let _ = app.emit(
                    VOICE_TRANSCRIBE_STREAM_EVENT,
                    TranscribeStreamEvent {
                        transcribe_id: tid,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        kind: TranscribeStreamKind::Completed {
                            full_text,
                            duration_ms,
                        },
                    },
                );
            }
            Err((error, partial_text)) => {
                let _ = app.emit(
                    VOICE_TRANSCRIBE_STREAM_EVENT,
                    TranscribeStreamEvent {
                        transcribe_id: tid,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        kind: TranscribeStreamKind::Failed {
                            error,
                            partial_text,
                        },
                    },
                );
            }
        }
    });

    Ok(transcribe_id)
}

/// Run the actual streaming transcription against OpenAI API.
/// Returns Ok(full_text) on success, Err((error, partial_text)) on failure.
async fn run_streaming_transcription(
    app: &tauri::AppHandle,
    transcribe_id: &str,
    file_path: &str,
    api_key: &str,
    model: &str,
    language: Option<String>,
) -> Result<String, (String, Option<String>)> {
    // Read the audio file
    let file_bytes = std::fs::read(file_path)
        .map_err(|e| (format!("Failed to read audio file: {}", e), None))?;

    let file_name = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.webm")
        .to_string();

    // Build multipart form
    let file_part = multipart::Part::bytes(file_bytes)
        .file_name(file_name)
        .mime_str("audio/webm")
        .map_err(|e| (format!("Failed to create multipart: {}", e), None))?;

    let mut form = multipart::Form::new()
        .part("file", file_part)
        .text("model", model.to_string())
        .text("stream", "true");

    if let Some(lang) = language {
        form = form.text("language", lang);
    }

    // Send request to OpenAI
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await
        .map_err(|e| (format!("HTTP request failed: {}", e), None))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_string());
        return Err((format!("OpenAI API error {}: {}", status, body), None));
    }

    // Parse SSE stream
    let mut full_text = String::new();
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| (format!("Stream read error: {}", e), Some(full_text.clone())))?;

        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);

        // Process complete SSE lines from buffer
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim_end_matches('\r').to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() || line.starts_with("event:") {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    continue;
                }

                match serde_json::from_str::<serde_json::Value>(data) {
                    Ok(json) => {
                        let event_type = json.get("type").and_then(|v| v.as_str());
                        match event_type {
                            Some("transcript.text.delta") => {
                                if let Some(delta) = json.get("delta").and_then(|v| v.as_str()) {
                                    full_text.push_str(delta);
                                    let _ = app.emit(
                                        VOICE_TRANSCRIBE_STREAM_EVENT,
                                        TranscribeStreamEvent {
                                            transcribe_id: transcribe_id.to_string(),
                                            timestamp: chrono::Utc::now().timestamp_millis(),
                                            kind: TranscribeStreamKind::Delta {
                                                text: delta.to_string(),
                                            },
                                        },
                                    );
                                }
                            }
                            Some("transcript.text.done") => {
                                if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
                                    full_text = text.to_string();
                                }
                            }
                            _ => {
                                // Ignore other event types (e.g., logprobs)
                            }
                        }
                    }
                    Err(e) => {
                        warn!(data = data, error = %e, "Failed to parse SSE data");
                    }
                }
            }
        }
    }

    if full_text.is_empty() {
        error!("Streaming transcription returned no text");
    }

    Ok(full_text)
}

/// Decode base64 audio and write to a temp file under /tmp/restflow-media/
fn save_audio_to_temp(audio_base64: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64)
        .map_err(|e| format!("Failed to decode base64 audio: {}", e))?;

    let dir = std::path::Path::new("/tmp/restflow-media");
    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create media dir: {}", e))?;

    let filename = format!("tauri-{}.webm", uuid::Uuid::new_v4());
    let file_path = dir.join(&filename);

    std::fs::write(&file_path, &bytes).map_err(|e| format!("Failed to write audio file: {}", e))?;

    Ok(file_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn test_save_audio_to_temp_valid_base64() {
        let sample = b"fake audio data";
        let encoded = base64::engine::general_purpose::STANDARD.encode(sample);
        let path = save_audio_to_temp(&encoded).unwrap();

        assert!(path.starts_with("/tmp/restflow-media/tauri-"));
        assert!(path.ends_with(".webm"));

        // Verify file content
        let content = std::fs::read(&path).unwrap();
        assert_eq!(content, sample);

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_audio_to_temp_invalid_base64() {
        let result = save_audio_to_temp("not valid base64!!!");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to decode base64"));
    }

    #[test]
    fn test_transcribe_result_serialization() {
        let result = TranscribeResult {
            text: "Hello world".to_string(),
            model: "whisper-1".to_string(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["text"], "Hello world");
        assert_eq!(json["model"], "whisper-1");
    }

    #[test]
    fn test_transcribe_stream_event_started() {
        let event = TranscribeStreamEvent {
            transcribe_id: "test-id".to_string(),
            timestamp: 1234567890,
            kind: TranscribeStreamKind::Started {
                model: "gpt-4o-mini-transcribe".to_string(),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["transcribe_id"], "test-id");
        assert_eq!(json["kind"]["type"], "started");
        assert_eq!(json["kind"]["model"], "gpt-4o-mini-transcribe");
    }

    #[test]
    fn test_transcribe_stream_event_delta() {
        let event = TranscribeStreamEvent {
            transcribe_id: "test-id".to_string(),
            timestamp: 1234567890,
            kind: TranscribeStreamKind::Delta {
                text: "Hello ".to_string(),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"]["type"], "delta");
        assert_eq!(json["kind"]["text"], "Hello ");
    }

    #[test]
    fn test_transcribe_stream_event_completed() {
        let event = TranscribeStreamEvent {
            transcribe_id: "test-id".to_string(),
            timestamp: 1234567890,
            kind: TranscribeStreamKind::Completed {
                full_text: "Hello world".to_string(),
                duration_ms: 1500,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"]["type"], "completed");
        assert_eq!(json["kind"]["full_text"], "Hello world");
        assert_eq!(json["kind"]["duration_ms"], 1500);
    }

    #[test]
    fn test_transcribe_stream_event_failed() {
        let event = TranscribeStreamEvent {
            transcribe_id: "test-id".to_string(),
            timestamp: 1234567890,
            kind: TranscribeStreamKind::Failed {
                error: "API error".to_string(),
                partial_text: Some("Hello".to_string()),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"]["type"], "failed");
        assert_eq!(json["kind"]["error"], "API error");
        assert_eq!(json["kind"]["partial_text"], "Hello");
    }

    #[test]
    fn test_transcribe_stream_event_failed_no_partial() {
        let event = TranscribeStreamEvent {
            transcribe_id: "test-id".to_string(),
            timestamp: 1234567890,
            kind: TranscribeStreamKind::Failed {
                error: "No key".to_string(),
                partial_text: None,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"]["type"], "failed");
        assert!(json["kind"]["partial_text"].is_null());
    }
}
