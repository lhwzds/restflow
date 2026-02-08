import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { ref, nextTick } from 'vue'
import type { BackgroundAgentStreamEvent as TaskStreamEvent } from '@/types/background-agent'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'

// Mock Tauri API
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('@/api/background-agent', () => ({
  onBackgroundAgentStreamEvent: vi.fn(),
  onBackgroundAgentStreamEventForBackgroundAgent: vi.fn(),
  runBackgroundAgentStreaming: vi.fn(),
  cancelBackgroundAgent: vi.fn(),
  getActiveBackgroundAgents: vi.fn(),
  isEventKind: vi.fn((event, type) => event.kind.type === type),
}))

// Helper to create mock events
function createEvent(backgroundAgentId: string, kind: StreamEventKind): TaskStreamEvent {
  return {
    task_id: backgroundAgentId,
    timestamp: Date.now(),
    kind,
  }
}

function createStartedEvent(backgroundAgentId: string): TaskStreamEvent {
  return createEvent(backgroundAgentId, {
    type: 'started',
    task_name: 'Test Task',
    agent_id: 'agent-1',
    execution_mode: 'cli:claude',
  })
}

function createOutputEvent(
  backgroundAgentId: string,
  text: string,
  isStderr = false,
): TaskStreamEvent {
  return createEvent(backgroundAgentId, {
    type: 'output',
    text,
    is_stderr: isStderr,
    is_complete: text.endsWith('\n'),
  })
}

function createProgressEvent(
  backgroundAgentId: string,
  phase: string,
  percent: number | null,
): TaskStreamEvent {
  return createEvent(backgroundAgentId, {
    type: 'progress',
    phase,
    percent,
    details: null,
  })
}

function createCompletedEvent(
  backgroundAgentId: string,
  result: string,
  durationMs: number,
): TaskStreamEvent {
  return createEvent(backgroundAgentId, {
    type: 'completed',
    result,
    duration_ms: durationMs,
    stats: null,
  })
}

function createFailedEvent(
  backgroundAgentId: string,
  error: string,
  durationMs: number,
): TaskStreamEvent {
  return createEvent(backgroundAgentId, {
    type: 'failed',
    error,
    error_code: null,
    duration_ms: durationMs,
    recoverable: false,
  })
}

function createCancelledEvent(
  backgroundAgentId: string,
  reason: string,
  durationMs: number,
): TaskStreamEvent {
  return createEvent(backgroundAgentId, {
    type: 'cancelled',
    reason,
    duration_ms: durationMs,
  })
}

function createHeartbeatEvent(backgroundAgentId: string, elapsedMs: number): TaskStreamEvent {
  return createEvent(backgroundAgentId, {
    type: 'heartbeat',
    elapsed_ms: elapsedMs,
  })
}

