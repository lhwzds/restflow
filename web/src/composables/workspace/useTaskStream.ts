/**
 * Task Stream Composable
 *
 * Handles real-time streaming events for tasks over the daemon HTTP stream endpoint.
 */

import { ref, computed, onUnmounted, type ComputedRef } from 'vue'
import { useTaskStore } from '@/stores/taskStore'
import type { TaskStreamEvent } from '@/types/generated/TaskStreamEvent'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'
import { streamClient } from '@/api/http-client'

const MAX_OUTPUT_LINES = 5000

export interface StreamState {
  isStreaming: boolean
  phase: string | null
  progress: number | null
  error: string | null
  startedAt: number | null
  completedAt: number | null
  durationMs: number | null
  result: string | null
}

export interface HeartbeatState {
  activeTasks: number
  pendingTasks: number
  uptimeMs: number
  lastPulse: number | null
}

function createInitialStreamState(): StreamState {
  return {
    isStreaming: false,
    phase: null,
    progress: null,
    error: null,
    startedAt: null,
    completedAt: null,
    durationMs: null,
    result: null,
  }
}

export function useTaskStream(trackTaskId: () => string | null) {
  const streamState = ref<StreamState>(createInitialStreamState())
  const heartbeatState = ref<HeartbeatState>({
    activeTasks: 0,
    pendingTasks: 0,
    uptimeMs: 0,
    lastPulse: null,
  })
  const outputLines = ref<string[]>([])

  let streamAbortController: AbortController | null = null
  const taskStore = useTaskStore()

  function handleStreamEvent(event: TaskStreamEvent): void {
    const taskId = trackTaskId()
    if (event.task_id !== taskId) return

    heartbeatState.value.lastPulse = event.timestamp

    const kind = event.kind as StreamEventKind
    if (!('type' in kind)) return

    switch (kind.type) {
      case 'started':
        streamState.value = {
          ...createInitialStreamState(),
          isStreaming: true,
          startedAt: event.timestamp,
          phase: 'Starting...',
        }
        outputLines.value = []
        void taskStore.fetchTasks()
        break

      case 'output':
        if (outputLines.value.length >= MAX_OUTPUT_LINES) {
          outputLines.value = outputLines.value.slice(
            outputLines.value.length - MAX_OUTPUT_LINES + 1,
          )
        }
        outputLines.value.push(kind.text)
        break

      case 'progress':
        streamState.value.phase = kind.phase
        streamState.value.progress = kind.percent ?? null
        break

      case 'completed':
        streamState.value.isStreaming = false
        streamState.value.completedAt = event.timestamp
        streamState.value.durationMs = kind.duration_ms
        streamState.value.result = kind.result
        streamState.value.phase = 'Completed'
        void taskStore.fetchTasks()
        break

      case 'failed':
        streamState.value.isStreaming = false
        streamState.value.error = kind.error
        streamState.value.durationMs = kind.duration_ms
        streamState.value.phase = 'Failed'
        void taskStore.fetchTasks()
        break

      case 'interrupted':
        streamState.value.isStreaming = false
        streamState.value.phase = 'Interrupted'
        streamState.value.durationMs = kind.duration_ms
        void taskStore.fetchTasks()
        break

      case 'heartbeat':
        streamState.value.phase = `Running (${Math.round(kind.elapsed_ms / 1000)}s)`
        break
    }
  }

  async function setupListeners(): Promise<void> {
    const taskId = trackTaskId()
    if (!taskId || streamAbortController) return

    streamAbortController = new AbortController()

    try {
      for await (const frame of streamClient(
        {
          type: 'SubscribeTaskEvents',
          data: { task_id: taskId },
        },
        { signal: streamAbortController.signal },
      )) {
        if (
          frame.stream_type === 'event' &&
          'background_agent' in frame.data.event &&
          frame.data.event.background_agent
        ) {
          handleStreamEvent(frame.data.event.background_agent)
          continue
        }

        if (frame.stream_type === 'error') {
          streamState.value.error = frame.data.message
          streamState.value.isStreaming = false
          break
        }
      }
    } catch (error) {
      if (streamAbortController?.signal.aborted) {
        return
      }
      streamState.value.error = error instanceof Error ? error.message : 'Task stream failed'
      streamState.value.isStreaming = false
    } finally {
      streamAbortController = null
    }
  }

  function reset(): void {
    streamState.value = createInitialStreamState()
    outputLines.value = []
  }

  function cleanup(): void {
    streamAbortController?.abort()
    streamAbortController = null
  }

  const isStreaming: ComputedRef<boolean> = computed(() => streamState.value.isStreaming)
  const outputText: ComputedRef<string> = computed(() => outputLines.value.join(''))

  onUnmounted(cleanup)

  return {
    streamState,
    heartbeatState,
    isStreaming,
    outputText,
    outputLines,
    setupListeners,
    reset,
    cleanup,
  }
}

export type UseTaskStreamReturn = ReturnType<typeof useTaskStream>
