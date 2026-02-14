/**
 * Background Agent Stream Composable
 *
 * Handles real-time streaming events and heartbeat for background agents.
 * Uses Tauri event listeners to receive task execution output and runner status.
 */

import { ref, computed, onUnmounted, type ComputedRef } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getBackgroundAgentStreamEventName, getHeartbeatEventName } from '@/api/background-agents'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import type { TaskStreamEvent } from '@/types/generated/TaskStreamEvent'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'
import type { HeartbeatEvent } from '@/types/generated/HeartbeatEvent'

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

/**
 * Composable for background agent streaming events
 *
 * @param trackTaskId - Getter for the task ID to filter stream events by
 */
export function useBackgroundAgentStream(trackTaskId: () => string | null) {
  const streamState = ref<StreamState>(createInitialStreamState())
  const heartbeatState = ref<HeartbeatState>({
    activeTasks: 0,
    pendingTasks: 0,
    uptimeMs: 0,
    lastPulse: null,
  })
  const outputLines = ref<string[]>([])

  let unlistenStream: UnlistenFn | null = null
  let unlistenHeartbeat: UnlistenFn | null = null
  let bridgeActivated = false

  const store = useBackgroundAgentStore()

  function handleStreamEvent(event: TaskStreamEvent): void {
    const taskId = trackTaskId()
    if (event.task_id !== taskId) return

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
        store.fetchAgents()
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
        store.fetchAgents()
        break

      case 'failed':
        streamState.value.isStreaming = false
        streamState.value.error = kind.error
        streamState.value.durationMs = kind.duration_ms
        streamState.value.phase = 'Failed'
        store.fetchAgents()
        break

      case 'cancelled':
        streamState.value.isStreaming = false
        streamState.value.phase = 'Cancelled'
        streamState.value.durationMs = kind.duration_ms
        store.fetchAgents()
        break

      case 'heartbeat':
        // inline heartbeat during execution — just update phase
        streamState.value.phase = `Running (${Math.round(kind.elapsed_ms / 1000)}s)`
        break
    }
  }

  function handleHeartbeatEvent(event: HeartbeatEvent): void {
    if (event.kind === 'pulse') {
      const pulse = event as Extract<HeartbeatEvent, { kind: 'pulse' }>
      heartbeatState.value = {
        activeTasks: pulse.active_tasks,
        pendingTasks: pulse.pending_tasks,
        uptimeMs: pulse.uptime_ms,
        lastPulse: pulse.timestamp,
      }
    }
  }

  async function setupListeners(): Promise<void> {
    // Activate the Rust bridge first (ensures events flow to frontend)
    if (!bridgeActivated) {
      await getBackgroundAgentStreamEventName()
      bridgeActivated = true
    }

    if (!unlistenStream) {
      unlistenStream = await listen<TaskStreamEvent>('background-agent:stream', (event) => {
        handleStreamEvent(event.payload)
      })
    }

    if (!unlistenHeartbeat) {
      const heartbeatChannel = await getHeartbeatEventName()
      unlistenHeartbeat = await listen<HeartbeatEvent>(heartbeatChannel, (event) => {
        handleHeartbeatEvent(event.payload)
      })
    }
  }

  function reset(): void {
    streamState.value = createInitialStreamState()
    outputLines.value = []
  }

  function cleanup(): void {
    if (unlistenStream) {
      unlistenStream()
      unlistenStream = null
    }
    if (unlistenHeartbeat) {
      unlistenHeartbeat()
      unlistenHeartbeat = null
    }
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

export type UseBackgroundAgentStreamReturn = ReturnType<typeof useBackgroundAgentStream>