describe('useBackgroundAgentStreamEvents', () => {
  let mockEventCallback: ((event: TaskStreamEvent) => void) | null = null
  let mockUnlisten: ReturnType<typeof vi.fn>

  beforeEach(async () => {
    vi.clearAllMocks()
    mockEventCallback = null
    mockUnlisten = vi.fn()

    const { onBackgroundAgentStreamEventForBackgroundAgent, getActiveBackgroundAgents } =
      await import('@/api/background-agent')
    vi.mocked(onBackgroundAgentStreamEventForBackgroundAgent).mockImplementation(
      async (_backgroundAgentId, callback) => {
        mockEventCallback = callback
        return mockUnlisten as unknown as () => void
      },
    )
    vi.mocked(getActiveBackgroundAgents).mockResolvedValue([])
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('initial state', () => {
    it('should start with null state', async () => {
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, isListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

      expect(state.value).toBeNull()
      expect(isListening.value).toBe(false)
    })

    it('should not start listening automatically', async () => {
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { isListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

      expect(isListening.value).toBe(false)
    })
  })

  describe('startListening', () => {
    it('should start listening and initialize state', async () => {
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, isListening, startListening } =
        useBackgroundAgentStreamEvents(backgroundAgentId)

      await startListening()

      expect(isListening.value).toBe(true)
      expect(state.value).not.toBeNull()
      expect(state.value?.backgroundAgentId).toBe('task-1')
      expect(state.value?.status).toBe('pending')
    })

    it('should not start if backgroundAgentId is null', async () => {
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>(null)
      const { isListening, startListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

      await startListening()

      expect(isListening.value).toBe(false)
    })

    it('should not start twice', async () => {
      const { onBackgroundAgentStreamEventForBackgroundAgent } =
        await import('@/api/background-agent')
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { startListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

      await startListening()
      await startListening()

      expect(onBackgroundAgentStreamEventForBackgroundAgent).toHaveBeenCalledTimes(1)
    })
  })

  describe('event handling', () => {
    it('should handle started event', async () => {
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, startListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      await nextTick()

      expect(state.value?.status).toBe('running')
      expect(state.value?.backgroundAgentName).toBe('Test Task')
      expect(state.value?.agentId).toBe('agent-1')
      expect(state.value?.executionMode).toBe('cli:claude')
    })

    it('should handle output event', async () => {
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, startListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

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
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, startListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createOutputEvent('task-1', 'Error!\n', true))
      await nextTick()

      expect(state.value?.stderr).toBe('Error!\n')
      expect(state.value?.outputLines?.[0]?.isStderr).toBe(true)
    })

    it('should handle progress event', async () => {
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, startListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createProgressEvent('task-1', 'Compiling', 50))
      await nextTick()

      expect(state.value?.progressPhase).toBe('Compiling')
      expect(state.value?.progressPercent).toBe(50)
    })

    it('should handle completed event', async () => {
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, isCompleted, isFinished, startListening } =
        useBackgroundAgentStreamEvents(backgroundAgentId)

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
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, isFailed, isFinished, startListening } =
        useBackgroundAgentStreamEvents(backgroundAgentId)

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
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, isCancelled, isFinished, startListening } =
        useBackgroundAgentStreamEvents(backgroundAgentId)

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
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, startListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

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
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { startListening, stopListening, isListening } =
        useBackgroundAgentStreamEvents(backgroundAgentId)

      await startListening()
      expect(isListening.value).toBe(true)

      stopListening()
      expect(mockUnlisten).toHaveBeenCalled()
      expect(isListening.value).toBe(false)
    })
  })

  describe('reset', () => {
    it('should clear state and stop listening', async () => {
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, startListening, reset, isListening } =
        useBackgroundAgentStreamEvents(backgroundAgentId)

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
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { combinedOutput, startListening } = useBackgroundAgentStreamEvents(backgroundAgentId)

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
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, startListening } = useBackgroundAgentStreamEvents(backgroundAgentId, {
        maxOutputLines: 5,
      })

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
      const { useBackgroundAgentStreamEvents } = await import('../useBackgroundAgentStreamEvents')
      const backgroundAgentId = ref<string | null>('task-1')
      const { state, startListening } = useBackgroundAgentStreamEvents(backgroundAgentId, {
        maxEvents: 5,
      })

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

describe('useMultiBackgroundAgentStreamEvents', () => {
  let mockEventCallback: ((event: TaskStreamEvent) => void) | null = null
  let mockUnlisten: ReturnType<typeof vi.fn>

  beforeEach(async () => {
    vi.clearAllMocks()
    mockEventCallback = null
    mockUnlisten = vi.fn()

    const { onBackgroundAgentStreamEvent, getActiveBackgroundAgents } =
      await import('@/api/background-agent')
    vi.mocked(onBackgroundAgentStreamEvent).mockImplementation(async (callback) => {
      mockEventCallback = callback
      return mockUnlisten as unknown as () => void
    })
    vi.mocked(getActiveBackgroundAgents).mockResolvedValue([])
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('initial state', () => {
    it('should start with empty tasks map', async () => {
      const { useMultiBackgroundAgentStreamEvents } =
        await import('../useBackgroundAgentStreamEvents')
      const { backgroundAgents, backgroundAgentCount, isListening } =
        useMultiBackgroundAgentStreamEvents()

      expect(backgroundAgents.value.size).toBe(0)
      expect(backgroundAgentCount.value).toBe(0)
      expect(isListening.value).toBe(false)
    })
  })

  describe('startListening', () => {
    it('should start listening for all events', async () => {
      const { onBackgroundAgentStreamEvent } = await import('@/api/background-agent')
      const { useMultiBackgroundAgentStreamEvents } =
        await import('../useBackgroundAgentStreamEvents')
      const { isListening, startListening } = useMultiBackgroundAgentStreamEvents()

      await startListening()

      expect(isListening.value).toBe(true)
      expect(onBackgroundAgentStreamEvent).toHaveBeenCalled()
    })

    it('should load active background agents on start', async () => {
      const { getActiveBackgroundAgents } = await import('@/api/background-agent')
      vi.mocked(getActiveBackgroundAgents).mockResolvedValue([
        {
          background_agent_id: 'task-1',
          background_agent_name: 'Task One',
          executor_agent_id: 'agent-1',
          started_at: Date.now(),
          execution_mode: 'api',
        },
        {
          background_agent_id: 'task-2',
          background_agent_name: 'Task Two',
          executor_agent_id: 'agent-2',
          started_at: Date.now(),
          execution_mode: 'cli:claude',
        },
      ])

      const { useMultiBackgroundAgentStreamEvents } =
        await import('../useBackgroundAgentStreamEvents')
      const { backgroundAgents, startListening } = useMultiBackgroundAgentStreamEvents()

      await startListening()

      expect(backgroundAgents.value.size).toBe(2)
      expect(backgroundAgents.value.get('task-1')?.status).toBe('running')
      expect(backgroundAgents.value.get('task-2')?.status).toBe('running')
    })
  })

  describe('multi-task event handling', () => {
    it('should track events for multiple tasks', async () => {
      const { useMultiBackgroundAgentStreamEvents } =
        await import('../useBackgroundAgentStreamEvents')
      const { backgroundAgents, runningCount, startListening } =
        useMultiBackgroundAgentStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createStartedEvent('task-2'))
      await nextTick()

      expect(backgroundAgents.value.size).toBe(2)
      expect(runningCount.value).toBe(2)
      expect(backgroundAgents.value.get('task-1')?.status).toBe('running')
      expect(backgroundAgents.value.get('task-2')?.status).toBe('running')
    })

    it('should track finished tasks separately', async () => {
      const { useMultiBackgroundAgentStreamEvents } =
        await import('../useBackgroundAgentStreamEvents')
      const { runningBackgroundAgents, finishedBackgroundAgents, startListening } =
        useMultiBackgroundAgentStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createStartedEvent('task-2'))
      mockEventCallback?.(createCompletedEvent('task-1', 'Done', 1000))
      await nextTick()

      expect(runningBackgroundAgents.value).toHaveLength(1)
      expect(finishedBackgroundAgents.value).toHaveLength(1)
      expect(runningBackgroundAgents.value[0]?.backgroundAgentId).toBe('task-2')
      expect(finishedBackgroundAgents.value[0]?.backgroundAgentId).toBe('task-1')
    })
  })

  describe('getBackgroundAgentState', () => {
    it('should return state for specific task', async () => {
      const { useMultiBackgroundAgentStreamEvents } =
        await import('../useBackgroundAgentStreamEvents')
      const { getBackgroundAgentState, startListening } = useMultiBackgroundAgentStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      await nextTick()

      const state = getBackgroundAgentState('task-1')
      expect(state?.backgroundAgentId).toBe('task-1')
      expect(state?.status).toBe('running')
    })

    it('should return undefined for unknown task', async () => {
      const { useMultiBackgroundAgentStreamEvents } =
        await import('../useBackgroundAgentStreamEvents')
      const { getBackgroundAgentState, startListening } = useMultiBackgroundAgentStreamEvents()

      await startListening()
      const state = getBackgroundAgentState('unknown')
      expect(state).toBeUndefined()
    })
  })

  describe('removeBackgroundAgent', () => {
    it('should remove task from tracking', async () => {
      const { useMultiBackgroundAgentStreamEvents } =
        await import('../useBackgroundAgentStreamEvents')
      const { backgroundAgents, removeBackgroundAgent, startListening } =
        useMultiBackgroundAgentStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      await nextTick()

      expect(backgroundAgents.value.size).toBe(1)

      removeBackgroundAgent('task-1')
      await nextTick()

      expect(backgroundAgents.value.size).toBe(0)
    })
  })

  describe('clearFinished', () => {
    it('should clear all finished tasks', async () => {
      const { useMultiBackgroundAgentStreamEvents } =
        await import('../useBackgroundAgentStreamEvents')
      const { backgroundAgents, clearFinished, startListening } =
        useMultiBackgroundAgentStreamEvents()

      await startListening()
      mockEventCallback?.(createStartedEvent('task-1'))
      mockEventCallback?.(createStartedEvent('task-2'))
      mockEventCallback?.(createCompletedEvent('task-1', 'Done', 1000))
      mockEventCallback?.(createFailedEvent('task-2', 'Error', 500))
      mockEventCallback?.(createStartedEvent('task-3'))
      await nextTick()

      expect(backgroundAgents.value.size).toBe(3)

      clearFinished()
      await nextTick()

      expect(backgroundAgents.value.size).toBe(1)
      expect(backgroundAgents.value.has('task-3')).toBe(true)
    })
  })
})
