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
import {
  saveVoiceMessage,
  startLiveTranscription,
  sendLiveAudioChunk,
  stopLiveTranscription,
} from '@/api/voice'
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
  /** Optional getter for the current session ID. If provided, voice messages are saved directly to the session media directory. */
  getSessionId?: () => string | undefined
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

  // --- Shared state ---
  let durationTimer: ReturnType<typeof setInterval> | null = null
  let stream: MediaStream | null = null
  let streamUnlisten: UnlistenFn | null = null

  // --- MediaRecorder state (voice-message mode) ---
  let mediaRecorder: MediaRecorder | null = null
  let audioChunks: Blob[] = []

  // --- AudioWorklet state (voice-to-text live mode) ---
  let audioContext: AudioContext | null = null
  let workletNode: AudioWorkletNode | null = null
  let sourceNode: MediaStreamAudioSourceNode | null = null
  let liveTranscribeId: string | null = null

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

  function cleanupAudioWorklet() {
    if (workletNode) {
      workletNode.disconnect()
      workletNode = null
    }
    if (sourceNode) {
      sourceNode.disconnect()
      sourceNode = null
    }
    if (audioContext) {
      audioContext.close().catch(() => {})
      audioContext = null
    }
  }

  onUnmounted(() => {
    cleanupStreamListener()
    cleanupAudioWorklet()
  })

  async function startRecording(mode: VoiceMode) {
    if (state.value.isRecording || state.value.isTranscribing) return
    if (!mode) return

    state.value.error = null

    if (!navigator.mediaDevices?.getUserMedia) {
      state.value.error = 'mic_not_available'
      return
    }

    if (mode === 'voice-to-text') {
      await startLiveRecording()
    } else if (mode === 'voice-message') {
      await startMediaRecording()
    }
  }

  // ===== Live Transcription (voice-to-text) =====

  async function startLiveRecording() {
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: { channelCount: 1 },
      })
      mediaStream.value = stream
    } catch {
      state.value.error = 'mic_permission_denied'
      return
    }

    try {
      // Create AudioContext at default sample rate (browser decides)
      audioContext = new AudioContext()

      // Load the AudioWorklet processor
      await audioContext.audioWorklet.addModule('/pcm-processor.js')

      // Create source from microphone stream
      sourceNode = audioContext.createMediaStreamSource(stream)

      // Create worklet node
      workletNode = new AudioWorkletNode(audioContext, 'pcm-processor')

      // Start live transcription session on the backend
      const effectiveModel = model || getVoiceModel()
      liveTranscribeId = await startLiveTranscription(effectiveModel, language)

      // Listen for transcription events from the backend
      streamUnlisten = await listen<TranscribeStreamEvent>(
        'voice:transcribe-stream',
        (event) => {
          const data = event.payload
          if (data.transcribe_id !== liveTranscribeId) return

          switch (data.kind.type) {
            case 'delta':
              state.value.streamingText += data.kind.text
              onTranscribeDelta?.(data.kind.text, state.value.streamingText)
              break
            case 'segment_done':
              // Replace streamingText with the de-duplicated authoritative text.
              // This corrects any word duplication at VAD segment boundaries.
              state.value.streamingText = data.kind.corrected_text
              break
            case 'completed':
              onTranscribed?.(data.kind.full_text)
              cleanupStreamListener()
              state.value.isTranscribing = false
              state.value.mode = null
              liveTranscribeId = null
              break
            case 'failed':
              state.value.error = data.kind.error
              cleanupStreamListener()
              cleanupAudioWorklet()
              stopStream()
              state.value.isRecording = false
              state.value.isTranscribing = false
              state.value.mode = null
              liveTranscribeId = null
              break
          }
        },
      )

      // Forward PCM16 audio chunks from worklet to backend
      workletNode.port.onmessage = (event) => {
        if (event.data.type === 'audio' && liveTranscribeId) {
          const base64 = int16BufferToBase64(event.data.buffer)
          sendLiveAudioChunk(liveTranscribeId, base64).catch((err) => {
            console.warn('Failed to send audio chunk:', err)
          })
        }
      }

      // Connect: source → worklet (no need to connect to destination)
      sourceNode.connect(workletNode)

      state.value.isRecording = true
      state.value.isTranscribing = true
      state.value.duration = 0
      state.value.mode = 'voice-to-text'
      state.value.streamingText = ''

      durationTimer = setInterval(() => {
        state.value.duration++
      }, 1000)
    } catch (err) {
      cleanupAudioWorklet()
      stopStream()
      state.value.error = err instanceof Error ? err.message : 'live_transcription_failed'
    }
  }

  async function stopLiveRecording() {
    clearTimers()
    cleanupAudioWorklet()
    stopStream()

    state.value.isRecording = false
    // Keep isTranscribing = true until we get the Completed event

    if (liveTranscribeId) {
      try {
        await stopLiveTranscription(liveTranscribeId)
      } catch (err) {
        console.warn('Failed to stop live transcription:', err)
      }
    }
  }

  function cancelLiveRecording() {
    clearTimers()
    cleanupStreamListener()
    cleanupAudioWorklet()
    stopStream()

    if (liveTranscribeId) {
      stopLiveTranscription(liveTranscribeId).catch(() => {})
      liveTranscribeId = null
    }

    state.value.isRecording = false
    state.value.isTranscribing = false
    state.value.mode = null
    state.value.duration = 0
    state.value.streamingText = ''
  }

  // ===== MediaRecorder (voice-message mode) =====

  async function startMediaRecording() {
    audioChunks = []

    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true })
      mediaStream.value = stream
    } catch {
      state.value.error = 'mic_permission_denied'
      return
    }

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
      handleMediaRecordingComplete()
    }

    mediaRecorder.start()

    state.value.isRecording = true
    state.value.duration = 0
    state.value.mode = 'voice-message'

    durationTimer = setInterval(() => {
      state.value.duration++
    }, 1000)
  }

  function stopMediaRecording() {
    if (!mediaRecorder) return
    clearTimers()
    mediaRecorder.stop()
  }

  function cancelMediaRecording() {
    if (!mediaRecorder) return
    clearTimers()
    cleanupStreamListener()
    mediaRecorder.onstop = null
    mediaRecorder.stop()
    stopStream()

    state.value.isRecording = false
    state.value.mode = null
    state.value.duration = 0
    state.value.streamingText = ''
  }

  async function handleMediaRecordingComplete() {
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

    state.value.isTranscribing = true
    try {
      const filePath = await saveVoiceMessage(base64, getSessionId?.())
      const audioBlobUrl = URL.createObjectURL(blob)
      onVoiceMessage?.({ filePath, audioBlobUrl, durationSec: state.value.duration })
    } catch (err) {
      state.value.error = err instanceof Error ? err.message : 'save_failed'
    } finally {
      state.value.isTranscribing = false
      state.value.mode = null
    }
  }

  // ===== Unified controls =====

  function stopRecording() {
    if (!state.value.isRecording) return

    if (state.value.mode === 'voice-to-text') {
      stopLiveRecording()
    } else if (state.value.mode === 'voice-message') {
      stopMediaRecording()
    }
  }

  function cancelRecording() {
    if (!state.value.isRecording) return

    if (state.value.mode === 'voice-to-text') {
      cancelLiveRecording()
    } else if (state.value.mode === 'voice-message') {
      cancelMediaRecording()
    }
  }

  function toggleRecording(mode: VoiceMode) {
    if (state.value.isRecording) {
      stopRecording()
    } else {
      startRecording(mode)
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

/** Convert an Int16Array ArrayBuffer to a base64 string. */
function int16BufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer)
  let binary = ''
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]!)
  }
  return btoa(binary)
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
