import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient } from '@/api/config'
import * as tasksApi from '@/api/tasks'
import type { Task } from '@/types/generated/Task'
import { API_ENDPOINTS } from '@/constants'

vi.mock('@/api/utils', () => ({
  isTauri: () => false,
  invokeCommand: vi.fn()
}))

describe('Tasks API', () => {
  let mock: MockAdapter

  beforeEach(() => {
    mock = new MockAdapter(apiClient)
  })

  afterEach(() => {
    mock.reset()
  })

  const createMockTask = (id: string): Omit<Task, 'created_at' | 'started_at' | 'completed_at' | 'context'> & {
    created_at: number
    started_at: number | null
    completed_at: number | null
    context: any
  } => ({
    id,
    workflow_id: 'wf1',
    node_id: 'node1',
    status: 'Completed',
    input: { type: 'Print', data: { message: 'test' } },
    output: { type: 'Print', data: { printed: 'success' } },
    error: null,
    created_at: Date.now(),
    started_at: null,
    completed_at: null,
    execution_id: 'exec1',
    context: {
      execution_id: 'exec1',
      workflow_id: 'wf1',
      status: 'Completed',
      current_node: null,
      variables: {},
      outputs: {}
    }
  })

  describe('getTaskStatus', () => {
    it('should fetch task status', async () => {
      const mockTask = createMockTask('task1')

      mock.onGet(API_ENDPOINTS.TASK.STATUS('task1')).reply(200, {
        success: true,
        data: mockTask
      })

      const result = await tasksApi.getTaskStatus('task1')
      expect(result.id).toBe('task1')
      expect(result.status).toBe('Completed')
      expect(result.result).toEqual({ type: 'Print', data: { printed: 'success' } })
    })
  })

  describe('listTasks', () => {
    it('should list tasks', async () => {
      const mockTasks = [createMockTask('task1'), createMockTask('task2')]

      mock.onGet(API_ENDPOINTS.TASK.LIST).reply(200, {
        success: true,
        data: mockTasks
      })

      const result = await tasksApi.listTasks()
      expect(result).toEqual(mockTasks)
    })

    it('should list tasks with parameters', async () => {
      const mockTasks = [createMockTask('task1')]

      mock.onGet(API_ENDPOINTS.TASK.LIST).reply((config) => {
        expect(config.params).toEqual({
          execution_id: 'exec1',
          limit: 10
        })
        return [200, { success: true, data: mockTasks }]
      })

      const result = await tasksApi.listTasks({
        execution_id: 'exec1',
        limit: 10
      })
      expect(result).toEqual(mockTasks)
    })
  })

  describe('getTasksByExecutionId', () => {
    it('should get tasks by execution ID', async () => {
      const mockTasks = [createMockTask('task1')]

      mock.onGet(API_ENDPOINTS.TASK.LIST).reply(200, {
        success: true,
        data: mockTasks
      })

      const result = await tasksApi.getTasksByExecutionId('exec1')
      expect(result).toEqual(mockTasks)
    })
  })

  describe('getExecutionStatus', () => {
    it('should get execution status', async () => {
      const mockTasks = [createMockTask('task1')]

      mock.onGet(API_ENDPOINTS.EXECUTION.STATUS('exec1')).reply(200, {
        success: true,
        data: mockTasks
      })

      const result = await tasksApi.getExecutionStatus('exec1')
      expect(result).toEqual(mockTasks)
    })
  })

  describe('executeNode', () => {
    it('should execute node and return task ID', async () => {
      const node = {
        id: 'node1',
        node_type: 'Agent',
        config: { model: 'gpt-4' }
      }

      mock.onPost(API_ENDPOINTS.NODE.EXECUTE).reply(200, {
        success: true,
        data: { task_id: 'task1', message: 'Success' }
      })

      const result = await tasksApi.executeNode(node, { key: 'value' })
      expect(result).toBe('task1')
    })
  })

  describe('Error Handling', () => {
    it('should handle network timeout', async () => {
      mock.onGet(API_ENDPOINTS.TASK.LIST).timeout()
      await expect(tasksApi.listTasks()).rejects.toThrow()
    })

    it('should handle 404 task not found', async () => {
      mock.onGet(API_ENDPOINTS.TASK.STATUS('missing')).reply(404, {
        success: false,
        message: 'Task not found'
      })
      await expect(tasksApi.getTaskStatus('missing')).rejects.toThrow('Task not found')
    })

    it('should handle 500 server error', async () => {
      mock.onGet(API_ENDPOINTS.EXECUTION.STATUS('exec1')).reply(500, {
        success: false,
        message: 'Server error'
      })
      await expect(tasksApi.getExecutionStatus('exec1')).rejects.toThrow('Server error')
    })

    it('should handle network error', async () => {
      mock.onGet(API_ENDPOINTS.TASK.LIST).networkError()
      await expect(tasksApi.listTasks()).rejects.toThrow()
    })
  })
})
