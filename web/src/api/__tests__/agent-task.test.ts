import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient } from '@/api/config'
import * as agentTaskApi from '@/api/agent-task'
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
    schedule: { type: 'interval', interval_ms: 3600000, start_at: null },
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
    },
    status: 'active',
    created_at: Date.now(),
    updated_at: Date.now(),
    last_run_at: null,
    next_run_at: Date.now() + 3600000,
    success_count: 0,
    failure_count: 0,
    last_error: null,
    ...overrides,
  })

  const createMockEvent = (id: string, taskId: string): TaskEvent => ({
    id,
    task_id: taskId,
    event_type: 'completed',
    timestamp: Date.now(),
    message: 'Task completed successfully',
    output: 'Agent output',
    duration_ms: 1500,
  })

  describe('listAgentTasks', () => {
    it('should fetch and return task list', async () => {
      const mockTasks = [createMockTask('task1'), createMockTask('task2')]

      mock.onGet(API_ENDPOINTS.AGENT_TASK.LIST).reply(200, {
        success: true,
        data: mockTasks,
      })

      const result = await agentTaskApi.listAgentTasks()
      expect(result).toEqual(mockTasks)
      expect(result).toHaveLength(2)
    })

    it('should return empty array when no tasks exist', async () => {
      mock.onGet(API_ENDPOINTS.AGENT_TASK.LIST).reply(200, {
        success: true,
        data: [],
      })

      const result = await agentTaskApi.listAgentTasks()
      expect(result).toEqual([])
    })
  })

  describe('listAgentTasksByStatus', () => {
    it('should fetch tasks filtered by status', async () => {
      const mockTasks = [
        createMockTask('task1', { status: 'active' }),
        createMockTask('task2', { status: 'active' }),
      ]

      mock.onGet(API_ENDPOINTS.AGENT_TASK.LIST_BY_STATUS('active')).reply(200, {
        success: true,
        data: mockTasks,
      })

      const result = await agentTaskApi.listAgentTasksByStatus('active')
      expect(result).toEqual(mockTasks)
      expect(result.every(t => t.status === 'active')).toBe(true)
    })
  })

  describe('getAgentTask', () => {
    it('should fetch specific task by ID', async () => {
      const mockTask = createMockTask('task1')

      mock.onGet(API_ENDPOINTS.AGENT_TASK.GET('task1')).reply(200, {
        success: true,
        data: mockTask,
      })

      const result = await agentTaskApi.getAgentTask('task1')
      expect(result).toEqual(mockTask)
      expect(result.id).toBe('task1')
    })

    it('should throw error when task not found', async () => {
      mock.onGet(API_ENDPOINTS.AGENT_TASK.GET('missing')).reply(404, {
        success: false,
        message: 'Agent task not found',
      })

      await expect(agentTaskApi.getAgentTask('missing')).rejects.toThrow('Agent task not found')
    })
  })

  describe('createAgentTask', () => {
    it('should create task with minimal fields', async () => {
      const request: agentTaskApi.CreateAgentTaskRequest = {
        name: 'New Task',
        agent_id: 'agent-001',
        schedule: { type: 'interval', interval_ms: 3600000, start_at: null },
      }

      const mockResponse = createMockTask('new-task', { name: 'New Task' })

      mock.onPost(API_ENDPOINTS.AGENT_TASK.CREATE).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await agentTaskApi.createAgentTask(request)
      expect(result.name).toBe('New Task')
      expect(result.agent_id).toBe('agent-001')
    })

    it('should create task with all fields', async () => {
      const request: agentTaskApi.CreateAgentTaskRequest = {
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

      mock.onPost(API_ENDPOINTS.AGENT_TASK.CREATE).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await agentTaskApi.createAgentTask(request)
      expect(result.name).toBe('Full Task')
      expect(result.description).toBe('A complete task')
    })
  })

  describe('updateAgentTask', () => {
    it('should update task name', async () => {
      const mockResponse = createMockTask('task1', { name: 'Updated Name' })

      mock.onPut(API_ENDPOINTS.AGENT_TASK.UPDATE('task1')).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await agentTaskApi.updateAgentTask('task1', { name: 'Updated Name' })
      expect(result.name).toBe('Updated Name')
    })

    it('should update task schedule', async () => {
      const newSchedule: TaskSchedule = { type: 'once', run_at: Date.now() + 86400000 }
      const mockResponse = createMockTask('task1', { schedule: newSchedule })

      mock.onPut(API_ENDPOINTS.AGENT_TASK.UPDATE('task1')).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await agentTaskApi.updateAgentTask('task1', { schedule: newSchedule })
      expect(result.schedule.type).toBe('once')
    })
  })

  describe('deleteAgentTask', () => {
    it('should delete task and return true', async () => {
      mock.onDelete(API_ENDPOINTS.AGENT_TASK.DELETE('task1')).reply(200, {
        success: true,
        data: true,
      })

      const result = await agentTaskApi.deleteAgentTask('task1')
      expect(result).toBe(true)
    })
  })

  describe('pauseAgentTask', () => {
    it('should pause task and return updated task', async () => {
      const mockResponse = createMockTask('task1', { status: 'paused' })

      mock.onPost(API_ENDPOINTS.AGENT_TASK.PAUSE('task1')).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await agentTaskApi.pauseAgentTask('task1')
      expect(result.status).toBe('paused')
    })
  })

  describe('resumeAgentTask', () => {
    it('should resume task and return updated task', async () => {
      const mockResponse = createMockTask('task1', { status: 'active' })

      mock.onPost(API_ENDPOINTS.AGENT_TASK.RESUME('task1')).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await agentTaskApi.resumeAgentTask('task1')
      expect(result.status).toBe('active')
    })
  })

  describe('getAgentTaskEvents', () => {
    it('should fetch all events for a task', async () => {
      const mockEvents = [
        createMockEvent('event1', 'task1'),
        createMockEvent('event2', 'task1'),
      ]

      mock.onGet(API_ENDPOINTS.AGENT_TASK.EVENTS('task1')).reply(200, {
        success: true,
        data: mockEvents,
      })

      const result = await agentTaskApi.getAgentTaskEvents('task1')
      expect(result).toHaveLength(2)
    })

    it('should fetch limited events when limit is specified', async () => {
      const mockEvents = [createMockEvent('event1', 'task1')]

      mock.onGet(`${API_ENDPOINTS.AGENT_TASK.EVENTS('task1')}?limit=1`).reply(200, {
        success: true,
        data: mockEvents,
      })

      const result = await agentTaskApi.getAgentTaskEvents('task1', 1)
      expect(result).toHaveLength(1)
    })
  })

  describe('getRunnableAgentTasks', () => {
    it('should fetch runnable tasks', async () => {
      const mockTasks = [createMockTask('task1')]

      mock.onGet(API_ENDPOINTS.AGENT_TASK.RUNNABLE).reply(200, {
        success: true,
        data: mockTasks,
      })

      const result = await agentTaskApi.getRunnableAgentTasks()
      expect(result).toHaveLength(1)
    })
  })

  describe('Helper Functions', () => {
    describe('createDefaultNotificationConfig', () => {
      it('should create default config with all fields', () => {
        const config = agentTaskApi.createDefaultNotificationConfig()
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
        const schedule = agentTaskApi.createOnceSchedule(runAt)
        expect(schedule.type).toBe('once')
        expect((schedule as any).run_at).toBe(runAt)
      })
    })

    describe('createIntervalSchedule', () => {
      it('should create interval schedule with interval_ms', () => {
        const schedule = agentTaskApi.createIntervalSchedule(3600000)
        expect(schedule.type).toBe('interval')
        expect((schedule as any).interval_ms).toBe(3600000)
        expect((schedule as any).start_at).toBeNull()
      })

      it('should create interval schedule with start_at', () => {
        const startAt = Date.now()
        const schedule = agentTaskApi.createIntervalSchedule(3600000, startAt)
        expect((schedule as any).start_at).toBe(startAt)
      })
    })

    describe('createCronSchedule', () => {
      it('should create cron schedule with expression', () => {
        const schedule = agentTaskApi.createCronSchedule('0 9 * * *')
        expect(schedule.type).toBe('cron')
        expect((schedule as any).expression).toBe('0 9 * * *')
        expect((schedule as any).timezone).toBeNull()
      })

      it('should create cron schedule with timezone', () => {
        const schedule = agentTaskApi.createCronSchedule('0 9 * * *', 'America/Los_Angeles')
        expect((schedule as any).timezone).toBe('America/Los_Angeles')
      })
    })

    describe('formatSchedule', () => {
      it('should format once schedule', () => {
        const schedule: TaskSchedule = { type: 'once', run_at: 1704067200000 }
        const result = agentTaskApi.formatSchedule(schedule)
        expect(result).toContain('Once at')
      })

      it('should format interval schedule with hours', () => {
        const schedule: TaskSchedule = { type: 'interval', interval_ms: 7200000, start_at: null }
        const result = agentTaskApi.formatSchedule(schedule)
        expect(result).toBe('Every 2 hours')
      })

      it('should format interval schedule with minutes', () => {
        const schedule: TaskSchedule = { type: 'interval', interval_ms: 1800000, start_at: null }
        const result = agentTaskApi.formatSchedule(schedule)
        expect(result).toBe('Every 30 minutes')
      })

      it('should format interval schedule with hours and minutes', () => {
        const schedule: TaskSchedule = { type: 'interval', interval_ms: 5400000, start_at: null }
        const result = agentTaskApi.formatSchedule(schedule)
        expect(result).toBe('Every 1h 30m')
      })

      it('should format cron schedule', () => {
        const schedule: TaskSchedule = { type: 'cron', expression: '0 9 * * *', timezone: null }
        const result = agentTaskApi.formatSchedule(schedule)
        expect(result).toBe('Cron: 0 9 * * *')
      })

      it('should format cron schedule with timezone', () => {
        const schedule: TaskSchedule = { type: 'cron', expression: '0 9 * * *', timezone: 'America/Los_Angeles' }
        const result = agentTaskApi.formatSchedule(schedule)
        expect(result).toBe('Cron: 0 9 * * * (America/Los_Angeles)')
      })
    })

    describe('formatTaskStatus', () => {
      it('should format all status types', () => {
        expect(agentTaskApi.formatTaskStatus('active')).toBe('Active')
        expect(agentTaskApi.formatTaskStatus('paused')).toBe('Paused')
        expect(agentTaskApi.formatTaskStatus('running')).toBe('Running')
        expect(agentTaskApi.formatTaskStatus('completed')).toBe('Completed')
        expect(agentTaskApi.formatTaskStatus('failed')).toBe('Failed')
      })
    })

    describe('getStatusColor', () => {
      it('should return correct colors for status', () => {
        expect(agentTaskApi.getStatusColor('active')).toBe('success')
        expect(agentTaskApi.getStatusColor('paused')).toBe('info')
        expect(agentTaskApi.getStatusColor('running')).toBe('primary')
        expect(agentTaskApi.getStatusColor('completed')).toBe('success')
        expect(agentTaskApi.getStatusColor('failed')).toBe('danger')
      })
    })
  })

  describe('Error Handling', () => {
    it('should handle network timeout', async () => {
      mock.onGet(API_ENDPOINTS.AGENT_TASK.LIST).timeout()
      await expect(agentTaskApi.listAgentTasks()).rejects.toThrow()
    })

    it('should handle 500 server error', async () => {
      mock.onPost(API_ENDPOINTS.AGENT_TASK.CREATE).reply(500, {
        success: false,
        message: 'Internal server error',
      })

      const request: agentTaskApi.CreateAgentTaskRequest = {
        name: 'Test',
        agent_id: 'agent-001',
        schedule: { type: 'interval', interval_ms: 3600000, start_at: null },
      }

      await expect(agentTaskApi.createAgentTask(request)).rejects.toThrow('Internal server error')
    })

    it('should handle network error', async () => {
      mock.onGet(API_ENDPOINTS.AGENT_TASK.LIST).networkError()
      await expect(agentTaskApi.listAgentTasks()).rejects.toThrow()
    })

    it('should handle validation error on update', async () => {
      mock.onPut(API_ENDPOINTS.AGENT_TASK.UPDATE('task1')).reply(400, {
        success: false,
        message: 'Invalid schedule configuration',
      })

      await expect(
        agentTaskApi.updateAgentTask('task1', { schedule: { type: 'cron', expression: 'invalid', timezone: null } })
      ).rejects.toThrow('Invalid schedule configuration')
    })
  })
})
