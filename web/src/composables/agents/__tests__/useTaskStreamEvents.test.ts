import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { ref, nextTick } from 'vue'
import type { TaskStreamEvent } from '@/types/generated/TaskStreamEvent'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'

// Mock Tauri API
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('@/api/agent-task', () => ({
  onTaskStreamEvent: vi.fn(),
  onTaskStreamEventForTask: vi.fn(),
  runAgentTaskStreaming: vi.fn(),
  cancelAgentTask: vi.fn(),
  getActiveAgentTasks: vi.fn(),
  isEventKind: vi.fn((event, type) => event.kind.type === type),
}))

// Helper to create mock events
function createEvent(taskId: string, kind: StreamEventKind): TaskStreamEvent {
  return {
    task_id: taskId,
    timestamp: Date.now(),
    kind,
  }
}

function createStartedEvent(taskId: string): TaskStreamEvent {
  return createEvent(taskId, {
    type: 'started',
    task_name: 'Test Task',
    agent_id: 'agent-1',
    execution_mode: 'cli:claude',
  })
}

function createOutputEvent(taskId: string, text: string, isStderr = false): TaskStreamEvent {
  return createEvent(taskId, {
    type: 'output',
    text,
    is_stderr: isStderr,
    is_complete: text.endsWith('\n'),
  })
}

function createProgressEvent(taskId: string, phase: string, percent: number | null): TaskStreamEvent {
  return createEvent(taskId, {
    type: 'progress',
    phase,
    percent,
    details: null,
  })
}

function createCompletedEvent(taskId: string, result: string, durationMs: number): TaskStreamEvent {
  return createEvent(taskId, {
    type: 'completed',
    result,
    duration_ms: durationMs,
    stats: null,
  })
}

function createFailedEvent(taskId: string, error: string, durationMs: number): TaskStreamEvent {
  return createEvent(taskId, {
    type: 'failed',
    error,
    error_code: null,
    duration_ms: durationMs,
    recoverable: false,
  })
}

function createCancelledEvent(taskId: string, reason: string, durationMs: number): TaskStreamEvent {
  return createEvent(taskId, {
    type: 'cancelled',
    reason,
    duration_ms: durationMs,
  })
}

function createHeartbeatEvent(taskId: string, elapsedMs: number): TaskStreamEvent {
  return createEvent(taskId, {
    type: 'heartbeat',
    elapsed_ms: elapsedMs,
  })
}

