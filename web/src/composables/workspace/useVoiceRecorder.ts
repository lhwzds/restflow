/**
 * Voice Recorder Composable
 *
 * Uses browser media APIs and daemon HTTP endpoints.
 */

import { ref, computed, readonly, getCurrentInstance, onUnmounted, type Ref, type ComputedRef } from 'vue'
import { saveVoiceMessage, transcribeAudio } from '@/api/voice'

export type VoiceMode = 'voice-to-text' | 'voice-message' | null

export interface VoiceRecorderState {
  isRecording: boolean
  isTranscribing: boolean
  duration: number
  mode: VoiceMode
  error: string | null
  streamingText: string
}

export interface VoiceMessageInfo {
  filePath: string
  audioBlobUrl: string
  durationSec: number
}

export interface VoiceRecorderOptions {
  model?: string
  language?: string
  onTranscribed?: (text: string) => void
  onTranscribeDelta?: (delta: string, accumulated: string) => void
  onVoiceMessage?: (info: VoiceMessageInfo) => void
  getSessionId?: () => string | undefined
}

export interface VoiceRecorderReturn {
  state: Readonly<Ref<VoiceRecorderState>>
  mediaStream: Readonly<Ref<MediaStream | null>>
  startRecording: (mode: VoiceMode) => Promise<void>
  stopRecording: () => void
  cancelRecording: () => void
  toggleRecording: (mode: VoiceMode) => void
  isSupported: ComputedRef<boolean>
}

const STORAGE_KEY_MODEL = 'restflow-voice-model'
const DEFAULT_MODEL = 'gpt-4o-mini-transcribe'

export function getVoiceModel(): string {
  try {
    return localStorage.getItem(STORAGE_KEY_MODEL) || DEFAULT_MODEL
  } catch {
    return DEFAULT_MODEL
  }
}

export function setVoiceModel(model: string): void {
  try {
    localStorage.setItem(STORAGE_KEY_MODEL, model)
  } catch {
    // Ignore storage errors.
  }
}

export function useVoiceRecorder(options: VoiceRecorderOptions = {}): VoiceRecorderReturn {
  const { model, language, onTranscribed, onTranscribeDelta, onVoiceMessage, getSessionId } =
    options

  const state = ref<VoiceRecorderState>({
    isRecording: false,
    isTranscribing: false,
    duration: 0,
    mode: null,
    error: null,
    streamingText: '',
  })

  const mediaStream = ref<MediaStream | null>(null)

  let durationTimer: ReturnType<typeof setInterval> | null = null
  let stream: MediaStream | null = null
  let mediaRecorder: MediaRecorder | null = null
  let audioChunks: Blob[] = []

  const isSupported = computed(
    () => typeof navigator !== 'undefined' && typeof MediaRecorder !== 'undefined',
  )

  function clearTimers(): void {
    if (durationTimer) {
      clearInterval(durationTimer)
      durationTimer = null
    }
  }

  function stopStream(): void {
    if (stream) {
      stream.getTracks().forEach((track) => track.stop())
      stream = null
    }
    mediaStream.value = null
  }

  function resetState(): void {
    state.value.isRecording = false
    state.value.isTranscribing = false
    state.value.mode = null
    state.value.duration = 0
    state.value.streamingText = ''
  }

  if (getCurrentInstance()) {
    onUnmounted(() => {
      clearTimers()
      if (mediaRecorder && mediaRecorder.state !== 'inactive') {
        mediaRecorder.stop()
      }
      stopStream()
    })
  }

  async function startRecording(mode: VoiceMode): Promise<void> {
    if (state.value.isRecording || state.value.isTranscribing) return
    if (!mode) return

    state.value.error = null

    if (!navigator.mediaDevices?.getUserMedia) {
      state.value.error = 'mic_not_available'
      return
    }

    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true })
      mediaStream.value = stream
    } catch {
      state.value.error = 'mic_permission_denied'
      return
    }

    audioChunks = []

    const mimeType = MediaRecorder.isTypeSupported('audio/webm')
      ? 'audio/webm'
      : MediaRecorder.isTypeSupported('audio/mp4')
        ? 'audio/mp4'
        : undefined

    mediaRecorder = new MediaRecorder(stream, mimeType ? { mimeType } : undefined)
    mediaRecorder.ondataavailable = (event) => {
      if (event.data.size > 0) {
        audioChunks.push(event.data)
      }
    }
    mediaRecorder.onstop = () => {
      void finalizeRecording(mode)
    }
    mediaRecorder.start()

    state.value.isRecording = true
    state.value.duration = 0
    state.value.mode = mode
    state.value.streamingText = ''

    durationTimer = setInterval(() => {
      state.value.duration += 1
    }, 1000)
  }

  async function finalizeRecording(mode: Exclude<VoiceMode, null>): Promise<void> {
    state.value.isRecording = false
    clearTimers()
    stopStream()

    if (audioChunks.length === 0) {
      state.value.mode = null
      return
    }

    const durationSec = state.value.duration
    const blob = new Blob(audioChunks, { type: mediaRecorder?.mimeType || 'audio/webm' })
    audioChunks = []
    const base64 = await blobToBase64(blob)

    state.value.isTranscribing = true
    try {
      if (mode === 'voice-to-text') {
        const result = await transcribeAudio(base64, model || getVoiceModel(), language)
        state.value.streamingText = result.text
        onTranscribeDelta?.(result.text, result.text)
        onTranscribed?.(result.text)
      } else {
        const filePath = await saveVoiceMessage(base64, getSessionId?.())
        const audioBlobUrl = URL.createObjectURL(blob)
        onVoiceMessage?.({ filePath, audioBlobUrl, durationSec })
      }
    } catch (error) {
      state.value.error = error instanceof Error ? error.message : 'transcription_failed'
    } finally {
      state.value.isTranscribing = false
      state.value.mode = null
    }
  }

  function stopRecording(): void {
    if (!state.value.isRecording || !mediaRecorder) return
    mediaRecorder.stop()
  }

  function cancelRecording(): void {
    if (!state.value.isRecording || !mediaRecorder) return

    clearTimers()
    mediaRecorder.onstop = null
    mediaRecorder.stop()
    audioChunks = []
    stopStream()
    resetState()
  }

  function toggleRecording(mode: VoiceMode): void {
    if (state.value.isRecording) {
      stopRecording()
    } else {
      void startRecording(mode)
    }
  }

  return {
    state: readonly(state) as Readonly<Ref<VoiceRecorderState>>,
    mediaStream: readonly(mediaStream) as Readonly<Ref<MediaStream | null>>,
    startRecording,
    stopRecording,
    cancelRecording,
    toggleRecording,
    isSupported,
  }
}

async function blobToBase64(blob: Blob): Promise<string> {
  const buffer = await blob.arrayBuffer()
  const bytes = new Uint8Array(buffer)
  let binary = ''
  for (const byte of bytes) {
    binary += String.fromCharCode(byte)
  }
  return btoa(binary)
}
