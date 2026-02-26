/**
 * Voice Recorder Composable
 *
 * Manages audio recording with two modes:
 * - Voice-to-text: Records audio → transcribes → returns text
 * - Voice message: Records audio → saves file → returns path
 *
 * Uses click-to-toggle interaction (not long-press).
 * Mode is chosen explicitly by the caller.
 */

import { ref, computed, readonly, onUnmounted, type Ref, type ComputedRef } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { transcribeAudioStream, saveVoiceMessage } from '@/api/voice'
import type { TranscribeStreamEvent } from '@/types/generated/TranscribeStreamEvent'

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
}

export interface VoiceRecorderReturn {
  state: Readonly<Ref<VoiceRecorderState>>
  mediaStream: Readonly<Ref<MediaStream | null>>
  startRecording: (mode: VoiceMode) => void
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
    // Ignore storage errors
  }
}

export function useVoiceRecorder(options: VoiceRecorderOptions = {}): VoiceRecorderReturn {
  const { model, language, onTranscribed, onTranscribeDelta, onVoiceMessage } = options

  const state = ref<VoiceRecorderState>({
    isRecording: false,
    isTranscribing: false,
    duration: 0,
    mode: null,
    error: null,
    streamingText: '',
  })

  const mediaStream = ref<MediaStream | null>(null)

  let mediaRecorder: MediaRecorder | null = null
  let audioChunks: Blob[] = []
  let durationTimer: ReturnType<typeof setInterval> | null = null
  let stream: MediaStream | null = null
  let streamUnlisten: UnlistenFn | null = null

  // Always show the mic button; check actual support at recording time.
  // Some webviews (e.g. Tauri) may lazily initialize mediaDevices.
  const isSupported = computed(
    () => typeof navigator !== 'undefined' && typeof MediaRecorder !== 'undefined',
  )

  function clearTimers() {
    if (durationTimer) {
      clearInterval(durationTimer)
      durationTimer = null
    }
  }

  function stopStream() {
    if (stream) {
      stream.getTracks().forEach((t) => t.stop())
      stream = null
    }
    mediaStream.value = null
  }

  function cleanupStreamListener() {
    if (streamUnlisten) {
      streamUnlisten()
      streamUnlisten = null
    }
  }

  onUnmounted(() => {
    cleanupStreamListener()
  })

  async function startRecording(mode: VoiceMode) {
    if (state.value.isRecording || state.value.isTranscribing) return
    if (!mode) return

    state.value.error = null
    audioChunks = []

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

    // Determine supported mime type
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
      handleRecordingComplete()
    }

    mediaRecorder.start()

    state.value.isRecording = true
    state.value.duration = 0
    state.value.mode = mode

    // Duration counter
    durationTimer = setInterval(() => {
      state.value.duration++
    }, 1000)
  }

  function stopRecording() {
    if (!state.value.isRecording || !mediaRecorder) return

    clearTimers()
    mediaRecorder.stop()
  }

  function cancelRecording() {
    if (!state.value.isRecording || !mediaRecorder) return

    clearTimers()
    cleanupStreamListener()

    // Detach handler to prevent processing
    mediaRecorder.onstop = null
    mediaRecorder.stop()
    stopStream()

    state.value.isRecording = false
    state.value.mode = null
    state.value.duration = 0
    state.value.streamingText = ''
  }

  function toggleRecording(mode: VoiceMode) {
    if (state.value.isRecording) {
      stopRecording()
    } else {
      startRecording(mode)
    }
  }

  async function handleRecordingComplete() {
    const mode = state.value.mode
    state.value.isRecording = false
    stopStream()

    if (audioChunks.length === 0) {
      state.value.mode = null
      return
    }

    const blob = new Blob(audioChunks, {
      type: mediaRecorder?.mimeType || 'audio/webm',
    })
    audioChunks = []

    const base64 = await blobToBase64(blob)

    if (mode === 'voice-to-text') {
      state.value.isTranscribing = true
      state.value.streamingText = ''
      try {
        const effectiveModel = model || getVoiceModel()
        const transcribeId = await transcribeAudioStream(base64, effectiveModel, language)

        // Listen for streaming transcription events
        streamUnlisten = await listen<TranscribeStreamEvent>(
          'voice:transcribe-stream',
          (event) => {
            const data = event.payload
            if (data.transcribe_id !== transcribeId) return

            switch (data.kind.type) {
              case 'delta':
                state.value.streamingText += data.kind.text
                onTranscribeDelta?.(data.kind.text, state.value.streamingText)
                break
              case 'completed':
                onTranscribed?.(data.kind.full_text)
                cleanupStreamListener()
                state.value.isTranscribing = false
                state.value.mode = null
                break
              case 'failed':
                state.value.error = data.kind.error
                cleanupStreamListener()
                state.value.isTranscribing = false
                state.value.mode = null
                break
            }
          },
        )
      } catch (err) {
        state.value.error = err instanceof Error ? err.message : 'transcription_failed'
        state.value.isTranscribing = false
        state.value.mode = null
      }
    } else if (mode === 'voice-message') {
      state.value.isTranscribing = true
      try {
        const filePath = await saveVoiceMessage(base64)
        const audioBlobUrl = URL.createObjectURL(blob)
        onVoiceMessage?.({ filePath, audioBlobUrl, durationSec: state.value.duration })
      } catch (err) {
        state.value.error = err instanceof Error ? err.message : 'save_failed'
      } finally {
        state.value.isTranscribing = false
        state.value.mode = null
      }
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

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onloadend = () => {
      const result = reader.result as string
      // Strip data URL prefix (e.g. "data:audio/webm;base64,")
      const base64 = result.split(',')[1] || ''
      resolve(base64)
    }
    reader.onerror = reject
    reader.readAsDataURL(blob)
  })
}
