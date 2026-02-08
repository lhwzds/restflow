/**
 * Composable for handling real-time background agent stream events.
 *
 * Provides reactive state management for background agent execution streaming,
 * including output buffering, status tracking, and event history.
 */

import { ref, computed, onUnmounted, type Ref } from 'vue'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'
import type { ExecutionStats } from '@/types/generated/ExecutionStats'
import type { BackgroundAgentStreamEvent } from '@/types/background-agent'
import {
  onBackgroundAgentStreamEvent,
  onBackgroundAgentStreamEventForBackgroundAgent,
  runBackgroundAgentStreaming,
  cancelBackgroundAgent,
  getActiveBackgroundAgents,
  isEventKind,
} from '@/api/background-agent'

/**
 * Execution state for a single background agent run.
 */
export interface BackgroundAgentExecutionState {
  /** Background agent ID */
  backgroundAgentId: string
  /** Background agent name (from started event) */
  backgroundAgentName: string | null
  /** Agent ID being executed */
  agentId: string | null
  /** Execution mode (api, cli:claude, etc.) */
  executionMode: string | null
  /** Current status */
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'
  /** Accumulated stdout output */
  stdout: string
  /** Accumulated stderr output */
  stderr: string
  /** All output lines (combined, in order) */
  outputLines: OutputLine[]
  /** Current progress phase */
  progressPhase: string | null
  /** Progress percentage (0-100) */
  progressPercent: number | null
  /** Final result (on completion) */
  result: string | null
  /** Error message (on failure) */
  error: string | null
  /** Execution statistics */
  stats: ExecutionStats | null
  /** Duration in milliseconds */
  durationMs: number
  /** Start timestamp */
  startedAt: number | null
  /** Last heartbeat timestamp */
  lastHeartbeat: number | null
  /** All events received (for debugging/history) */
  events: BackgroundAgentStreamEvent[]
}

/**
 * A single output line with metadata
 */
export interface OutputLine {
  text: string
  isStderr: boolean
  timestamp: number
}

/**
 * Create initial execution state for a background agent.
 */
function createInitialState(backgroundAgentId: string): BackgroundAgentExecutionState {
  return {
    backgroundAgentId,
    backgroundAgentName: null,
    agentId: null,
    executionMode: null,
    status: 'pending',
    stdout: '',
    stderr: '',
    outputLines: [],
    progressPhase: null,
    progressPercent: null,
    result: null,
    error: null,
    stats: null,
    durationMs: 0,
    startedAt: null,
    lastHeartbeat: null,
    events: [],
  }
}

/**
 * Options for useBackgroundAgentStreamEvents composable
 */
export interface UseBackgroundAgentStreamEventsOptions {
  /** Maximum number of output lines to keep (default: 10000) */
  maxOutputLines?: number
  /** Maximum events to keep in history (default: 1000) */
  maxEvents?: number
  /** Whether to auto-scroll output (for UI hints) */
  autoScroll?: boolean
}

/**
 * Composable for managing stream events for a single background agent.
 *
 * @param backgroundAgentId - The background agent ID to monitor
 * @param options - Configuration options
 */
