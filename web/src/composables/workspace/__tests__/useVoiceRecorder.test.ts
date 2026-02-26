import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { useVoiceRecorder, getVoiceModel, setVoiceModel } from '../useVoiceRecorder'

// Mock voice API
vi.mock('@/api/voice', () => ({
  transcribeAudio: vi.fn(),
  saveVoiceMessage: vi.fn(),
}))

// Mock MediaRecorder
class MockMediaRecorder {
  static isTypeSupported = vi.fn().mockReturnValue(true)
  ondataavailable: ((event: { data: Blob }) => void) | null = null
  onstop: (() => void) | null = null
  mimeType = 'audio/webm'

  start = vi.fn()
  stop = vi.fn().mockImplementation(() => {
    this.onstop?.()
  })
}

// Mock MediaStream
class MockMediaStream {
  getTracks = vi.fn().mockReturnValue([{ stop: vi.fn() }])
}

describe('useVoiceRecorder', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()

    // Set up global mocks
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

  it('should report isSupported when mediaDevices is available', () => {
    const { isSupported } = useVoiceRecorder()
    expect(isSupported.value).toBe(true)
  })

  it('should start in idle state', () => {
    const { state } = useVoiceRecorder()
    expect(state.value.isRecording).toBe(false)
    expect(state.value.isTranscribing).toBe(false)
    expect(state.value.duration).toBe(0)
    expect(state.value.mode).toBeNull()
    expect(state.value.error).toBeNull()
  })

  it('should start recording with explicit voice-to-text mode', async () => {
    const { state, startRecording } = useVoiceRecorder()

    await startRecording('voice-to-text')

    expect(state.value.isRecording).toBe(true)
    expect(state.value.mode).toBe('voice-to-text')
  })

  it('should start recording with explicit voice-message mode', async () => {
    const { state, startRecording } = useVoiceRecorder()

    await startRecording('voice-message')

    expect(state.value.isRecording).toBe(true)
    expect(state.value.mode).toBe('voice-message')
  })

  it('should not start recording with null mode', async () => {
    const { state, startRecording } = useVoiceRecorder()

    await startRecording(null)

    expect(state.value.isRecording).toBe(false)
  })

  it('should toggle recording on and off', async () => {
    const { state, toggleRecording } = useVoiceRecorder()

    // Toggle on
    await toggleRecording('voice-to-text')
    expect(state.value.isRecording).toBe(true)
    expect(state.value.mode).toBe('voice-to-text')

    // Toggle off (stops recording)
    toggleRecording('voice-to-text')
    // After stop, isRecording becomes false via onstop handler
    expect(state.value.isRecording).toBe(false)
  })

  it('should increment duration every second', async () => {
    const { state, startRecording } = useVoiceRecorder()

    await startRecording('voice-to-text')
    expect(state.value.duration).toBe(0)

    vi.advanceTimersByTime(1000)
    expect(state.value.duration).toBe(1)

    vi.advanceTimersByTime(2000)
    expect(state.value.duration).toBe(3)
  })

  it('should set error on mic permission denied', async () => {
    const mediaDevices = navigator.mediaDevices as unknown as { getUserMedia: ReturnType<typeof vi.fn> }
    mediaDevices.getUserMedia = vi.fn().mockRejectedValue(new DOMException('Permission denied'))

    const { state, startRecording } = useVoiceRecorder()

    await startRecording('voice-to-text')
    expect(state.value.error).toBe('mic_permission_denied')
    expect(state.value.isRecording).toBe(false)
  })

  it('should cancel recording without processing', async () => {
    const onTranscribed = vi.fn()
    const { state, startRecording, cancelRecording } = useVoiceRecorder({ onTranscribed })

    await startRecording('voice-to-text')
    expect(state.value.isRecording).toBe(true)

    cancelRecording()
    expect(state.value.isRecording).toBe(false)
    expect(state.value.mode).toBeNull()
    expect(onTranscribed).not.toHaveBeenCalled()
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
      setItem: vi.fn((key: string, value: string) => { store[key] = value }),
      removeItem: vi.fn((key: string) => { delete store[key] }),
      clear: vi.fn(() => { store = {} }),
      key: vi.fn(() => null),
      length: 0,
    }
    vi.stubGlobal('localStorage', mockStorage)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('should return default model when not set', () => {
    expect(getVoiceModel()).toBe('gpt-4o-mini-transcribe')
  })

  it('should persist and retrieve model via localStorage', () => {
    store[STORAGE_KEY] = 'whisper-1'
    expect(getVoiceModel()).toBe('whisper-1')
  })

  it('should set model via setVoiceModel', () => {
    setVoiceModel('whisper-1')
    expect(store[STORAGE_KEY]).toBe('whisper-1')
  })
})
