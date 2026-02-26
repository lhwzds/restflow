/**
 * Voice Recorder Composable
 *
 * Manages audio recording with two modes:
 * - Voice-to-text (short click): Records audio → transcribes → returns text
 * - Voice message (long press): Records audio → saves file → returns path
 */

import { ref, computed, readonly, type Ref, type ComputedRef } from 'vue'
import { transcribeAudio, saveVoiceMessage } from '@/api/voice'

export type VoiceMode = 'voice-to-text' | 'voice-message' | null

export interface VoiceRecorderState {
  isRecording: boolean
  isTranscribing: boolean
  duration: number
  mode: VoiceMode
  error: string | null
}

export interface VoiceRecorderOptions {
  model?: string
  language?: string
  onTranscribed?: (text: string) => void
  onVoiceMessage?: (filePath: string) => void
  longPressThreshold?: number
}

export interface VoiceRecorderReturn {
  state: Readonly<Ref<VoiceRecorderState>>
  startRecording: () => void
  stopRecording: () => void
  cancelRecording: () => void
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
  const {
    model,
    language,
    onTranscribed,
    onVoiceMessage,
    longPressThreshold = 500,
  } = options

  const state = ref<VoiceRecorderState>({
    isRecording: false,
    isTranscribing: false,
    duration: 0,
    mode: null,
    error: null,
  })

  let mediaRecorder: MediaRecorder | null = null
  let audioChunks: Blob[] = []
  let durationTimer: ReturnType<typeof setInterval> | null = null
  let longPressTimer: ReturnType<typeof setTimeout> | null = null
  let stream: MediaStream | null = null

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
    if (longPressTimer) {
      clearTimeout(longPressTimer)
      longPressTimer = null
    }
  }

  function stopStream() {
    if (stream) {
      stream.getTracks().forEach((t) => t.stop())
      stream = null
    }
  }

  async function startRecording() {
    if (state.value.isRecording || state.value.isTranscribing) return

    state.value.error = null
    audioChunks = []

    if (!navigator.mediaDevices?.getUserMedia) {
      state.value.error = 'mic_not_available'
      return
    }

    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true })
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

    // Default to voice-to-text mode; upgrade to voice-message on long press
    state.value.isRecording = true
    state.value.duration = 0
    state.value.mode = 'voice-to-text'

    // Duration counter
    durationTimer = setInterval(() => {
      state.value.duration++
    }, 1000)

    // Long press detection
    longPressTimer = setTimeout(() => {
      if (state.value.isRecording) {
        state.value.mode = 'voice-message'
      }
    }, longPressThreshold)
  }

  function stopRecording() {
    if (!state.value.isRecording || !mediaRecorder) return

    clearTimers()
    mediaRecorder.stop()
  }

  function cancelRecording() {
    if (!state.value.isRecording || !mediaRecorder) return

    clearTimers()

    // Detach handler to prevent processing
    mediaRecorder.onstop = null
    mediaRecorder.stop()
    stopStream()

    state.value.isRecording = false
    state.value.mode = null
    state.value.duration = 0
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
      try {
        const effectiveModel = model || getVoiceModel()
        const result = await transcribeAudio(base64, effectiveModel, language)
        onTranscribed?.(result.text)
      } catch (err) {
        state.value.error = err instanceof Error ? err.message : 'transcription_failed'
      } finally {
        state.value.isTranscribing = false
        state.value.mode = null
      }
    } else if (mode === 'voice-message') {
      state.value.isTranscribing = true
      try {
        const filePath = await saveVoiceMessage(base64)
        onVoiceMessage?.(filePath)
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
    startRecording,
    stopRecording,
    cancelRecording,
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
