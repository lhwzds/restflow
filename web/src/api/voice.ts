/**
 * Voice API
 *
 * Provides voice recording and transcription operations
 * for the Tauri desktop application.
 */

import { tauriInvoke } from './tauri-client'

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
  return tauriInvoke<TranscribeResult>('transcribe_audio', {
    audioBase64,
    model: model ?? null,
    language: language ?? null,
  })
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
  return tauriInvoke<string>('transcribe_audio_stream', {
    audioBase64,
    model: model ?? null,
    language: language ?? null,
  })
}

/**
 * Save voice message audio file for AI processing.
 * Used for voice message mode (long press mic button).
 * Returns the file path where the audio was saved.
 */
export function saveVoiceMessage(audioBase64: string): Promise<string> {
  return tauriInvoke<string>('save_voice_message', { audioBase64 })
}

/**
 * Start a live transcription session via OpenAI Realtime WebSocket API.
 * Returns a transcribe_id; text deltas arrive via Tauri `voice:transcribe-stream` events.
 */
export function startLiveTranscription(
  model?: string,
  language?: string,
): Promise<string> {
  return tauriInvoke<string>('start_live_transcription', {
    model: model ?? null,
    language: language ?? null,
  })
}

/**
 * Send a PCM16 audio chunk to an active live transcription session.
 * The audioBase64 should be base64-encoded Int16Array PCM data at 24kHz.
 */
export function sendLiveAudioChunk(
  transcribeId: string,
  audioBase64: string,
): Promise<void> {
  return tauriInvoke<void>('send_live_audio_chunk', {
    transcribeId,
    audioBase64,
  })
}

/**
 * Stop a live transcription session gracefully.
 * The final Completed event will be emitted via Tauri events.
 */
export function stopLiveTranscription(transcribeId: string): Promise<void> {
  return tauriInvoke<void>('stop_live_transcription', { transcribeId })
}