describe('useTaskStreamEvents', () => {
  let mockEventCallback: ((event: TaskStreamEvent) => void) | null = null
  let mockUnlisten: ReturnType<typeof vi.fn>

  beforeEach(async () => {
    vi.clearAllMocks()
    mockEventCallback = null
    mockUnlisten = vi.fn()

    const { onTaskStreamEventForTask, getActiveAgentTasks } = await import('@/api/agent-task')
    vi.mocked(onTaskStreamEventForTask).mockImplementation(async (_taskId, callback) => {
      mockEventCallback = callback
      return mockUnlisten as unknown as () => void
    })
    vi.mocked(getActiveAgentTasks).mockResolvedValue([])
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('initial state', () => {
    it('should start with null state', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, isListening } = useTaskStreamEvents(taskId)

      expect(state.value).toBeNull()
      expect(isListening.value).toBe(false)
    })

    it('should not start listening automatically', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { isListening } = useTaskStreamEvents(taskId)

      expect(isListening.value).toBe(false)
    })
  })

  describe('startListening', () => {
    it('should start listening and initialize state', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, isListening, startListening } = useTaskStreamEvents(taskId)

      await startListening()

      expect(isListening.value).toBe(true)
      expect(state.value).not.toBeNull()
      expect(state.value?.taskId).toBe('task-1')
      expect(state.value?.status).toBe('pending')
    })

    it('should not start if taskId is null', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>(null)
      const { isListening, startListening } = useTaskStreamEvents(taskId)

      await startListening()

      expect(isListening.value).toBe(false)
    })

    it('should not start twice', async () => {
      const { onTaskStreamEventForTask } = await import('@/api/agent-task')
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { startListening } = useTaskStreamEvents(taskId)

      await startListening()
      await startListening()

      expect(onTaskStreamEventForTask).toHaveBeenCalledTimes(1)
    })
  })

  describe('event handling', () => {
    it('should handle started event', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, startListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      await nextTick()

      expect(state.value?.status).toBe('running')
      expect(state.value?.taskName).toBe('Test Task')
      expect(state.value?.agentId).toBe('agent-1')
      expect(state.value?.executionMode).toBe('cli:claude')
    })

    it('should handle output event', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, startListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createOutputEvent('task-1', 'Hello world\n'))
      await nextTick()

      expect(state.value?.stdout).toBe('Hello world\n')
      expect(state.value?.outputLines).toHaveLength(1)
      expect(state.value?.outputLines?.[0]?.text).toBe('Hello world\n')
      expect(state.value?.outputLines?.[0]?.isStderr).toBe(false)
    })

    it('should handle stderr output', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, startListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createOutputEvent('task-1', 'Error!\n', true))
      await nextTick()

      expect(state.value?.stderr).toBe('Error!\n')
      expect(state.value?.outputLines?.[0]?.isStderr).toBe(true)
    })

    it('should handle progress event', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, startListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createProgressEvent('task-1', 'Compiling', 50))
      await nextTick()

      expect(state.value?.progressPhase).toBe('Compiling')
      expect(state.value?.progressPercent).toBe(50)
    })

    it('should handle completed event', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, isCompleted, isFinished, startListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createCompletedEvent('task-1', 'Success!', 5000))
      await nextTick()

      expect(state.value?.status).toBe('completed')
      expect(state.value?.result).toBe('Success!')
      expect(state.value?.durationMs).toBe(5000)
      expect(isCompleted.value).toBe(true)
      expect(isFinished.value).toBe(true)
    })

    it('should handle failed event', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, isFailed, isFinished, startListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createFailedEvent('task-1', 'Connection error', 3000))
      await nextTick()

      expect(state.value?.status).toBe('failed')
      expect(state.value?.error).toBe('Connection error')
      expect(state.value?.durationMs).toBe(3000)
      expect(isFailed.value).toBe(true)
      expect(isFinished.value).toBe(true)
    })

    it('should handle cancelled event', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, isCancelled, isFinished, startListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createCancelledEvent('task-1', 'User cancelled', 2000))
      await nextTick()

      expect(state.value?.status).toBe('cancelled')
      expect(state.value?.error).toBe('User cancelled')
      expect(isCancelled.value).toBe(true)
      expect(isFinished.value).toBe(true)
    })

    it('should handle heartbeat event', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, startListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createHeartbeatEvent('task-1', 10000))
      await nextTick()

      expect(state.value?.lastHeartbeat).not.toBeNull()
      expect(state.value?.durationMs).toBe(10000)
    })
  })

  describe('stopListening', () => {
    it('should call unlisten function', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { startListening, stopListening, isListening } = useTaskStreamEvents(taskId)

      await startListening()
      expect(isListening.value).toBe(true)

      stopListening()
      expect(mockUnlisten).toHaveBeenCalled()
      expect(isListening.value).toBe(false)
    })
  })

  describe('reset', () => {
    it('should clear state and stop listening', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, startListening, reset, isListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      await nextTick()

      expect(state.value).not.toBeNull()

      reset()

      expect(state.value).toBeNull()
      expect(isListening.value).toBe(false)
    })
  })

  describe('combinedOutput', () => {
    it('should combine all output lines', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { combinedOutput, startListening } = useTaskStreamEvents(taskId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createOutputEvent('task-1', 'Line 1\n'))
      mockEventCallback?.(createOutputEvent('task-1', 'Error\n', true))
      mockEventCallback?.(createOutputEvent('task-1', 'Line 2\n'))
      await nextTick()

      expect(combinedOutput.value).toBe('Line 1\nError\nLine 2\n')
    })
  })

  describe('output line limiting', () => {
    it('should limit output lines to maxOutputLines', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, startListening } = useTaskStreamEvents(taskId, { maxOutputLines: 5 })

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))

      // Add 10 lines
      for (let i = 0; i < 10; i++) {
        mockEventCallback?.(createOutputEvent('task-1', `Line ${i}\n`))
      }
      await nextTick()

      expect(state.value?.outputLines).toHaveLength(5)
      expect(state.value?.outputLines?.[0]?.text).toBe('Line 5\n')
      expect(state.value?.outputLines?.[4]?.text).toBe('Line 9\n')
    })
  })

  describe('event history limiting', () => {
    it('should limit events to maxEvents', async () => {
      const { useTaskStreamEvents } = await import('../useTaskStreamEvents')
      const taskId = ref<string | null>('task-1')
      const { state, startListening } = useTaskStreamEvents(taskId, { maxEvents: 5 })

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))

      // Add 10 output events
      for (let i = 0; i < 10; i++) {
        mockEventCallback?.(createOutputEvent('task-1', `Line ${i}\n`))
      }
      await nextTick()

      expect(state.value?.events).toHaveLength(5)
    })
  })
})

