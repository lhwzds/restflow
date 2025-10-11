import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient } from '@/api/config'
import * as workflowsApi from '@/api/workflows'
import type { Workflow } from '@/types/generated/Workflow'
import type { ExecutionContext } from '@/types/generated/ExecutionContext'
import { API_ENDPOINTS } from '@/constants'

vi.mock('@/api/utils', () => ({
  isTauri: () => false,
  invokeCommand: vi.fn()
}))

describe('Workflows API', () => {
  let mock: MockAdapter

  beforeEach(() => {
    mock = new MockAdapter(apiClient)
  })

  afterEach(() => {
    mock.reset()
  })

  const createMockWorkflow = (id: string): Workflow => ({
    id,
    name: `Test Workflow ${id}`,
    nodes: [
      {
        id: 'node1',
        node_type: 'Agent',
        config: { model: 'gpt-4', prompt: 'test' },
        position: null
      }
    ],
    edges: []
  })

  describe('listWorkflows', () => {
    it('should fetch and return workflow list', async () => {
      const mockWorkflows = [createMockWorkflow('wf1'), createMockWorkflow('wf2')]

      mock.onGet(API_ENDPOINTS.WORKFLOW.LIST).reply(200, {
        success: true,
        data: mockWorkflows
      })

      const result = await workflowsApi.listWorkflows()
      expect(result).toEqual(mockWorkflows)
    })
  })

  describe('createWorkflow', () => {
    it('should create workflow and return id', async () => {
      const workflow = createMockWorkflow('new-wf')

      mock.onPost(API_ENDPOINTS.WORKFLOW.CREATE).reply(200, {
        success: true,
        data: { id: 'new-wf' }
      })

      const result = await workflowsApi.createWorkflow(workflow)
      expect(result).toEqual({ id: 'new-wf' })
    })
  })

  describe('getWorkflow', () => {
    it('should fetch specific workflow', async () => {
      const mockWorkflow = createMockWorkflow('wf1')

      mock.onGet(API_ENDPOINTS.WORKFLOW.GET('wf1')).reply(200, {
        success: true,
        data: mockWorkflow
      })

      const result = await workflowsApi.getWorkflow('wf1')
      expect(result).toEqual(mockWorkflow)
    })
  })

  describe('updateWorkflow', () => {
    it('should update workflow', async () => {
      const workflow = createMockWorkflow('wf1')

      mock.onPut(API_ENDPOINTS.WORKFLOW.UPDATE('wf1')).reply(200, {
        success: true
      })

      await expect(workflowsApi.updateWorkflow('wf1', workflow)).resolves.toBeUndefined()
    })
  })

  describe('deleteWorkflow', () => {
    it('should delete workflow', async () => {
      mock.onDelete(API_ENDPOINTS.WORKFLOW.DELETE('wf1')).reply(200, {
        success: true
      })

      await expect(workflowsApi.deleteWorkflow('wf1')).resolves.toBeUndefined()
    })
  })

  describe('executeSyncRun', () => {
    it('should execute workflow synchronously', async () => {
      const workflow = createMockWorkflow('wf1')
      const mockContext: ExecutionContext = {
        execution_id: 'exec-1',
        workflow_id: 'wf1',
        data: {
          'node.node1': { result: 'success' }
        }
      }

      mock.onPost(API_ENDPOINTS.EXECUTION.SYNC_RUN).reply(200, {
        success: true,
        data: mockContext
      })

      const result = await workflowsApi.executeSyncRun(workflow)
      expect(result).toEqual(mockContext)
    })
  })

  describe('executeSyncRunById', () => {
    it('should execute workflow by ID', async () => {
      const mockContext: ExecutionContext = {
        execution_id: 'exec-1',
        workflow_id: 'wf1',
        data: {
          'var.input_key': 'value',
          'node.node1': { output: 'completed' }
        }
      }

      mock.onPost(API_ENDPOINTS.EXECUTION.SYNC_RUN_BY_ID('wf1')).reply(200, {
        success: true,
        data: mockContext
      })

      const result = await workflowsApi.executeSyncRunById('wf1', { key: 'value' })
      expect(result).toEqual(mockContext)
    })
  })

  describe('executeAsyncSubmit', () => {
    it('should submit async execution', async () => {
      mock.onPost(API_ENDPOINTS.EXECUTION.ASYNC_SUBMIT('wf1')).reply(200, {
        success: true,
        data: { execution_id: 'exec-1', workflow_id: 'wf1' }
      })

      const result = await workflowsApi.executeAsyncSubmit('wf1', { key: 'value' })
      expect(result).toEqual({ execution_id: 'exec-1' })
    })
  })

  describe('Error Handling', () => {
    it('should handle network timeout', async () => {
      mock.onGet(API_ENDPOINTS.WORKFLOW.LIST).timeout()
      await expect(workflowsApi.listWorkflows()).rejects.toThrow()
    })

    it('should handle 404 not found', async () => {
      mock.onGet(API_ENDPOINTS.WORKFLOW.GET('missing')).reply(404, {
        success: false,
        message: 'Workflow not found'
      })
      await expect(workflowsApi.getWorkflow('missing')).rejects.toThrow('Workflow not found')
    })

    it('should handle 500 server error', async () => {
      mock.onPost(API_ENDPOINTS.WORKFLOW.CREATE).reply(500, {
        success: false,
        message: 'Internal server error'
      })
      await expect(workflowsApi.createWorkflow(createMockWorkflow('test'))).rejects.toThrow('Internal server error')
    })

    it('should handle network error', async () => {
      mock.onGet(API_ENDPOINTS.WORKFLOW.LIST).networkError()
      await expect(workflowsApi.listWorkflows()).rejects.toThrow()
    })

    it('should handle failed execution', async () => {
      mock.onPost(API_ENDPOINTS.EXECUTION.SYNC_RUN).reply(200, {
        success: false,
        message: 'Execution failed'
      })
      await expect(workflowsApi.executeSyncRun(createMockWorkflow('test'))).rejects.toThrow('Execution failed')
    })
  })
})
