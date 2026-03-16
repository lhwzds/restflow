import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { getVoiceModel, setVoiceModel, useVoiceRecorder } from '../useVoiceRecorder'
import { saveVoiceMessage, transcribeAudio } from '@/api/voice'

declare const global: typeof globalThis

vi.mock('@/api/voice', () => ({
  saveVoiceMessage: vi.fn().mockResolvedValue('/tmp/voice.webm'),
  transcribeAudio: vi.fn().mockResolvedValue({ text: 'hello world', model: 'whisper-1' }),
}))

class MockMediaRecorder {
  static isTypeSupported = vi.fn().mockReturnValue(true)
  ondataavailable: ((event: { data: Blob }) => void) | null = null
  onstop: (() => void) | null = null
  mimeType = 'audio/webm'
  state = 'recording'

  constructor(_stream: MediaStream, _options?: unknown) {}

  start = vi.fn()
  stop = vi.fn().mockImplementation(() => {
    this.ondataavailable?.({ data: new Blob(['audio'], { type: 'audio/webm' }) })
    this.state = 'inactive'
    this.onstop?.()
  })
}

class MockMediaStream {
  getTracks = vi.fn().mockReturnValue([{ stop: vi.fn() }])
}

async function flushPromises(turns = 6): Promise<void> {
  for (let index = 0; index < turns; index += 1) {
    await Promise.resolve()
  }
}

describe('useVoiceRecorder', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()

    Object.defineProperty(global, 'MediaRecorder', {
      value: MockMediaRecorder,
      writable: true,
      configurable: true,
    })

    Object.defineProperty(global.navigator, 'mediaDevices', {
      value: {
        getUserMedia: vi.fn().mockResolvedValue(new MockMediaStream()),
      },
      writable: true,
      configurable: true,
    })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('reports support when media recording APIs are available', () => {
    const { isSupported } = useVoiceRecorder()
    expect(isSupported.value).toBe(true)
  })

  it('transcribes voice-to-text recordings through the daemon HTTP API', async () => {
    const onTranscribed = vi.fn()
    const onTranscribeDelta = vi.fn()
    const { state, startRecording, stopRecording } = useVoiceRecorder({
      onTranscribed,
      onTranscribeDelta,
    })

    await startRecording('voice-to-text')
    stopRecording()
    await flushPromises()

    expect(transcribeAudio).toHaveBeenCalled()
    expect(onTranscribeDelta).toHaveBeenCalledWith('hello world', 'hello world')
    expect(onTranscribed).toHaveBeenCalledWith('hello world')
    expect(state.value.isTranscribing).toBe(false)
    expect(state.value.mode).toBeNull()
  })

  it('saves voice-message recordings through the daemon HTTP API', async () => {
    const onVoiceMessage = vi.fn()
    const { startRecording, stopRecording } = useVoiceRecorder({
      onVoiceMessage,
      getSessionId: () => 'session-1',
    })

    await startRecording('voice-message')
    stopRecording()
    await flushPromises()

    expect(saveVoiceMessage).toHaveBeenCalledWith(expect.any(String), 'session-1')
    expect(onVoiceMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        filePath: '/tmp/voice.webm',
        durationSec: 0,
      }),
    )
  })

  it('handles permission errors', async () => {
    const mediaDevices = navigator.mediaDevices as unknown as { getUserMedia: ReturnType<typeof vi.fn> }
    mediaDevices.getUserMedia = vi.fn().mockRejectedValue(new DOMException('Permission denied'))

    const { state, startRecording } = useVoiceRecorder()

    await startRecording('voice-to-text')

    expect(state.value.error).toBe('mic_permission_denied')
    expect(state.value.isRecording).toBe(false)
  })

  it('cancels recording without uploading audio', async () => {
    const { state, startRecording, cancelRecording } = useVoiceRecorder()

    await startRecording('voice-to-text')
    cancelRecording()

    expect(state.value.isRecording).toBe(false)
    expect(transcribeAudio).not.toHaveBeenCalled()
    expect(saveVoiceMessage).not.toHaveBeenCalled()
  })
})

describe('voice model storage', () => {
  const STORAGE_KEY = 'restflow-voice-model'
  let store: Record<string, string>
  let mockStorage: Storage

  beforeEach(() => {
    store = {}
    mockStorage = {
      getItem: vi.fn((key: string) => store[key] ?? null),
      setItem: vi.fn((key: string, value: string) => {
        store[key] = value
      }),
      removeItem: vi.fn((key: string) => {
        delete store[key]
      }),
      clear: vi.fn(() => {
        store = {}
      }),
      key: vi.fn(() => null),
      length: 0,
    }
    vi.stubGlobal('localStorage', mockStorage)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('returns the default model when not set', () => {
    expect(getVoiceModel()).toBe('gpt-4o-mini-transcribe')
  })

  it('persists and retrieves the model via localStorage', () => {
    store[STORAGE_KEY] = 'whisper-1'
    expect(getVoiceModel()).toBe('whisper-1')
  })

  it('sets the model via setVoiceModel', () => {
    setVoiceModel('whisper-1')
    expect(store[STORAGE_KEY]).toBe('whisper-1')
  })
})
