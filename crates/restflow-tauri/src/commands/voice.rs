//! Voice recording Tauri commands
//!
//! Provides commands for:
//! - `transcribe_audio`: Voice-to-text via daemon's transcribe tool
//! - `transcribe_audio_stream`: Streaming voice-to-text via OpenAI API directly
//! - `save_voice_message`: Save audio file for AI transcribe tool usage
//! - `read_media_file`: Read a media file from persistent storage as base64
//! - `start_live_transcription`: Live transcription via OpenAI Realtime WebSocket API
//! - `send_live_audio_chunk`: Forward PCM16 audio chunks to an active live session
//! - `stop_live_transcription`: Stop a live transcription session

use crate::state::AppState;
use base64::Engine;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use reqwest::multipart;
use restflow_core::daemon::{IpcRequest, IpcResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use tauri::{Emitter, State};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};
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
    /// A VAD segment finished; carries the de-duplicated full text so far.
    /// The frontend should replace its streamingText with this value.
    SegmentDone { corrected_text: String },
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

/// Save audio as a file for AI to process with transcribe tool.
/// If `session_id` is provided, saves directly to `~/.restflow/media/{session_id}/`.
/// Otherwise saves to `~/.restflow/media/` (will be relocated by ChatDispatcher later).
#[tauri::command]
pub async fn save_voice_message(
    audio_base64: String,
    session_id: Option<String>,
) -> Result<String, String> {
    let file_path = save_audio_to_session(&audio_base64, session_id.as_deref())?;
    debug!(path = %file_path, "Saved voice message file");
    Ok(file_path)
}

