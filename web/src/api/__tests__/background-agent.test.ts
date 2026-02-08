import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient } from '@/api/config'
import * as backgroundAgentApi from '@/api/background-agent'
import type { AgentTask } from '@/types/generated/AgentTask'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import type { TaskSchedule } from '@/types/generated/TaskSchedule'
import { API_ENDPOINTS } from '@/constants'

describe('Agent Task API', () => {
  let mock: MockAdapter

  beforeEach(() => {
    mock = new MockAdapter(apiClient)
  })

  afterEach(() => {
    mock.reset()
  })

  const createMockTask = (id: string, overrides: Partial<AgentTask> = {}): AgentTask => ({
    id,
    name: `Test Task ${id}`,
    description: null,
    agent_id: 'agent-001',
    input: null,
    input_template: null,
    schedule: { type: 'interval', interval_ms: 3600000, start_at: null },
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
    ...overrides,
  })

  const createMockEvent = (id: string, taskId: string): TaskEvent => ({
    id,
    task_id: taskId,
    event_type: 'completed',
    timestamp: Date.now(),
    message: 'Task completed successfully',
    output: 'Agent output',
    tokens_used: null,
    cost_usd: null,
    duration_ms: 1500,
  })

  describe('listBackgroundAgents', () => {
    it('should fetch and return task list', async () => {
      const mockTasks = [createMockTask('task1'), createMockTask('task2')]

      mock.onGet(API_ENDPOINTS.BACKGROUND_AGENT.LIST).reply(200, {
        success: true,
        data: mockTasks,
      })

      const result = await backgroundAgentApi.listBackgroundAgents()
      expect(result).toEqual(mockTasks)
      expect(result).toHaveLength(2)
    })

    it('should return empty array when no tasks exist', async () => {
      mock.onGet(API_ENDPOINTS.BACKGROUND_AGENT.LIST).reply(200, {
        success: true,
        data: [],
      })

      const result = await backgroundAgentApi.listBackgroundAgents()
      expect(result).toEqual([])
    })
  })

  describe('listBackgroundAgentsByStatus', () => {
    it('should fetch tasks filtered by status', async () => {
      const mockTasks = [
        createMockTask('task1', { status: 'active' }),
        createMockTask('task2', { status: 'active' }),
      ]

      mock.onGet(API_ENDPOINTS.BACKGROUND_AGENT.LIST_BY_STATUS('active')).reply(200, {
        success: true,
        data: mockTasks,
      })

      const result = await backgroundAgentApi.listBackgroundAgentsByStatus('active')
      expect(result).toEqual(mockTasks)
      expect(result.every((t) => t.status === 'active')).toBe(true)
    })
  })

  describe('getBackgroundAgent', () => {
    it('should fetch specific task by ID', async () => {
      const mockTask = createMockTask('task1')

      mock.onGet(API_ENDPOINTS.BACKGROUND_AGENT.GET('task1')).reply(200, {
        success: true,
        data: mockTask,
      })

      const result = await backgroundAgentApi.getBackgroundAgent('task1')
      expect(result).toEqual(mockTask)
      expect(result.id).toBe('task1')
    })

    it('should throw error when task not found', async () => {
      mock.onGet(API_ENDPOINTS.BACKGROUND_AGENT.GET('missing')).reply(404, {
        success: false,
        message: 'Background agent not found',
      })

      await expect(backgroundAgentApi.getBackgroundAgent('missing')).rejects.toThrow(
        'Background agent not found',
      )
    })
  })

  describe('createBackgroundAgent', () => {
    it('should create task with minimal fields', async () => {
      const request: backgroundAgentApi.CreateBackgroundAgentRequest = {
        name: 'New Task',
        agent_id: 'agent-001',
        schedule: { type: 'interval', interval_ms: 3600000, start_at: null },
      }

      const mockResponse = createMockTask('new-task', { name: 'New Task' })

      mock.onPost(API_ENDPOINTS.BACKGROUND_AGENT.CREATE).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await backgroundAgentApi.createBackgroundAgent(request)
      expect(result.name).toBe('New Task')
      expect(result.agent_id).toBe('agent-001')
    })

    it('should create task with all fields', async () => {
      const request: backgroundAgentApi.CreateBackgroundAgentRequest = {
        name: 'Full Task',
        agent_id: 'agent-002',
        schedule: { type: 'cron', expression: '0 9 * * *', timezone: 'America/Los_Angeles' },
        description: 'A complete task',
        input: 'Hello agent',
        notification: {
          telegram_enabled: true,
          telegram_bot_token: null,
          telegram_chat_id: '123456',
          notify_on_failure_only: false,
          include_output: true,
        },
      }

      const mockResponse = createMockTask('full-task', {
        name: 'Full Task',
        description: 'A complete task',
        input: 'Hello agent',
        schedule: { type: 'cron', expression: '0 9 * * *', timezone: 'America/Los_Angeles' },
      })

      mock.onPost(API_ENDPOINTS.BACKGROUND_AGENT.CREATE).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await backgroundAgentApi.createBackgroundAgent(request)
      expect(result.name).toBe('Full Task')
      expect(result.description).toBe('A complete task')
    })

    it('should pass input_template and memory_scope fields', async () => {
      const request: backgroundAgentApi.CreateBackgroundAgentRequest = {
        name: 'Templated Task',
        agent_id: 'agent-003',
        schedule: { type: 'interval', interval_ms: 3600000, start_at: null },
        input_template: 'Task {{task.id}}',
        memory_scope: 'per_background_agent',
      }

      const mockResponse = createMockTask('templated-task', {
        name: 'Templated Task',
      })

      mock.onPost(API_ENDPOINTS.BACKGROUND_AGENT.CREATE).reply((config) => {
        const body = JSON.parse(config.data)
        expect(body.input_template).toBe('Task {{task.id}}')
        expect(body.memory_scope).toBe('per_background_agent')
        return [
          200,
          {
            success: true,
            data: mockResponse,
          },
        ]
      })

      const result = await backgroundAgentApi.createBackgroundAgent(request)
      expect(result.id).toBe('templated-task')
    })
  })

  describe('updateBackgroundAgent', () => {
    it('should update task name', async () => {
      const mockResponse = createMockTask('task1', { name: 'Updated Name' })

      mock.onPatch(API_ENDPOINTS.BACKGROUND_AGENT.UPDATE('task1')).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await backgroundAgentApi.updateBackgroundAgent('task1', { name: 'Updated Name' })
      expect(result.name).toBe('Updated Name')
    })

    it('should update task schedule', async () => {
      const newSchedule: TaskSchedule = { type: 'once', run_at: Date.now() + 86400000 }
      const mockResponse = createMockTask('task1', { schedule: newSchedule })

      mock.onPatch(API_ENDPOINTS.BACKGROUND_AGENT.UPDATE('task1')).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await backgroundAgentApi.updateBackgroundAgent('task1', { schedule: newSchedule })
      expect(result.schedule.type).toBe('once')
    })

    it('should pass input_template and memory_scope on update', async () => {
      const mockResponse = createMockTask('task1')

      mock.onPatch(API_ENDPOINTS.BACKGROUND_AGENT.UPDATE('task1')).reply((config) => {
        const body = JSON.parse(config.data)
        expect(body.input_template).toBe('Updated {{task.name}}')
        expect(body.memory_scope).toBe('shared_agent')
        return [
          200,
          {
            success: true,
            data: mockResponse,
          },
        ]
      })

      await backgroundAgentApi.updateBackgroundAgent('task1', {
        input_template: 'Updated {{task.name}}',
        memory_scope: 'shared_agent',
      })
    })
  })

  describe('deleteBackgroundAgent', () => {
    it('should delete task and return true', async () => {
      mock.onDelete(API_ENDPOINTS.BACKGROUND_AGENT.DELETE('task1')).reply(200, {
        success: true,
        data: true,
      })

      const result = await backgroundAgentApi.deleteBackgroundAgent('task1')
      expect(result).toBe(true)
    })
  })

  describe('pauseBackgroundAgent', () => {
    it('should pause task and return updated task', async () => {
      const mockResponse = createMockTask('task1', { status: 'paused' })

      mock.onPost(API_ENDPOINTS.BACKGROUND_AGENT.CONTROL('task1')).reply((config) => {
        const body = JSON.parse(config.data)
        expect(body.action).toBe('pause')
        return [
          200,
          {
            success: true,
            data: mockResponse,
          },
        ]
      })

      const result = await backgroundAgentApi.pauseBackgroundAgent('task1')
      expect(result.status).toBe('paused')
    })
  })

  describe('resumeBackgroundAgent', () => {
    it('should resume task and return updated task', async () => {
      const mockResponse = createMockTask('task1', { status: 'active' })

      mock.onPost(API_ENDPOINTS.BACKGROUND_AGENT.CONTROL('task1')).reply((config) => {
        const body = JSON.parse(config.data)
        expect(body.action).toBe('resume')
        return [
          200,
          {
            success: true,
            data: mockResponse,
          },
        ]
      })

      const result = await backgroundAgentApi.resumeBackgroundAgent('task1')
      expect(result.status).toBe('active')
    })
  })

  describe('getBackgroundAgentEvents', () => {
    it('should fetch all events for a task', async () => {
      const mockEvents = [createMockEvent('event1', 'task1'), createMockEvent('event2', 'task1')]

      mock.onGet(API_ENDPOINTS.BACKGROUND_AGENT.PROGRESS('task1')).reply(200, {
        success: true,
        data: {
          recent_events: mockEvents,
        },
      })

      const result = await backgroundAgentApi.getBackgroundAgentEvents('task1')
      expect(result).toHaveLength(2)
    })

    it('should fetch limited events when limit is specified', async () => {
      const mockEvents = [createMockEvent('event1', 'task1')]

      mock.onGet(`${API_ENDPOINTS.BACKGROUND_AGENT.PROGRESS('task1')}?event_limit=1`).reply(200, {
        success: true,
        data: {
          recent_events: mockEvents,
        },
      })

      const result = await backgroundAgentApi.getBackgroundAgentEvents('task1', 1)
      expect(result).toHaveLength(1)
    })
  })

  describe('getRunnableBackgroundAgents', () => {
    it('should fetch active tasks and filter by next_run_at', async () => {
      const now = Date.now()
      const mockTasks = [
        createMockTask('task1', { status: 'active', next_run_at: now - 1000 }),
        createMockTask('task2', { status: 'active', next_run_at: now + 100000 }),
      ]

      mock.onGet(API_ENDPOINTS.BACKGROUND_AGENT.LIST_BY_STATUS('active')).reply(200, {
        success: true,
        data: mockTasks,
      })

      const result = await backgroundAgentApi.getRunnableBackgroundAgents()
      expect(result).toHaveLength(1)
      expect(result[0]?.id).toBe('task1')
    })
  })

  describe('Helper Functions', () => {
    describe('createDefaultNotificationConfig', () => {
      it('should create default config with all fields', () => {
        const config = backgroundAgentApi.createDefaultNotificationConfig()
        expect(config.telegram_enabled).toBe(false)
        expect(config.telegram_bot_token).toBeNull()
        expect(config.telegram_chat_id).toBeNull()
        expect(config.notify_on_failure_only).toBe(false)
        expect(config.include_output).toBe(true)
      })
    })

    describe('createOnceSchedule', () => {
      it('should create once schedule with run_at', () => {
        const runAt = Date.now() + 3600000
        const schedule = backgroundAgentApi.createOnceSchedule(runAt)
        expect(schedule.type).toBe('once')
        expect((schedule as any).run_at).toBe(runAt)
      })
    })

    describe('createIntervalSchedule', () => {
      it('should create interval schedule with interval_ms', () => {
        const schedule = backgroundAgentApi.createIntervalSchedule(3600000)
        expect(schedule.type).toBe('interval')
        expect((schedule as any).interval_ms).toBe(3600000)
        expect((schedule as any).start_at).toBeNull()
      })

      it('should create interval schedule with start_at', () => {
        const startAt = Date.now()
        const schedule = backgroundAgentApi.createIntervalSchedule(3600000, startAt)
        expect((schedule as any).start_at).toBe(startAt)
      })
    })

    describe('createCronSchedule', () => {
      it('should create cron schedule with expression', () => {
        const schedule = backgroundAgentApi.createCronSchedule('0 9 * * *')
        expect(schedule.type).toBe('cron')
        expect((schedule as any).expression).toBe('0 9 * * *')
        expect((schedule as any).timezone).toBeNull()
      })

      it('should create cron schedule with timezone', () => {
        const schedule = backgroundAgentApi.createCronSchedule('0 9 * * *', 'America/Los_Angeles')
        expect((schedule as any).timezone).toBe('America/Los_Angeles')
      })
    })

    describe('formatSchedule', () => {
      it('should format once schedule', () => {
        const schedule: TaskSchedule = { type: 'once', run_at: 1704067200000 }
        const result = backgroundAgentApi.formatSchedule(schedule)
        expect(result).toContain('Once at')
      })

      it('should format interval schedule with hours', () => {
        const schedule: TaskSchedule = { type: 'interval', interval_ms: 7200000, start_at: null }
        const result = backgroundAgentApi.formatSchedule(schedule)
        expect(result).toBe('Every 2 hours')
      })

      it('should format interval schedule with minutes', () => {
        const schedule: TaskSchedule = { type: 'interval', interval_ms: 1800000, start_at: null }
        const result = backgroundAgentApi.formatSchedule(schedule)
        expect(result).toBe('Every 30 minutes')
      })

      it('should format interval schedule with hours and minutes', () => {
        const schedule: TaskSchedule = { type: 'interval', interval_ms: 5400000, start_at: null }
        const result = backgroundAgentApi.formatSchedule(schedule)
        expect(result).toBe('Every 1h 30m')
      })

      it('should format cron schedule', () => {
        const schedule: TaskSchedule = { type: 'cron', expression: '0 9 * * *', timezone: null }
        const result = backgroundAgentApi.formatSchedule(schedule)
        expect(result).toBe('Cron: 0 9 * * *')
      })

      it('should format cron schedule with timezone', () => {
        const schedule: TaskSchedule = {
          type: 'cron',
          expression: '0 9 * * *',
          timezone: 'America/Los_Angeles',
        }
        const result = backgroundAgentApi.formatSchedule(schedule)
        expect(result).toBe('Cron: 0 9 * * * (America/Los_Angeles)')
      })
    })

    describe('formatBackgroundAgentStatus', () => {
      it('should format all status types', () => {
        expect(backgroundAgentApi.formatBackgroundAgentStatus('active')).toBe('Active')
        expect(backgroundAgentApi.formatBackgroundAgentStatus('paused')).toBe('Paused')
        expect(backgroundAgentApi.formatBackgroundAgentStatus('running')).toBe('Running')
        expect(backgroundAgentApi.formatBackgroundAgentStatus('completed')).toBe('Completed')
        expect(backgroundAgentApi.formatBackgroundAgentStatus('failed')).toBe('Failed')
      })
    })

    describe('getStatusColor', () => {
      it('should return correct colors for status', () => {
        expect(backgroundAgentApi.getStatusColor('active')).toBe('success')
        expect(backgroundAgentApi.getStatusColor('paused')).toBe('info')
        expect(backgroundAgentApi.getStatusColor('running')).toBe('primary')
        expect(backgroundAgentApi.getStatusColor('completed')).toBe('success')
        expect(backgroundAgentApi.getStatusColor('failed')).toBe('danger')
      })
    })
  })

  describe('Error Handling', () => {
    it('should handle network timeout', async () => {
      mock.onGet(API_ENDPOINTS.BACKGROUND_AGENT.LIST).timeout()
      await expect(backgroundAgentApi.listBackgroundAgents()).rejects.toThrow()
    })

    it('should handle 500 server error', async () => {
      mock.onPost(API_ENDPOINTS.BACKGROUND_AGENT.CREATE).reply(500, {
        success: false,
        message: 'Internal server error',
      })

      const request: backgroundAgentApi.CreateBackgroundAgentRequest = {
        name: 'Test',
        agent_id: 'agent-001',
        schedule: { type: 'interval', interval_ms: 3600000, start_at: null },
      }

      await expect(backgroundAgentApi.createBackgroundAgent(request)).rejects.toThrow('Internal server error')
    })

    it('should handle network error', async () => {
      mock.onGet(API_ENDPOINTS.BACKGROUND_AGENT.LIST).networkError()
      await expect(backgroundAgentApi.listBackgroundAgents()).rejects.toThrow()
    })

    it('should handle validation error on update', async () => {
      mock.onPatch(API_ENDPOINTS.BACKGROUND_AGENT.UPDATE('task1')).reply(400, {
        success: false,
        message: 'Invalid schedule configuration',
      })

      await expect(
        backgroundAgentApi.updateBackgroundAgent('task1', {
          schedule: { type: 'cron', expression: 'invalid', timezone: null },
        }),
      ).rejects.toThrow('Invalid schedule configuration')
    })
  })
})
