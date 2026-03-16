/**
 * Voice API
 *
 * Browser-friendly wrappers around daemon voice HTTP endpoints.
 */

import { fetchJson } from './http-client'

export interface TranscribeResult {
  text: string
  model: string
}

export function transcribeAudio(
  audioBase64: string,
  model?: string,
  language?: string,
): Promise<TranscribeResult> {
  return fetchJson<TranscribeResult>('/api/voice/transcribe', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      audio_base64: audioBase64,
      model: model ?? null,
      language: language ?? null,
    }),
  })
}

export function saveVoiceMessage(audioBase64: string, sessionId?: string): Promise<string> {
  return fetchJson<string>('/api/voice/save', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      audio_base64: audioBase64,
      session_id: sessionId ?? null,
    }),
  })
}

export function readMediaFile(filePath: string): Promise<string> {
  return fetchJson<string>('/api/voice/read', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ file_path: filePath }),
  })
}
