import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useTaskStore as useCanonicalTaskStore } from '../taskStore'
import {
  useBackgroundAgentStore,
  useBackgroundAgentStore as useTaskStore,
} from '../backgroundAgentStore'
import * as api from '@/api/task'
import type { Task } from '@/types/generated/Task'

const {
  mockListTasks,
  mockGetTask,
  mockPauseTask,
  mockResumeTask,
  mockStopTask,
  mockRunTaskNow,
  mockCreateTaskFromSession,
  mockUpdateTask,
  mockDeleteTask,
} = vi.hoisted(() => ({
  mockListTasks: vi.fn(),
  mockGetTask: vi.fn(),
  mockPauseTask: vi.fn(),
  mockResumeTask: vi.fn(),
  mockStopTask: vi.fn(),
  mockRunTaskNow: vi.fn(),
  mockCreateTaskFromSession: vi.fn(),
  mockUpdateTask: vi.fn(),
  mockDeleteTask: vi.fn(),
}))

vi.mock('@/api/task', () => ({
  listTasks: mockListTasks,
  getTask: mockGetTask,
  pauseTask: mockPauseTask,
  resumeTask: mockResumeTask,
  stopTask: mockStopTask,
  runTaskNow: mockRunTaskNow,
  createTaskFromSession: mockCreateTaskFromSession,
  updateTask: mockUpdateTask,
  deleteTask: mockDeleteTask,
}))

/**
 * Build a minimal task fixture with required fields.
 */
function createMockAgent(
  id: string,
  status: Task['status'] = 'active',
): Task {
  return {
    id,
    name: `Agent ${id}`,
    description: null,
    agent_id: 'test-agent',
    chat_session_id: `session-${id}`,
    input: null,
    input_template: null,
    schedule: { type: 'manual' },
    execution_mode: 'api',
    timeout_secs: null,
    notification: { enabled: false },
    memory: { enabled: false },
    durability_mode: 'none',
    resource_limits: {},
    prerequisites: [],
    continuation: { enabled: false },
    continuation_total_iterations: 0,
    continuation_segments_completed: 0,
    status,
    created_at: 1000,
    updated_at: 1000,
    last_run_at: null,
    next_run_at: null,
    success_count: 0,
    failure_count: 0,
    total_tokens_used: 0,
    total_cost_usd: 0,
    last_error: null,
    webhook: null,
    summary_message_id: null,
  } as unknown as Task
}