/// Read a media file from `~/.restflow/media/` and return its contents as base64.
/// Path must be under the media directory for security.
#[tauri::command]
pub async fn read_media_file(file_path: String) -> Result<String, String> {
    let media_dir = restflow_core::paths::media_dir()
        .map_err(|e| format!("Failed to resolve media dir: {}", e))?;
    let requested = std::path::Path::new(&file_path);

    // Security: ensure the path is under ~/.restflow/media/
    if !requested.starts_with(&media_dir) {
        return Err("Path is not within the media directory".to_string());
    }

    let bytes =
        std::fs::read(requested).map_err(|e| format!("Failed to read media file: {}", e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
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

// ============================================================================
// Live Transcription (OpenAI Realtime WebSocket API)
// ============================================================================

type WsStream = tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// An active live transcription session.
struct LiveSession {
    /// Channel to send base64-encoded PCM16 audio chunks to the WebSocket task.
    audio_tx: mpsc::Sender<String>,
    /// Signal to stop the session.
    stop_tx: watch::Sender<bool>,
}

/// Global registry of active live transcription sessions.
fn live_sessions() -> &'static RwLock<HashMap<String, LiveSession>> {
    static SESSIONS: OnceLock<RwLock<HashMap<String, LiveSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Start a live transcription session using the OpenAI Realtime WebSocket API.
/// Returns a transcribe_id; text deltas arrive via Tauri `voice:transcribe-stream` events.
#[tauri::command]
pub async fn start_live_transcription(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    model: Option<String>,
    language: Option<String>,
) -> Result<String, String> {
    let transcribe_id = uuid::Uuid::new_v4().to_string();
    let model_name = model.unwrap_or_else(|| "gpt-4o-transcribe".to_string());

    // Get OpenAI API key
    let api_key = state
        .executor()
        .get_secret("OPENAI_API_KEY".to_string())
        .await
        .map_err(|e| format!("Failed to get API key: {}", e))?
        .ok_or_else(|| "OPENAI_API_KEY not configured".to_string())?;

    // Build WebSocket request with auth headers
    let ws_url = "wss://api.openai.com/v1/realtime?intent=transcription";
    let request = http::Request::builder()
        .uri(ws_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("OpenAI-Beta", "realtime=v1")
        .header("Host", "api.openai.com")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .map_err(|e| format!("Failed to build WebSocket request: {}", e))?;

    // Connect to OpenAI Realtime API
    let (ws_stream, _response) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| format!("WebSocket connection failed: {}", e))?;

    info!(transcribe_id = %transcribe_id, "Connected to OpenAI Realtime API");

    let (ws_write, ws_read) = ws_stream.split();

    // Create channels
    let (audio_tx, audio_rx) = mpsc::channel::<String>(256);
    let (stop_tx, stop_rx) = watch::channel(false);

    // Register session
    {
        let mut sessions = live_sessions().write().unwrap();
        sessions.insert(transcribe_id.clone(), LiveSession { audio_tx, stop_tx });
    }

    // Emit started event
    let _ = app.emit(
        VOICE_TRANSCRIBE_STREAM_EVENT,
        TranscribeStreamEvent {
            transcribe_id: transcribe_id.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: TranscribeStreamKind::Started {
                model: model_name.clone(),
            },
        },
    );

    // Spawn the WebSocket event loop
    let tid = transcribe_id.clone();
    let lang = language.clone();
    tokio::spawn(async move {
        live_transcription_task(
            app, tid, model_name, lang, ws_write, ws_read, audio_rx, stop_rx,
        )
        .await;
    });

    Ok(transcribe_id)
}

/// Send a PCM16 audio chunk to an active live transcription session.
#[tauri::command]
pub async fn send_live_audio_chunk(
    transcribe_id: String,
    audio_base64: String,
) -> Result<(), String> {
    let tx = {
        let sessions = live_sessions().read().unwrap();
        sessions.get(&transcribe_id).map(|s| s.audio_tx.clone())
    };

    let tx = tx.ok_or_else(|| format!("No active session: {}", transcribe_id))?;

    tx.send(audio_base64)
        .await
        .map_err(|_| "Session closed".to_string())
}

/// Stop a live transcription session gracefully.
#[tauri::command]
pub async fn stop_live_transcription(transcribe_id: String) -> Result<(), String> {
    let stop_tx = {
        let sessions = live_sessions().read().unwrap();
        sessions.get(&transcribe_id).map(|s| s.stop_tx.clone())
    };

    if let Some(stop_tx) = stop_tx {
        let _ = stop_tx.send(true);
    }

    Ok(())
}

/// The background WebSocket event loop for a live transcription session.
async fn live_transcription_task(
    app: tauri::AppHandle,
    transcribe_id: String,
    model: String,
    language: Option<String>,
    mut ws_write: SplitSink<WsStream, WsMessage>,
    mut ws_read: SplitStream<WsStream>,
    mut audio_rx: mpsc::Receiver<String>,
    mut stop_rx: watch::Receiver<bool>,
) {
    let start = std::time::Instant::now();
    let mut full_text = String::new();
    // Tracks where the current VAD segment starts in full_text,
    // so we can replace delta-accumulated text with the authoritative transcript.
    let mut segment_start_idx: usize = 0;

    // Send session configuration
    let lang = language.as_deref().unwrap_or("en");
    let session_config = serde_json::json!({
        "type": "transcription_session.update",
        "session": {
            "input_audio_format": "pcm16",
            "input_audio_transcription": {
                "model": model,
                "language": lang
            },
            "turn_detection": {
                "type": "server_vad",
                "threshold": 0.5,
                "silence_duration_ms": 500
            }
        }
    });

    if let Err(e) = ws_write
        .send(WsMessage::Text(session_config.to_string().into()))
        .await
    {
        error!(error = %e, "Failed to send session config");
        emit_failed(
            &app,
            &transcribe_id,
            format!("Failed to send config: {}", e),
            None,
        );
        cleanup_session(&transcribe_id);
        return;
    }

    debug!(transcribe_id = %transcribe_id, "Sent session config to OpenAI");

    // Main event loop
    let mut stopped = false;
    loop {
        tokio::select! {
            // Forward audio chunks to OpenAI
            chunk = audio_rx.recv() => {
                match chunk {
                    Some(audio_b64) => {
                        let msg = serde_json::json!({
                            "type": "input_audio_buffer.append",
                            "audio": audio_b64
                        });
                        if let Err(e) = ws_write
                            .send(WsMessage::Text(msg.to_string().into()))
                            .await
                        {
                            warn!(error = %e, "Failed to send audio chunk");
                        }
                    }
                    None => {
                        // Channel closed, treat as stop
                        if !stopped {
                            stopped = true;
                            let commit = serde_json::json!({"type": "input_audio_buffer.commit"});
                            let _ = ws_write.send(WsMessage::Text(commit.to_string().into())).await;
                        }
                    }
                }
            }

            // Process WebSocket messages from OpenAI
            ws_msg = ws_read.next() => {
                match ws_msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Some(should_break) = handle_realtime_event(
                            &app, &transcribe_id, &text, &mut full_text, &mut segment_start_idx
                        ) {
                            if should_break && stopped {
                                break;
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        info!(transcribe_id = %transcribe_id, "WebSocket closed by server");
                        break;
                    }
                    Some(Err(e)) => {
                        error!(error = %e, "WebSocket error");
                        emit_failed(
                            &app,
                            &transcribe_id,
                            format!("WebSocket error: {}", e),
                            if full_text.is_empty() { None } else { Some(full_text.clone()) },
                        );
                        cleanup_session(&transcribe_id);
                        return;
                    }
                    None => {
                        // Stream ended
                        break;
                    }
                    _ => {} // Ignore binary, ping, pong
                }
            }

            // Handle stop signal
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() && !stopped {
                    debug!(transcribe_id = %transcribe_id, "Stop signal received");
                    // Commit any remaining audio
                    let commit = serde_json::json!({"type": "input_audio_buffer.commit"});
                    let _ = ws_write.send(WsMessage::Text(commit.to_string().into())).await;

                    // Wait for final transcription with timeout
                    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
                    loop {
                        tokio::select! {
                            ws_msg = ws_read.next() => {
                                match ws_msg {
                                    Some(Ok(WsMessage::Text(text))) => {
                                        if let Some(true) = handle_realtime_event(
                                            &app, &transcribe_id, &text, &mut full_text, &mut segment_start_idx
                                        ) {
                                            // Got a completed event, we can finish
                                            break;
                                        }
                                    }
                                    _ => break,
                                }
                            }
                            _ = tokio::time::sleep_until(deadline) => {
                                debug!(transcribe_id = %transcribe_id, "Timeout waiting for final transcription");
                                break;
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    // Close WebSocket
    let _ = ws_write.send(WsMessage::Close(None)).await;

    // Emit completed event
    let _ = app.emit(
        VOICE_TRANSCRIBE_STREAM_EVENT,
        TranscribeStreamEvent {
            transcribe_id: transcribe_id.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: TranscribeStreamKind::Completed {
                full_text,
                duration_ms,
            },
        },
    );

    cleanup_session(&transcribe_id);
    info!(transcribe_id = %transcribe_id, duration_ms, "Live transcription completed");
}

/// Parse an OpenAI Realtime API event and emit corresponding Tauri events.
/// Returns Some(true) if a transcription segment completed, Some(false) for other handled events.
fn handle_realtime_event(
    app: &tauri::AppHandle,
    transcribe_id: &str,
    text: &str,
    full_text: &mut String,
    segment_start_idx: &mut usize,
) -> Option<bool> {
    let json: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "Failed to parse Realtime event");
            return None;
        }
    };

    let event_type = json.get("type").and_then(|v| v.as_str())?;

    match event_type {
        "conversation.item.input_audio_transcription.delta" => {
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
            Some(false)
        }
        "conversation.item.input_audio_transcription.completed" => {
            if let Some(transcript) = json.get("transcript").and_then(|v| v.as_str()) {
                // Replace delta-accumulated segment text with the authoritative transcript.
                // This de-duplicates words at VAD segment boundaries where overlapping
                // audio causes the same word to appear in consecutive segments.
                let delta_segment = full_text[*segment_start_idx..].to_string();
                full_text.truncate(*segment_start_idx);
                if !full_text.is_empty() && !full_text.ends_with(' ') {
                    full_text.push(' ');
                }
                full_text.push_str(transcript);
                *segment_start_idx = full_text.len();

                debug!(
                    transcribe_id = %transcribe_id,
                    delta_segment = %delta_segment,
                    authoritative = %transcript,
                    corrected_full = %full_text,
                    "Segment completed (replaced delta text with authoritative transcript)"
                );

                // Emit SegmentDone so the frontend can correct its streamingText
                let _ = app.emit(
                    VOICE_TRANSCRIBE_STREAM_EVENT,
                    TranscribeStreamEvent {
                        transcribe_id: transcribe_id.to_string(),
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        kind: TranscribeStreamKind::SegmentDone {
                            corrected_text: full_text.clone(),
                        },
                    },
                );
            }
            Some(true)
        }
        "input_audio_buffer.speech_started" => {
            debug!(transcribe_id = %transcribe_id, "Speech started");
            Some(false)
        }
        "input_audio_buffer.speech_stopped" => {
            debug!(transcribe_id = %transcribe_id, "Speech stopped");
            Some(false)
        }
        "transcription_session.created" | "transcription_session.updated" => {
            debug!(transcribe_id = %transcribe_id, event_type, "Session event");
            Some(false)
        }
        "error" => {
            let error_msg = json
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown Realtime API error");
            error!(transcribe_id = %transcribe_id, error = error_msg, "Realtime API error");
            emit_failed(
                app,
                transcribe_id,
                error_msg.to_string(),
                if full_text.is_empty() {
                    None
                } else {
                    Some(full_text.clone())
                },
            );
            Some(false)
        }
        _ => {
            debug!(transcribe_id = %transcribe_id, event_type, "Unhandled Realtime event");
            None
        }
    }
}

/// Emit a Failed event.
fn emit_failed(
    app: &tauri::AppHandle,
    transcribe_id: &str,
    error: String,
    partial_text: Option<String>,
) {
    let _ = app.emit(
        VOICE_TRANSCRIBE_STREAM_EVENT,
        TranscribeStreamEvent {
            transcribe_id: transcribe_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: TranscribeStreamKind::Failed {
                error,
                partial_text,
            },
        },
    );
}

/// Remove a session from the global registry.
fn cleanup_session(transcribe_id: &str) {
    let mut sessions = live_sessions().write().unwrap();
    sessions.remove(transcribe_id);
}

/// Decode base64 audio and write to `~/.restflow/media/{session_id}/` (or `~/.restflow/media/` if no session).
fn save_audio_to_session(audio_base64: &str, session_id: Option<&str>) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64)
        .map_err(|e| format!("Failed to decode base64 audio: {}", e))?;

    let dir = match session_id {
        Some(sid) => restflow_core::paths::session_media_dir(sid)
            .map_err(|e| format!("Failed to create session media dir: {}", e))?,
        None => restflow_core::paths::media_dir()
            .map_err(|e| format!("Failed to create media dir: {}", e))?,
    };

    let filename = format!("voice-{}.webm", uuid::Uuid::new_v4());
    let file_path = dir.join(&filename);

    std::fs::write(&file_path, &bytes).map_err(|e| format!("Failed to write audio file: {}", e))?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Decode base64 audio and write to a temp file under ~/.restflow/media/
fn save_audio_to_temp(audio_base64: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64)
        .map_err(|e| format!("Failed to decode base64 audio: {}", e))?;

    let dir = restflow_core::paths::media_dir()
        .map_err(|e| format!("Failed to create media dir: {}", e))?;

    let filename = format!("tmp-{}.webm", uuid::Uuid::new_v4());
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

        let media_dir = restflow_core::paths::media_dir().unwrap();
        assert!(path.starts_with(media_dir.to_str().unwrap()));
        assert!(path.contains("tmp-"));
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
