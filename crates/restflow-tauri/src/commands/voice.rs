//! Voice recording Tauri commands
//!
//! Provides two commands:
//! - `transcribe_audio`: Voice-to-text via daemon's transcribe tool
//! - `save_voice_message`: Save audio file for AI transcribe tool usage

use crate::state::AppState;
use base64::Engine;
use restflow_core::daemon::{IpcRequest, IpcResponse};
use serde::Serialize;
use tauri::State;
use tracing::debug;

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
            let success = value.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
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
                .or_else(|| result.get("text").and_then(|v| v.as_str()).map(String::from))
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

/// Decode base64 audio and write to a temp file under /tmp/restflow-media/
fn save_audio_to_temp(audio_base64: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64)
        .map_err(|e| format!("Failed to decode base64 audio: {}", e))?;

    let dir = std::path::Path::new("/tmp/restflow-media");
    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create media dir: {}", e))?;

    let filename = format!("tauri-{}.webm", uuid::Uuid::new_v4());
    let file_path = dir.join(&filename);

    std::fs::write(&file_path, &bytes)
        .map_err(|e| format!("Failed to write audio file: {}", e))?;

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
}