describe('useMultiTaskStreamEvents', () => {
  let mockEventCallback: ((event: TaskStreamEvent) => void) | null = null
  let mockUnlisten: ReturnType<typeof vi.fn>

  beforeEach(async () => {
    vi.clearAllMocks()
    mockEventCallback = null
    mockUnlisten = vi.fn()

    const { onTaskStreamEvent, getActiveAgentTasks } = await import('@/api/agent-task')
    vi.mocked(onTaskStreamEvent).mockImplementation(async (callback) => {
      mockEventCallback = callback
      return mockUnlisten as unknown as () => void
    })
    vi.mocked(getActiveAgentTasks).mockResolvedValue([])
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('initial state', () => {
    it('should start with empty tasks map', async () => {
      const { useMultiTaskStreamEvents } = await import('../useTaskStreamEvents')
      const { tasks, taskCount, isListening } = useMultiTaskStreamEvents()

      expect(tasks.value.size).toBe(0)
      expect(taskCount.value).toBe(0)
      expect(isListening.value).toBe(false)
    })
  })

  describe('startListening', () => {
    it('should start listening for all events', async () => {
      const { onTaskStreamEvent } = await import('@/api/agent-task')
      const { useMultiTaskStreamEvents } = await import('../useTaskStreamEvents')
      const { isListening, startListening } = useMultiTaskStreamEvents()

      await startListening()

      expect(isListening.value).toBe(true)
      expect(onTaskStreamEvent).toHaveBeenCalled()
    })

    it('should load active tasks on start', async () => {
      const { getActiveAgentTasks } = await import('@/api/agent-task')
      vi.mocked(getActiveAgentTasks).mockResolvedValue(['task-1', 'task-2'])

      const { useMultiTaskStreamEvents } = await import('../useTaskStreamEvents')
      const { tasks, startListening } = useMultiTaskStreamEvents()

      await startListening()

      expect(tasks.value.size).toBe(2)
      expect(tasks.value.get('task-1')?.status).toBe('running')
      expect(tasks.value.get('task-2')?.status).toBe('running')
    })
  })

  describe('multi-task event handling', () => {
    it('should track events for multiple tasks', async () => {
      const { useMultiTaskStreamEvents } = await import('../useTaskStreamEvents')
      const { tasks, runningCount, startListening } = useMultiTaskStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createStartedEvent('task-2'))
      await nextTick()

      expect(tasks.value.size).toBe(2)
      expect(runningCount.value).toBe(2)
      expect(tasks.value.get('task-1')?.status).toBe('running')
      expect(tasks.value.get('task-2')?.status).toBe('running')
    })

    it('should track finished tasks separately', async () => {
      const { useMultiTaskStreamEvents } = await import('../useTaskStreamEvents')
      const { runningTasks, finishedTasks, startListening } = useMultiTaskStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createStartedEvent('task-2'))
      mockEventCallback?.(createCompletedEvent('task-1', 'Done', 1000))
      await nextTick()

      expect(runningTasks.value).toHaveLength(1)
      expect(finishedTasks.value).toHaveLength(1)
      expect(runningTasks.value[0]?.taskId).toBe('task-2')
      expect(finishedTasks.value[0]?.taskId).toBe('task-1')
    })
  })

  describe('getTaskState', () => {
    it('should return state for specific task', async () => {
      const { useMultiTaskStreamEvents } = await import('../useTaskStreamEvents')
      const { getTaskState, startListening } = useMultiTaskStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      await nextTick()

      const state = getTaskState('task-1')
      expect(state?.taskId).toBe('task-1')
      expect(state?.status).toBe('running')
    })

    it('should return undefined for unknown task', async () => {
      const { useMultiTaskStreamEvents } = await import('../useTaskStreamEvents')
      const { getTaskState, startListening } = useMultiTaskStreamEvents()

      await startListening()
      const state = getTaskState('unknown')
      expect(state).toBeUndefined()
    })
  })

  describe('removeTask', () => {
    it('should remove task from tracking', async () => {
      const { useMultiTaskStreamEvents } = await import('../useTaskStreamEvents')
      const { tasks, removeTask, startListening } = useMultiTaskStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      await nextTick()

      expect(tasks.value.size).toBe(1)

      removeTask('task-1')
      await nextTick()

      expect(tasks.value.size).toBe(0)
    })
  })

  describe('clearFinished', () => {
    it('should clear all finished tasks', async () => {
      const { useMultiTaskStreamEvents } = await import('../useTaskStreamEvents')
      const { tasks, clearFinished, startListening } = useMultiTaskStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createStartedEvent('task-2'))
      mockEventCallback?.(createCompletedEvent('task-1', 'Done', 1000))
      mockEventCallback?.(createFailedEvent('task-2', 'Error', 500))
      mockEventCallback?.(createStartedEvent('task-3'))
      await nextTick()

      expect(tasks.value.size).toBe(3)

      clearFinished()
      await nextTick()

      expect(tasks.value.size).toBe(1)
      expect(tasks.value.has('task-3')).toBe(true)
    })
  })
})