export function useBackgroundAgentStreamEvents(
  backgroundAgentId: Ref<string | null>,
  options: UseBackgroundAgentStreamEventsOptions = {},
) {
  const { maxOutputLines = 10000, maxEvents = 1000 } = options

  const state = ref<BackgroundAgentExecutionState | null>(null)
  const isListening = ref(false)
  let unlistenFn: (() => void) | null = null

  /**
   * Process an incoming stream event
   */
  function handleEvent(event: BackgroundAgentStreamEvent) {
    if (!state.value || state.value.backgroundAgentId !== event.task_id) {
      // Initialize state if needed
      state.value = createInitialState(event.task_id)
    }

    // Store event in history
    state.value.events.push(event)
    if (state.value.events.length > maxEvents) {
      state.value.events = state.value.events.slice(-maxEvents)
    }

    // Process based on event kind
    const kind = event.kind

    switch (kind.type) {
      case 'started':
        state.value.status = 'running'
        state.value.backgroundAgentName = kind.task_name
        state.value.agentId = kind.agent_id
        state.value.executionMode = kind.execution_mode
        state.value.startedAt = event.timestamp
        break

      case 'output':
        processOutput(kind, event.timestamp)
        break

      case 'progress':
        state.value.progressPhase = kind.phase
        state.value.progressPercent = kind.percent
        break

      case 'completed':
        state.value.status = 'completed'
        state.value.result = kind.result
        state.value.durationMs = kind.duration_ms
        state.value.stats = kind.stats
        break

      case 'failed':
        state.value.status = 'failed'
        state.value.error = kind.error
        state.value.durationMs = kind.duration_ms
        break

      case 'cancelled':
        state.value.status = 'cancelled'
        state.value.error = kind.reason
        state.value.durationMs = kind.duration_ms
        break

      case 'heartbeat':
        state.value.lastHeartbeat = event.timestamp
        state.value.durationMs = kind.elapsed_ms
        break
    }
  }

  /**
   * Process output event
   */
  function processOutput(kind: Extract<StreamEventKind, { type: 'output' }>, timestamp: number) {
    if (!state.value) return

    const line: OutputLine = {
      text: kind.text,
      isStderr: kind.is_stderr,
      timestamp,
    }

    // Add to appropriate buffer
    if (kind.is_stderr) {
      state.value.stderr += kind.text
    } else {
      state.value.stdout += kind.text
    }

    // Add to output lines
    state.value.outputLines.push(line)
    if (state.value.outputLines.length > maxOutputLines) {
      state.value.outputLines = state.value.outputLines.slice(-maxOutputLines)
    }
  }

  /**
   * Start listening for events
   */
  async function startListening() {
    if (isListening.value || !backgroundAgentId.value) return

    // Initialize state
    state.value = createInitialState(backgroundAgentId.value)
    isListening.value = true

    unlistenFn = await onBackgroundAgentStreamEventForBackgroundAgent(
      backgroundAgentId.value,
      handleEvent,
    )
  }

  /**
   * Stop listening for events
   */
  function stopListening() {
    if (unlistenFn) {
      unlistenFn()
      unlistenFn = null
    }
    isListening.value = false
  }

  /**
   * Clear state and stop listening
   */
  function reset() {
    stopListening()
    state.value = null
  }

  /**
   * Run the background agent and start listening
   */
  async function runBackgroundAgent() {
    if (!backgroundAgentId.value) {
      throw new Error('No background agent ID specified')
    }

    await startListening()
    await runBackgroundAgentStreaming(backgroundAgentId.value)
  }

  /**
   * Cancel the running background agent
   */
  async function cancel() {
    if (!backgroundAgentId.value || state.value?.status !== 'running') {
      return false
    }
    return cancelBackgroundAgent(backgroundAgentId.value)
  }

  // Computed properties
  const isRunning = computed(() => state.value?.status === 'running')
  const isCompleted = computed(() => state.value?.status === 'completed')
  const isFailed = computed(() => state.value?.status === 'failed')
  const isCancelled = computed(() => state.value?.status === 'cancelled')
  const isFinished = computed(() =>
    ['completed', 'failed', 'cancelled'].includes(state.value?.status || ''),
  )

  const combinedOutput = computed(() => {
    if (!state.value) return ''
    return state.value.outputLines.map((l) => l.text).join('')
  })

  const outputLineCount = computed(() => state.value?.outputLines.length ?? 0)

  // Cleanup on unmount
  onUnmounted(() => {
    stopListening()
  })

  return {
    // State
    state,
    isListening,

    // Computed
    isRunning,
    isCompleted,
    isFailed,
    isCancelled,
    isFinished,
    combinedOutput,
    outputLineCount,

    // Actions
    startListening,
    stopListening,
    reset,
    runBackgroundAgent,
    cancel,
  }
}

/**
 * Composable for managing multiple concurrent background agent executions
 *
 * Use this when you need to monitor multiple background agents at once,
 * such as in a dashboard or background agent list view.
 */
