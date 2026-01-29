/**
 * Composable for handling real-time task stream events
 *
 * Provides reactive state management for task execution streaming,
 * including output buffering, status tracking, and event history.
 */

import { ref, computed, onUnmounted, type Ref, type ComputedRef } from 'vue'
import type { TaskStreamEvent } from '@/types/generated/TaskStreamEvent'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'
import type { ExecutionStats } from '@/types/generated/ExecutionStats'
import {
  onTaskStreamEvent,
  onTaskStreamEventForTask,
  runAgentTaskStreaming,
  cancelAgentTask,
  getActiveAgentTasks,
  isEventKind,
} from '@/api/agent-task'

/**
 * Execution state for a single task
 */
export interface TaskExecutionState {
  /** Task ID */
  taskId: string
  /** Task name (from started event) */
  taskName: string | null
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
  events: TaskStreamEvent[]
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
 * Create initial execution state for a task
 */
function createInitialState(taskId: string): TaskExecutionState {
  return {
    taskId,
    taskName: null,
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
 * Options for useTaskStreamEvents composable
 */
export interface UseTaskStreamEventsOptions {
  /** Maximum number of output lines to keep (default: 10000) */
  maxOutputLines?: number
  /** Maximum events to keep in history (default: 1000) */
  maxEvents?: number
  /** Whether to auto-scroll output (for UI hints) */
  autoScroll?: boolean
}

/**
 * Composable for managing task stream events for a single task
 *
 * @param taskId - The task ID to monitor
 * @param options - Configuration options
 */
export function useTaskStreamEvents(
  taskId: Ref<string | null>,
  options: UseTaskStreamEventsOptions = {},
) {
  const { maxOutputLines = 10000, maxEvents = 1000 } = options

  const state = ref<TaskExecutionState | null>(null)
  const isListening = ref(false)
  let unlistenFn: (() => void) | null = null

  /**
   * Process an incoming stream event
   */
  function handleEvent(event: TaskStreamEvent) {
    if (!state.value || state.value.taskId !== event.task_id) {
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
        state.value.taskName = kind.task_name
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
  function processOutput(
    kind: Extract<StreamEventKind, { type: 'output' }>,
    timestamp: number,
  ) {
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
    if (isListening.value || !taskId.value) return

    // Initialize state
    state.value = createInitialState(taskId.value)
    isListening.value = true

    unlistenFn = await onTaskStreamEventForTask(taskId.value, handleEvent)
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
   * Run the task and start listening
   */
  async function runTask() {
    if (!taskId.value) {
      throw new Error('No task ID specified')
    }

    await startListening()
    await runAgentTaskStreaming(taskId.value)
  }

  /**
   * Cancel the running task
   */
  async function cancel() {
    if (!taskId.value || state.value?.status !== 'running') {
      return false
    }
    return cancelAgentTask(taskId.value)
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
    runTask,
    cancel,
  }
}

/**
 * Composable for managing multiple concurrent task executions
 *
 * Use this when you need to monitor multiple tasks at once,
 * such as in a dashboard or task list view.
 */
export function useMultiTaskStreamEvents(options: UseTaskStreamEventsOptions = {}) {
  const { maxOutputLines = 5000, maxEvents = 500 } = options

  const tasks = ref<Map<string, TaskExecutionState>>(new Map())
  const isListening = ref(false)
  let unlistenFn: (() => void) | null = null

  /**
   * Process an incoming stream event
   */
  function handleEvent(event: TaskStreamEvent) {
    let state = tasks.value.get(event.task_id)

    if (!state) {
      state = createInitialState(event.task_id)
      tasks.value.set(event.task_id, state)
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
        state.taskName = kind.task_name
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
    tasks.value = new Map(tasks.value)
  }

  /**
   * Start listening for all task events
   */
  async function startListening() {
    if (isListening.value) return

    isListening.value = true
    unlistenFn = await onTaskStreamEvent(handleEvent)

    // Load currently active tasks
    const activeIds = await getActiveAgentTasks()
    for (const taskId of activeIds) {
      if (!tasks.value.has(taskId)) {
        const state = createInitialState(taskId)
        state.status = 'running'
        tasks.value.set(taskId, state)
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
   * Get state for a specific task
   */
  function getTaskState(taskId: string): TaskExecutionState | undefined {
    return tasks.value.get(taskId)
  }

  /**
   * Remove a task from tracking (e.g., after it's finished and dismissed)
   */
  function removeTask(taskId: string) {
    tasks.value.delete(taskId)
    tasks.value = new Map(tasks.value)
  }

  /**
   * Clear all finished tasks
   */
  function clearFinished() {
    for (const [taskId, state] of tasks.value) {
      if (['completed', 'failed', 'cancelled'].includes(state.status)) {
        tasks.value.delete(taskId)
      }
    }
    tasks.value = new Map(tasks.value)
  }

  // Computed properties
  const runningTasks = computed(() =>
    Array.from(tasks.value.values()).filter((t) => t.status === 'running'),
  )

  const finishedTasks = computed(() =>
    Array.from(tasks.value.values()).filter((t) =>
      ['completed', 'failed', 'cancelled'].includes(t.status),
    ),
  )

  const taskCount = computed(() => tasks.value.size)
  const runningCount = computed(() => runningTasks.value.length)

  // Cleanup on unmount
  onUnmounted(() => {
    stopListening()
  })

  return {
    // State
    tasks,
    isListening,

    // Computed
    runningTasks,
    finishedTasks,
    taskCount,
    runningCount,

    // Actions
    startListening,
    stopListening,
    getTaskState,
    removeTask,
    clearFinished,
  }
}

// Re-export utility
export { isEventKind }
