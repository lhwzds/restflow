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
 * Save voice message audio file for AI processing.
 * Used for voice message mode (long press mic button).
 * Returns the file path where the audio was saved.
 */
export function saveVoiceMessage(audioBase64: string): Promise<string> {
  return tauriInvoke<string>('save_voice_message', { audioBase64 })
}