export function useMultiBackgroundAgentStreamEvents(
  options: UseBackgroundAgentStreamEventsOptions = {},
) {
  const { maxOutputLines = 5000, maxEvents = 500 } = options

  const backgroundAgents = ref<Map<string, BackgroundAgentExecutionState>>(new Map())
  const isListening = ref(false)
  let unlistenFn: (() => void) | null = null

  /**
   * Process an incoming stream event
   */
  function handleEvent(event: BackgroundAgentStreamEvent) {
    let state = backgroundAgents.value.get(event.task_id)

    if (!state) {
      state = createInitialState(event.task_id)
      backgroundAgents.value.set(event.task_id, state)
    }

    // Store event
    state.events.push(event)
    if (state.events.length > maxEvents) {
      state.events = state.events.slice(-maxEvents)
    }

    // Process event kind
    const kind = event.kind

    switch (kind.type) {
      case 'started':
        state.status = 'running'
        state.backgroundAgentName = kind.task_name
        state.agentId = kind.agent_id
        state.executionMode = kind.execution_mode
        state.startedAt = event.timestamp
        break

      case 'output': {
        const line: OutputLine = {
          text: kind.text,
          isStderr: kind.is_stderr,
          timestamp: event.timestamp,
        }
        if (kind.is_stderr) {
          state.stderr += kind.text
        } else {
          state.stdout += kind.text
        }
        state.outputLines.push(line)
        if (state.outputLines.length > maxOutputLines) {
          state.outputLines = state.outputLines.slice(-maxOutputLines)
        }
        break
      }

      case 'progress':
        state.progressPhase = kind.phase
        state.progressPercent = kind.percent
        break

      case 'completed':
        state.status = 'completed'
        state.result = kind.result
        state.durationMs = kind.duration_ms
        state.stats = kind.stats
        break

      case 'failed':
        state.status = 'failed'
        state.error = kind.error
        state.durationMs = kind.duration_ms
        break

      case 'cancelled':
        state.status = 'cancelled'
        state.error = kind.reason
        state.durationMs = kind.duration_ms
        break

      case 'heartbeat':
        state.lastHeartbeat = event.timestamp
        state.durationMs = kind.elapsed_ms
        break
    }

    // Trigger reactivity
    backgroundAgents.value = new Map(backgroundAgents.value)
  }

  /**
   * Start listening for all background agent events
   */
  async function startListening() {
    if (isListening.value) return

    isListening.value = true
    unlistenFn = await onBackgroundAgentStreamEvent(handleEvent)

    // Load currently active background agents
    const activeBackgroundAgents = await getActiveBackgroundAgents()
    for (const activeBackgroundAgent of activeBackgroundAgents) {
      if (!backgroundAgents.value.has(activeBackgroundAgent.background_agent_id)) {
        const state = createInitialState(activeBackgroundAgent.background_agent_id)
        state.status = 'running'
        state.backgroundAgentName = activeBackgroundAgent.background_agent_name
        state.agentId = activeBackgroundAgent.executor_agent_id
        state.executionMode = activeBackgroundAgent.execution_mode
        state.startedAt = activeBackgroundAgent.started_at
        backgroundAgents.value.set(activeBackgroundAgent.background_agent_id, state)
      }
    }
  }

  /**
   * Stop listening for events
   */
  function stopListening() {
    if (unlistenFn) {
      unlistenFn()
      unlistenFn = null
    }
    isListening.value = false
  }

  /**
   * Get state for a specific background agent
   */
  function getBackgroundAgentState(
    backgroundAgentId: string,
  ): BackgroundAgentExecutionState | undefined {
    return backgroundAgents.value.get(backgroundAgentId)
  }

  /**
   * Remove a background agent from tracking (e.g., after it's finished and dismissed)
   */
  function removeBackgroundAgent(backgroundAgentId: string) {
    backgroundAgents.value.delete(backgroundAgentId)
    backgroundAgents.value = new Map(backgroundAgents.value)
  }

  /**
   * Clear all finished background agents
   */
  function clearFinished() {
    for (const [backgroundAgentId, state] of backgroundAgents.value) {
      if (['completed', 'failed', 'cancelled'].includes(state.status)) {
        backgroundAgents.value.delete(backgroundAgentId)
      }
    }
    backgroundAgents.value = new Map(backgroundAgents.value)
  }

  // Computed properties
  const runningBackgroundAgents = computed(() =>
    Array.from(backgroundAgents.value.values()).filter(
      (backgroundAgent) => backgroundAgent.status === 'running',
    ),
  )

  const finishedBackgroundAgents = computed(() =>
    Array.from(backgroundAgents.value.values()).filter((backgroundAgent) =>
      ['completed', 'failed', 'cancelled'].includes(backgroundAgent.status),
    ),
  )

  const backgroundAgentCount = computed(() => backgroundAgents.value.size)
  const runningCount = computed(() => runningBackgroundAgents.value.length)

  // Cleanup on unmount
  onUnmounted(() => {
    stopListening()
  })

  return {
    // State
    backgroundAgents,
    isListening,

    // Computed
    runningBackgroundAgents,
    finishedBackgroundAgents,
    backgroundAgentCount,
    runningCount,

    // Actions
    startListening,
    stopListening,
    getBackgroundAgentState,
    removeBackgroundAgent,
    clearFinished,
  }
}

// Re-export utility
export { isEventKind }
