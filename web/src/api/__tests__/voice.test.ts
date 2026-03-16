import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fetchJson } from '../http-client'
import { readMediaFile, saveVoiceMessage, transcribeAudio } from '../voice'

vi.mock('../http-client', () => ({
  fetchJson: vi.fn(),
}))

describe('voice API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('posts audio transcription requests to the daemon', async () => {
    vi.mocked(fetchJson).mockResolvedValue({ text: 'Hello world', model: 'whisper-1' })

    const result = await transcribeAudio('base64data', 'whisper-1', 'en')

    expect(fetchJson).toHaveBeenCalledWith('/api/voice/transcribe', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        audio_base64: 'base64data',
        model: 'whisper-1',
        language: 'en',
      }),
    })
    expect(result).toEqual({ text: 'Hello world', model: 'whisper-1' })
  })

  it('saves voice messages through the daemon HTTP API', async () => {
    vi.mocked(fetchJson).mockResolvedValue('/tmp/voice.webm')

    const result = await saveVoiceMessage('base64data')

    expect(fetchJson).toHaveBeenCalledWith('/api/voice/save', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        audio_base64: 'base64data',
        session_id: null,
      }),
    })
    expect(result).toBe('/tmp/voice.webm')
  })

  it('reads media files through the daemon HTTP API', async () => {
    vi.mocked(fetchJson).mockResolvedValue('encoded-audio')

    const result = await readMediaFile('/tmp/voice.webm')

    expect(fetchJson).toHaveBeenCalledWith('/api/voice/read', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ file_path: '/tmp/voice.webm' }),
    })
    expect(result).toBe('encoded-audio')
  })
})
