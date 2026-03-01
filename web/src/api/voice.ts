/**
 * Voice API
 *
 * Provides voice recording and transcription operations
 * for the Tauri desktop application.
 */

import { invokeCommand } from './tauri-client'

export interface TranscribeResult {
  text: string
  model: string
}

/**
 * Transcribe audio to text via daemon's transcribe tool.
 * Used for voice-to-text mode (click mic button).
 */
export function transcribeAudio(
  audioBase64: string,
  model?: string,
  language?: string,
): Promise<TranscribeResult> {
  return invokeCommand('transcribeAudio', audioBase64, model ?? null, language ?? null)
}

/**
 * Start streaming transcription via OpenAI API directly.
 * Returns a transcribe_id; actual text arrives via Tauri events
 * on the 'voice:transcribe-stream' channel.
 */
export function transcribeAudioStream(
  audioBase64: string,
  model?: string,
  language?: string,
): Promise<string> {
  return invokeCommand('transcribeAudioStream', audioBase64, model ?? null, language ?? null)
}

/**
 * Save voice message audio file for AI processing.
 * Used for voice message mode (long press mic button).
 * If sessionId is provided, saves directly to ~/.restflow/media/{sessionId}/.
 * Returns the file path where the audio was saved.
 */
export function saveVoiceMessage(audioBase64: string, sessionId?: string): Promise<string> {
  return invokeCommand('saveVoiceMessage', audioBase64, sessionId ?? null)
}

/**
 * Read a media file from persistent storage (~/.restflow/media/).
 * Returns the file contents as base64.
 * Used for replaying voice messages after page reload.
 */
export function readMediaFile(filePath: string): Promise<string> {
  return invokeCommand('readMediaFile', filePath)
}

/**
 * Start a live transcription session via OpenAI Realtime WebSocket API.
 * Returns a transcribe_id; text deltas arrive via Tauri `voice:transcribe-stream` events.
 */
export function startLiveTranscription(model?: string, language?: string): Promise<string> {
  return invokeCommand('startLiveTranscription', model ?? null, language ?? null)
}

/**
 * Send a PCM16 audio chunk to an active live transcription session.
 * The audioBase64 should be base64-encoded Int16Array PCM data at 24kHz.
 */
export function sendLiveAudioChunk(transcribeId: string, audioBase64: string): Promise<void> {
  return invokeCommand('sendLiveAudioChunk', transcribeId, audioBase64).then(() => undefined)
}

/**
 * Stop a live transcription session gracefully.
 * The final Completed event will be emitted via Tauri events.
 */
export function stopLiveTranscription(transcribeId: string): Promise<void> {
  return invokeCommand('stopLiveTranscription', transcribeId).then(() => undefined)
}
