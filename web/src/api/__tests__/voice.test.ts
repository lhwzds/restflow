import { describe, it, expect, vi, beforeEach } from 'vitest'
import { transcribeAudio, transcribeAudioStream, saveVoiceMessage } from '../voice'
import { invokeCommand } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  invokeCommand: vi.fn(),
}))

describe('voice API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('transcribeAudio', () => {
    it('should call transcribe_audio with correct arguments', async () => {
      const mockInvokeCommand = vi.mocked(invokeCommand)
      mockInvokeCommand.mockResolvedValue({ text: 'Hello world', model: 'whisper-1' })

      const result = await transcribeAudio('base64data', 'whisper-1', 'en')

      expect(mockInvokeCommand).toHaveBeenCalledWith(
        'transcribeAudio',
        'base64data',
        'whisper-1',
        'en',
      )
      expect(result).toEqual({ text: 'Hello world', model: 'whisper-1' })
    })

    it('should pass null for optional params when not provided', async () => {
      const mockInvokeCommand = vi.mocked(invokeCommand)
      mockInvokeCommand.mockResolvedValue({ text: 'test', model: 'gpt-4o-mini-transcribe' })

      await transcribeAudio('base64data')

      expect(mockInvokeCommand).toHaveBeenCalledWith('transcribeAudio', 'base64data', null, null)
    })
  })

  describe('transcribeAudioStream', () => {
    it('should call transcribe_audio_stream and return transcribe_id', async () => {
      const mockInvokeCommand = vi.mocked(invokeCommand)
      mockInvokeCommand.mockResolvedValue('abc-123-transcribe-id')

      const result = await transcribeAudioStream('base64data', 'gpt-4o-mini-transcribe', 'en')

      expect(mockInvokeCommand).toHaveBeenCalledWith(
        'transcribeAudioStream',
        'base64data',
        'gpt-4o-mini-transcribe',
        'en',
      )
      expect(result).toBe('abc-123-transcribe-id')
    })

    it('should pass null for optional params when not provided', async () => {
      const mockInvokeCommand = vi.mocked(invokeCommand)
      mockInvokeCommand.mockResolvedValue('some-id')

      await transcribeAudioStream('base64data')

      expect(mockInvokeCommand).toHaveBeenCalledWith(
        'transcribeAudioStream',
        'base64data',
        null,
        null,
      )
    })
  })

  describe('saveVoiceMessage', () => {
    it('should call save_voice_message and return file path', async () => {
      const mockInvokeCommand = vi.mocked(invokeCommand)
      mockInvokeCommand.mockResolvedValue('/home/user/.restflow/media/voice-abc123.webm')

      const result = await saveVoiceMessage('base64data')

      expect(mockInvokeCommand).toHaveBeenCalledWith('saveVoiceMessage', 'base64data', null)
      expect(result).toBe('/home/user/.restflow/media/voice-abc123.webm')
    })
  })
})
