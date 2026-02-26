import { describe, it, expect, vi, beforeEach } from 'vitest'
import { transcribeAudio, transcribeAudioStream, saveVoiceMessage } from '../voice'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

describe('voice API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('transcribeAudio', () => {
    it('should call transcribe_audio with correct arguments', async () => {
      const { invoke } = await import('@tauri-apps/api/core')
      const mockInvoke = vi.mocked(invoke)
      mockInvoke.mockResolvedValue({ text: 'Hello world', model: 'whisper-1' })

      const result = await transcribeAudio('base64data', 'whisper-1', 'en')

      expect(mockInvoke).toHaveBeenCalledWith('transcribe_audio', {
        audioBase64: 'base64data',
        model: 'whisper-1',
        language: 'en',
      })
      expect(result).toEqual({ text: 'Hello world', model: 'whisper-1' })
    })

    it('should pass null for optional params when not provided', async () => {
      const { invoke } = await import('@tauri-apps/api/core')
      const mockInvoke = vi.mocked(invoke)
      mockInvoke.mockResolvedValue({ text: 'test', model: 'gpt-4o-mini-transcribe' })

      await transcribeAudio('base64data')

      expect(mockInvoke).toHaveBeenCalledWith('transcribe_audio', {
        audioBase64: 'base64data',
        model: null,
        language: null,
      })
    })
  })

  describe('transcribeAudioStream', () => {
    it('should call transcribe_audio_stream and return transcribe_id', async () => {
      const { invoke } = await import('@tauri-apps/api/core')
      const mockInvoke = vi.mocked(invoke)
      mockInvoke.mockResolvedValue('abc-123-transcribe-id')

      const result = await transcribeAudioStream('base64data', 'gpt-4o-mini-transcribe', 'en')

      expect(mockInvoke).toHaveBeenCalledWith('transcribe_audio_stream', {
        audioBase64: 'base64data',
        model: 'gpt-4o-mini-transcribe',
        language: 'en',
      })
      expect(result).toBe('abc-123-transcribe-id')
    })

    it('should pass null for optional params when not provided', async () => {
      const { invoke } = await import('@tauri-apps/api/core')
      const mockInvoke = vi.mocked(invoke)
      mockInvoke.mockResolvedValue('some-id')

      await transcribeAudioStream('base64data')

      expect(mockInvoke).toHaveBeenCalledWith('transcribe_audio_stream', {
        audioBase64: 'base64data',
        model: null,
        language: null,
      })
    })
  })

  describe('saveVoiceMessage', () => {
    it('should call save_voice_message and return file path', async () => {
      const { invoke } = await import('@tauri-apps/api/core')
      const mockInvoke = vi.mocked(invoke)
      mockInvoke.mockResolvedValue('/tmp/restflow-media/tauri-abc123.webm')

      const result = await saveVoiceMessage('base64data')

      expect(mockInvoke).toHaveBeenCalledWith('save_voice_message', {
        audioBase64: 'base64data',
      })
      expect(result).toBe('/tmp/restflow-media/tauri-abc123.webm')
    })
  })
})
