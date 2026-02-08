/**
 * Agent Task Store Tests
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useBackgroundAgentStore } from '../backgroundAgentStore'
import type { AgentTask } from '@/types/generated/AgentTask'
import * as backgroundAgentApi from '@/api/background-agent'

// Mock the API module
vi.mock('@/api/background-agent', () => ({
  listBackgroundAgents: vi.fn(),
  listBackgroundAgentsByStatus: vi.fn(),
  getBackgroundAgent: vi.fn(),
  createBackgroundAgent: vi.fn(),
  updateBackgroundAgent: vi.fn(),
  deleteBackgroundAgent: vi.fn(),
  pauseBackgroundAgent: vi.fn(),
  resumeBackgroundAgent: vi.fn(),
  getBackgroundAgentEvents: vi.fn(),
  onBackgroundAgentStreamEvent: vi.fn(),
}))

const mockTask: AgentTask = {
  id: 'task-1',
  name: 'Test Task',
  description: 'A test task',
  agent_id: 'agent-1',
  input: 'test input',
  input_template: null,
  schedule: { type: 'once', run_at: Date.now() + 3600000 },
  execution_mode: { type: 'api' },
  notification: {
    telegram_enabled: false,
    telegram_bot_token: null,
    telegram_chat_id: null,
    notify_on_failure_only: false,
    include_output: true,
  },
  memory: {
    max_messages: 100,
    enable_file_memory: false,
    persist_on_complete: false,
    memory_scope: 'shared_agent',
  },
  status: 'active',
  created_at: Date.now(),
  updated_at: Date.now(),
  last_run_at: null,
  next_run_at: Date.now() + 3600000,
  success_count: 0,
  failure_count: 0,
  total_tokens_used: 0,
  total_cost_usd: 0,
  last_error: null,
  webhook: null,
  summary_message_id: null,
}

const mockTask2: AgentTask = {
  ...mockTask,
  id: 'task-2',
  name: 'Another Task',
  status: 'paused',
  created_at: Date.now() - 1000,
}

describe('agentTaskStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  describe('initial state', () => {
    it('should have empty initial state', () => {
      const store = useBackgroundAgentStore()
      expect(store.tasks).toEqual([])
      expect(store.selectedTaskId).toBeNull()
      expect(store.isLoading).toBe(false)
      expect(store.error).toBeNull()
      expect(store.statusFilter).toBe('all')
    })
  })

  describe('fetchTasks', () => {
    it('should fetch and store tasks', async () => {
      const mockTasks = [mockTask, mockTask2]
      vi.mocked(backgroundAgentApi.listBackgroundAgents).mockResolvedValue(mockTasks)

      const store = useBackgroundAgentStore()
      await store.fetchTasks()

      expect(store.tasks).toEqual(mockTasks)
      expect(store.isLoading).toBe(false)
      expect(store.error).toBeNull()
    })

    it('should handle fetch error', async () => {
      vi.mocked(backgroundAgentApi.listBackgroundAgents).mockRejectedValue(new Error('Network error'))

      const store = useBackgroundAgentStore()
      await store.fetchTasks()

      expect(store.tasks).toEqual([])
      expect(store.error).toBe('Network error')
    })
  })

  describe('createTask', () => {
    it('should create a new task and add to store', async () => {
      vi.mocked(backgroundAgentApi.createBackgroundAgent).mockResolvedValue(mockTask)

      const store = useBackgroundAgentStore()
      const request = {
        name: 'Test Task',
        agent_id: 'agent-1',
        schedule: { type: 'once' as const, run_at: Date.now() + 3600000 },
      }

      const result = await store.createTask(request)

      expect(result).toEqual(mockTask)
      expect(store.tasks).toContainEqual(mockTask)
    })

    it('should handle create error', async () => {
      vi.mocked(backgroundAgentApi.createBackgroundAgent).mockRejectedValue(new Error('Create failed'))

      const store = useBackgroundAgentStore()
      const result = await store.createTask({
        name: 'Test',
        agent_id: 'agent-1',
        schedule: { type: 'once', run_at: Date.now() },
      })

      expect(result).toBeNull()
      expect(store.error).toBe('Create failed')
    })
  })

  describe('updateTask', () => {
    it('should update an existing task', async () => {
      const updatedTask = { ...mockTask, name: 'Updated Name' }
      vi.mocked(backgroundAgentApi.updateBackgroundAgent).mockResolvedValue(updatedTask)

      const store = useBackgroundAgentStore()
      store.tasks = [mockTask]

      const result = await store.updateTask('task-1', { name: 'Updated Name' })

      expect(result).toEqual(updatedTask)
      expect(store.tasks[0]!.name).toBe('Updated Name')
    })
  })

  describe('deleteTask', () => {
    it('should delete a task and remove from store', async () => {
      vi.mocked(backgroundAgentApi.deleteBackgroundAgent).mockResolvedValue(true)

      const store = useBackgroundAgentStore()
      store.tasks = [mockTask, mockTask2]

      const result = await store.deleteTask('task-1')

      expect(result).toBe(true)
      expect(store.tasks).toHaveLength(1)
      expect(store.tasks[0]!.id).toBe('task-2')
    })

    it('should clear selected task if deleted', async () => {
      vi.mocked(backgroundAgentApi.deleteBackgroundAgent).mockResolvedValue(true)

      const store = useBackgroundAgentStore()
      store.tasks = [mockTask]
      store.selectedTaskId = 'task-1'

      await store.deleteTask('task-1')

      expect(store.selectedTaskId).toBeNull()
    })
  })

  describe('pauseTask and resumeTask', () => {
    it('should pause a task', async () => {
      const pausedTask = { ...mockTask, status: 'paused' as const }
      vi.mocked(backgroundAgentApi.pauseBackgroundAgent).mockResolvedValue(pausedTask)

      const store = useBackgroundAgentStore()
      store.tasks = [mockTask]

      const result = await store.pauseTask('task-1')

      expect(result).toBe(true)
      expect(store.tasks[0]!.status).toBe('paused')
    })

    it('should resume a paused task', async () => {
      const activeTask = { ...mockTask2, status: 'active' as const }
      vi.mocked(backgroundAgentApi.resumeBackgroundAgent).mockResolvedValue(activeTask)

      const store = useBackgroundAgentStore()
      store.tasks = [mockTask2]

      const result = await store.resumeTask('task-2')

      expect(result).toBe(true)
      expect(store.tasks[0]!.status).toBe('active')
    })
  })

  describe('filteredTasks getter', () => {
    it('should filter by status', () => {
      const store = useBackgroundAgentStore()
      store.tasks = [mockTask, mockTask2]
      store.statusFilter = 'paused'

      expect(store.filteredTasks).toHaveLength(1)
      expect(store.filteredTasks[0]!.status).toBe('paused')
    })

    it('should filter by search query', () => {
      const store = useBackgroundAgentStore()
      store.tasks = [mockTask, mockTask2]
      store.searchQuery = 'Another'

      expect(store.filteredTasks).toHaveLength(1)
      expect(store.filteredTasks[0]!.name).toBe('Another Task')
    })

    it('should sort by name ascending', () => {
      const store = useBackgroundAgentStore()
      store.tasks = [mockTask, mockTask2]
      store.sortField = 'name'
      store.sortOrder = 'asc'

      expect(store.filteredTasks[0]!.name).toBe('Another Task')
      expect(store.filteredTasks[1]!.name).toBe('Test Task')
    })

    it('should sort by created_at descending by default', () => {
      const store = useBackgroundAgentStore()
      store.tasks = [mockTask, mockTask2]

      // mockTask has later created_at, should come first in desc order
      expect(store.filteredTasks[0]!.id).toBe('task-1')
    })
  })

  describe('statusCounts getter', () => {
    it('should count tasks by status', () => {
      const store = useBackgroundAgentStore()
      store.tasks = [mockTask, mockTask2]

      expect(store.statusCounts).toEqual({
        all: 2,
        active: 1,
        paused: 1,
        running: 0,
        completed: 0,
        failed: 0,
      })
    })
  })

  describe('selectTask', () => {
    it('should select a task and fetch events', async () => {
      const mockEvents = [
        { id: 'event-1', task_id: 'task-1', event_type: 'started', timestamp: Date.now() },
      ]
      vi.mocked(backgroundAgentApi.getBackgroundAgentEvents).mockResolvedValue(mockEvents as any)

      const store = useBackgroundAgentStore()
      await store.selectTask('task-1')

      expect(store.selectedTaskId).toBe('task-1')
      expect(backgroundAgentApi.getBackgroundAgentEvents).toHaveBeenCalledWith('task-1', undefined)
    })

    it('should clear selection when null', async () => {
      const store = useBackgroundAgentStore()
      store.selectedTaskId = 'task-1'
      store.selectedTaskEvents = [{ id: 'event-1' } as any]

      await store.selectTask(null)

      expect(store.selectedTaskId).toBeNull()
      expect(store.selectedTaskEvents).toEqual([])
    })
  })

  describe('setSort', () => {
    it('should toggle sort order when same field', () => {
      const store = useBackgroundAgentStore()
      store.sortField = 'name'
      store.sortOrder = 'asc'

      store.setSort('name')

      expect(store.sortOrder).toBe('desc')
    })

    it('should set new field with default desc order', () => {
      const store = useBackgroundAgentStore()
      store.sortField = 'name'
      store.sortOrder = 'asc'

      store.setSort('created_at')

      expect(store.sortField).toBe('created_at')
      expect(store.sortOrder).toBe('desc')
    })
  })

  describe('clearFilters', () => {
    it('should reset all filters to defaults', () => {
      const store = useBackgroundAgentStore()
      store.statusFilter = 'paused'
      store.searchQuery = 'test'
      store.sortField = 'name'
      store.sortOrder = 'asc'

      store.clearFilters()

      expect(store.statusFilter).toBe('all')
      expect(store.searchQuery).toBe('')
      expect(store.sortField).toBe('created_at')
      expect(store.sortOrder).toBe('desc')
    })
  })

  describe('local updates', () => {
    it('should update task locally', () => {
      const store = useBackgroundAgentStore()
      store.tasks = [mockTask]

      const updatedTask = { ...mockTask, name: 'Locally Updated' }
      store.updateTaskLocally(updatedTask)

      expect(store.tasks[0]!.name).toBe('Locally Updated')
    })

    it('should add new task if not exists', () => {
      const store = useBackgroundAgentStore()
      store.tasks = [mockTask]

      store.updateTaskLocally(mockTask2)

      expect(store.tasks).toHaveLength(2)
    })

    it('should remove task locally', () => {
      const store = useBackgroundAgentStore()
      store.tasks = [mockTask, mockTask2]

      store.removeTaskLocally('task-1')

      expect(store.tasks).toHaveLength(1)
      expect(store.tasks[0]!.id).toBe('task-2')
    })
  })

  describe('realtime sync', () => {
    it('should subscribe to task stream and refresh task on terminal events', async () => {
      let streamCallback: ((event: any) => void) | null = null
      const unlisten = vi.fn()

      vi.mocked(backgroundAgentApi.onBackgroundAgentStreamEvent).mockImplementation(async (callback: any) => {
        streamCallback = callback
        return unlisten
      })
      vi.mocked(backgroundAgentApi.getBackgroundAgent).mockResolvedValue({
        ...mockTask,
        status: 'completed',
      })

      const store = useBackgroundAgentStore()
      await store.startRealtimeSync()

      expect(backgroundAgentApi.onBackgroundAgentStreamEvent).toHaveBeenCalledTimes(1)
      expect(streamCallback).not.toBeNull()

      streamCallback!({
        task_id: 'task-1',
        timestamp: Date.now(),
        kind: { type: 'completed', result: 'done', duration_ms: 100 },
      })

      await Promise.resolve()
      expect(backgroundAgentApi.getBackgroundAgent).toHaveBeenCalledWith('task-1')

      store.stopRealtimeSync()
      expect(unlisten).toHaveBeenCalledTimes(1)
    })

    it('should poll tasks while realtime sync is active', async () => {
      vi.useFakeTimers()
      const unlisten = vi.fn()
      vi.mocked(backgroundAgentApi.onBackgroundAgentStreamEvent).mockResolvedValue(unlisten as any)
      vi.mocked(backgroundAgentApi.listBackgroundAgents).mockResolvedValue([mockTask])

      const store = useBackgroundAgentStore()
      await store.startRealtimeSync(1000)

      await vi.advanceTimersByTimeAsync(1000)
      expect(backgroundAgentApi.listBackgroundAgents).toHaveBeenCalledTimes(1)

      store.stopRealtimeSync()
      vi.useRealTimers()
    })
  })
})