describe('taskStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  // ---------------------------------------------------------------------------
  // Getters
  // ---------------------------------------------------------------------------

  describe('getters', () => {
    describe('filteredAgents', () => {
      it('returns all agents when no status filter is set', () => {
        const store = useTaskStore()
        store.agents = [createMockAgent('a1', 'active'), createMockAgent('a2', 'paused')]

        expect(store.filteredAgents).toHaveLength(2)
      })

      it('filters agents by status when a filter is set', () => {
        const store = useTaskStore()
        store.agents = [
          createMockAgent('a1', 'active'),
          createMockAgent('a2', 'paused'),
          createMockAgent('a3', 'running'),
        ]
        store.statusFilter = 'paused'

        expect(store.filteredAgents).toHaveLength(1)
        expect(store.filteredAgents[0]!.id).toBe('a2')
      })

      it('returns empty array when no agents match the filter', () => {
        const store = useTaskStore()
        store.agents = [createMockAgent('a1', 'active')]
        store.statusFilter = 'failed'

        expect(store.filteredAgents).toHaveLength(0)
      })
    })

    describe('selectedAgent', () => {
      it('returns null when no agent is selected', () => {
        const store = useTaskStore()
        store.agents = [createMockAgent('a1')]

        expect(store.selectedAgent).toBeNull()
      })

      it('returns the matching agent when selectedAgentId is set', () => {
        const store = useTaskStore()
        store.agents = [createMockAgent('a1'), createMockAgent('a2')]
        store.selectedAgentId = 'a2'

        expect(store.selectedAgent).not.toBeNull()
        expect(store.selectedAgent!.id).toBe('a2')
      })

      it('returns null when selectedAgentId does not match any agent', () => {
        const store = useTaskStore()
        store.agents = [createMockAgent('a1')]
        store.selectedAgentId = 'nonexistent'

        expect(store.selectedAgent).toBeNull()
      })
    })

    describe('runningCount', () => {
      it('returns 0 when no agents are running', () => {
        const store = useTaskStore()
        store.agents = [createMockAgent('a1', 'active'), createMockAgent('a2', 'paused')]

        expect(store.runningCount).toBe(0)
      })

      it('counts only running agents', () => {
        const store = useTaskStore()
        store.agents = [
          createMockAgent('a1', 'running'),
          createMockAgent('a2', 'active'),
          createMockAgent('a3', 'running'),
        ]

        expect(store.runningCount).toBe(2)
      })
    })
  })

  // ---------------------------------------------------------------------------
  // Actions
  // ---------------------------------------------------------------------------

  describe('actions', () => {
    describe('fetchAgents', () => {
      it('fetches agents and updates state on success', async () => {
        const mockAgents = [createMockAgent('a1'), createMockAgent('a2')]
        vi.mocked(api.listTasks).mockResolvedValue(mockAgents)

        const store = useTaskStore()
        await store.fetchAgents()

        expect(api.listTasks).toHaveBeenCalledOnce()
        expect(store.agents).toEqual(mockAgents)
        expect(store.isLoading).toBe(false)
        expect(store.error).toBeNull()
      })

      it('sets error on failure', async () => {
        vi.mocked(api.listTasks).mockRejectedValue(new Error('Network error'))

        const store = useTaskStore()
        await store.fetchAgents()

        expect(store.error).toBe('Network error')
        expect(store.isLoading).toBe(false)
        expect(store.agents).toEqual([])
      })

      it('sets a generic error message for non-Error rejections', async () => {
        vi.mocked(api.listTasks).mockRejectedValue('something broke')

        const store = useTaskStore()
        await store.fetchAgents()

        expect(store.error).toBe('Failed to fetch tasks')
      })

      it('toggles isLoading while fetching', async () => {
        let resolveFetch: (value: Task[]) => void
        vi.mocked(api.listTasks).mockImplementation(
          () =>
            new Promise((resolve) => {
              resolveFetch = resolve
            }),
        )

        const store = useTaskStore()
        const promise = store.fetchAgents()
        expect(store.isLoading).toBe(true)

        resolveFetch!([])
        await promise
        expect(store.isLoading).toBe(false)
      })
    })

    describe('pauseAgent', () => {
      it('calls API and updates agent locally on success', async () => {
        const paused = createMockAgent('a1', 'paused')
        vi.mocked(api.pauseTask).mockResolvedValue(paused)

        const store = useTaskStore()
        store.agents = [createMockAgent('a1', 'active')]

        await store.pauseAgent('a1')

        expect(api.pauseTask).toHaveBeenCalledWith('a1')
        expect(store.agents[0]!.status).toBe('paused')
        expect(store.error).toBeNull()
      })

      it('sets error on failure', async () => {
        vi.mocked(api.pauseTask).mockRejectedValue(new Error('Pause failed'))

        const store = useTaskStore()
        await store.pauseAgent('a1')

        expect(store.error).toBe('Pause failed')
      })
    })

    describe('resumeAgent', () => {
      it('calls API and updates agent locally on success', async () => {
        const resumed = createMockAgent('a1', 'active')
        vi.mocked(api.resumeTask).mockResolvedValue(resumed)

        const store = useTaskStore()
        store.agents = [createMockAgent('a1', 'paused')]

        await store.resumeAgent('a1')

        expect(api.resumeTask).toHaveBeenCalledWith('a1')
        expect(store.agents[0]!.status).toBe('active')
        expect(store.error).toBeNull()
      })

      it('sets error on failure', async () => {
        vi.mocked(api.resumeTask).mockRejectedValue(new Error('Resume failed'))

        const store = useTaskStore()
        await store.resumeAgent('a1')

        expect(store.error).toBe('Resume failed')
      })
    })

    describe('deleteAgent', () => {
      it('removes agent from local list on success', async () => {
        vi.mocked(api.deleteTask).mockResolvedValue({ id: 'a1', deleted: true })

        const store = useTaskStore()
        store.agents = [createMockAgent('a1'), createMockAgent('a2')]
        store.selectedAgentId = 'a1'

        const result = await store.deleteAgent('a1')

        expect(api.deleteTask).toHaveBeenCalledWith('a1')
        expect(result).toBe(true)
        expect(store.agents).toHaveLength(1)
        expect(store.agents[0]!.id).toBe('a2')
        // Selected agent should be cleared when deleted agent was selected
        expect(store.selectedAgentId).toBeNull()
      })

      it('does not remove agent when API returns false', async () => {
        vi.mocked(api.deleteTask).mockResolvedValue({ id: 'a1', deleted: false })

        const store = useTaskStore()
        store.agents = [createMockAgent('a1')]

        const result = await store.deleteAgent('a1')

        expect(result).toBe(false)
        expect(store.agents).toHaveLength(1)
      })

      it('does not clear selectedAgentId when deleting a different agent', async () => {
        vi.mocked(api.deleteTask).mockResolvedValue({ id: 'a1', deleted: true })

        const store = useTaskStore()
        store.agents = [createMockAgent('a1'), createMockAgent('a2')]
        store.selectedAgentId = 'a2'

        await store.deleteAgent('a1')

        expect(store.selectedAgentId).toBe('a2')
      })

      it('returns false and sets error on failure', async () => {
        vi.mocked(api.deleteTask).mockRejectedValue(new Error('Delete failed'))

        const store = useTaskStore()
        const result = await store.deleteAgent('a1')

        expect(result).toBe(false)
        expect(store.error).toBe('Delete failed')
      })

    })

    describe('convertSessionToAgent', () => {
      it('calls API, stores converted agent, and returns it on success', async () => {
        const converted = createMockAgent('a-converted', 'active')
        vi.mocked(api.createTaskFromSession).mockResolvedValue({
          task: converted,
          source_session_id: 'session-1',
          source_session_agent_id: 'test-agent',
          run_now: true,
        })

        const store = useTaskStore()
        store.agents = [createMockAgent('a1')]

        const result = await store.convertSessionToAgent({
          session_id: 'session-1',
          name: 'Background: Session 1',
          run_now: true,
        })

        expect(api.createTaskFromSession).toHaveBeenCalledWith({
          session_id: 'session-1',
          name: 'Background: Session 1',
          run_now: true,
        })
        expect(result).toEqual(converted)
        expect(store.agents.map((agent) => agent.id)).toEqual(['a1', 'a-converted'])
        expect(store.error).toBeNull()
      })

      it('updates an existing agent locally instead of appending duplicates', async () => {
        const existing = createMockAgent('a-converted', 'paused')
        const updated = { ...existing, status: 'running' as const, name: 'Converted Updated' }
        vi.mocked(api.createTaskFromSession).mockResolvedValue({
          task: updated,
          source_session_id: 'session-1',
          source_session_agent_id: 'test-agent',
          run_now: true,
        })

        const store = useTaskStore()
        store.agents = [createMockAgent('a1'), existing]

        const result = await store.convertSessionToAgent({
          session_id: 'session-1',
          name: 'Converted Updated',
          run_now: true,
        })

        expect(result).toEqual(updated)
        expect(store.agents).toHaveLength(2)
        expect(store.agents[1]).toEqual(updated)
      })

      it('returns null and sets error on failure', async () => {
        vi.mocked(api.createTaskFromSession).mockRejectedValue(new Error('Convert failed'))

        const store = useTaskStore()
        const result = await store.convertSessionToAgent({
          session_id: 'session-1',
        })

        expect(result).toBeNull()
        expect(store.error).toBe('Convert failed')
      })

    })

    describe('convertSessionToWorkspace', () => {
      it('deletes background agent binding while preserving the session', async () => {
        const store = useTaskStore()
        const target = createMockAgent('bg-1', 'active')
        target.chat_session_id = 'session-keep'
        store.agents = [target, createMockAgent('bg-2', 'paused')]

        vi.mocked(api.deleteTask).mockResolvedValue({ id: 'bg-1', deleted: true })

        const result = await store.convertSessionToWorkspace('session-keep')

        expect(result).toBe(true)
        expect(api.updateTask).not.toHaveBeenCalled()
        expect(api.deleteTask).toHaveBeenCalledWith('bg-1')
        expect(store.agents.map((agent) => agent.id)).toEqual(['bg-2'])
        expect(store.error).toBeNull()
      })

      it('refreshes agent list once when session binding is not loaded locally', async () => {
        const store = useTaskStore()
        const fetched = createMockAgent('bg-3', 'active')
        fetched.chat_session_id = 'session-fetched'

        vi.mocked(api.listTasks).mockResolvedValue([fetched])
        vi.mocked(api.deleteTask).mockResolvedValue({ id: 'bg-3', deleted: true })

        const result = await store.convertSessionToWorkspace('session-fetched')

        expect(result).toBe(true)
        expect(api.listTasks).toHaveBeenCalledOnce()
        expect(api.updateTask).not.toHaveBeenCalled()
        expect(api.deleteTask).toHaveBeenCalledWith('bg-3')
      })

      it('returns false when no bound background agent exists for session', async () => {
        const store = useTaskStore()
        vi.mocked(api.listTasks).mockResolvedValue([])

        const result = await store.convertSessionToWorkspace('missing-session')

        expect(result).toBe(false)
        expect(store.error).toBe('Task binding not found for this session')
        expect(api.updateTask).not.toHaveBeenCalled()
        expect(api.deleteTask).not.toHaveBeenCalled()
      })

    })

    describe('updateAgentLocally', () => {
      it('replaces an existing agent in the list', () => {
        const store = useTaskStore()
        store.agents = [createMockAgent('a1', 'active')]

        const updated = createMockAgent('a1', 'paused')
        store.updateAgentLocally(updated)

        expect(store.agents).toHaveLength(1)
        expect(store.agents[0]!.status).toBe('paused')
      })

      it('appends a new agent if not found in the list', () => {
        const store = useTaskStore()
        store.agents = [createMockAgent('a1')]

        const newAgent = createMockAgent('a2', 'running')
        store.updateAgentLocally(newAgent)

        expect(store.agents).toHaveLength(2)
        expect(store.agents[1]!.id).toBe('a2')
      })
    })

    describe('selectAgent', () => {
      it('sets selectedAgentId', () => {
        const store = useTaskStore()
        store.selectAgent('a1')
        expect(store.selectedAgentId).toBe('a1')
      })

      it('clears selectedAgentId when null is passed', () => {
        const store = useTaskStore()
        store.selectedAgentId = 'a1'
        store.selectAgent(null)
        expect(store.selectedAgentId).toBeNull()
      })
    })

    describe('setStatusFilter', () => {
      it('sets status filter', () => {
        const store = useTaskStore()
        store.setStatusFilter('running')
        expect(store.statusFilter).toBe('running')
      })

      it('clears status filter when null is passed', () => {
        const store = useTaskStore()
        store.statusFilter = 'running'
        store.setStatusFilter(null)
        expect(store.statusFilter).toBeNull()
      })
    })

    describe('stopAgent', () => {
      it('calls stop API and updates the agent locally', async () => {
        vi.mocked(api.stopTask).mockResolvedValue(createMockAgent('task-1', 'paused'))

        const store = useTaskStore()
        store.agents = [createMockAgent('task-1', 'running')]
        await store.stopAgent('task-1')

        expect(api.stopTask).toHaveBeenCalledWith('task-1')
        expect(api.listTasks).not.toHaveBeenCalled()
        expect(store.agents[0]!.status).toBe('paused')
      })

      it('sets error on failure', async () => {
        vi.mocked(api.stopTask).mockRejectedValue(new Error('Stop failed'))

        const store = useTaskStore()
        await store.stopAgent('task-1')

        expect(store.error).toBe('Stop failed')
      })
    })

    describe('runAgentNow', () => {
      it('calls run-now API and updates the agent locally', async () => {
        const runningAgent = createMockAgent('a1', 'running')
        vi.mocked(api.runTaskNow).mockResolvedValue(runningAgent)

        const store = useTaskStore()
        store.agents = [createMockAgent('a1', 'active')]
        const result = await store.runAgentNow('a1')

        expect(api.runTaskNow).toHaveBeenCalledWith('a1')
        expect(api.listTasks).not.toHaveBeenCalled()
        expect(result).toEqual(runningAgent)
        expect(store.agents[0]).toEqual(runningAgent)
      })

      it('returns null and sets error on failure', async () => {
        vi.mocked(api.runTaskNow).mockRejectedValue(new Error('Run failed'))

        const store = useTaskStore()
        const result = await store.runAgentNow('a1')

        expect(result).toBeNull()
        expect(store.error).toBe('Run failed')
      })

    })
  })

  it('keeps the legacy background-agent store wrapper wired to the canonical task store', () => {
    const taskStore = useCanonicalTaskStore()
    const legacyStore = useBackgroundAgentStore()

    expect(legacyStore).toBe(taskStore)
    expect(typeof legacyStore.fetchAgents).toBe('function')
    expect(typeof legacyStore.pauseAgent).toBe('function')
    expect(typeof legacyStore.resumeAgent).toBe('function')
    expect(typeof legacyStore.stopAgent).toBe('function')
    expect(typeof legacyStore.runAgentNow).toBe('function')
    expect(typeof legacyStore.deleteAgent).toBe('function')
    expect(typeof legacyStore.convertSessionToAgent).toBe('function')
    expect(typeof legacyStore.convertSessionToWorkspace).toBe('function')
    expect(legacyStore.filteredAgents).toBe(legacyStore.filteredTasks)
    expect(legacyStore.selectedAgent).toBe(legacyStore.selectedTask)
    expect(legacyStore.runningCount).toBe(legacyStore.runningTaskCount)
    expect(legacyStore.agentBySessionId).toBe(legacyStore.taskBySessionId)
  })
})
