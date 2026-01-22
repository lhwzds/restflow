import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient } from '@/api/config'
import * as tasksApi from '@/api/tasks'
import { API_ENDPOINTS } from '@/constants'

describe('Tasks API', () => {
  let mock: MockAdapter

  beforeEach(() => {
    mock = new MockAdapter(apiClient)
  })

  afterEach(() => {
    mock.reset()
  })

  describe('testNodeExecution', () => {
    it('should test node execution', async () => {
      const payload = {
        nodes: [
          {
            id: 'node1',
            node_type: 'Agent',
            config: { model: 'gpt-4' },
          },
        ],
        edges: [],
        input: { key: 'value' },
      }

      const mockResponse = { result: 'success' }

      mock.onPost(API_ENDPOINTS.EXECUTION.INLINE_RUN).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await tasksApi.testNodeExecution(payload)
      expect(result).toEqual(mockResponse)
    })
  })

  describe('Error Handling', () => {
    it('should handle network timeout', async () => {
      mock.onPost(API_ENDPOINTS.EXECUTION.INLINE_RUN).timeout()
      await expect(
        tasksApi.testNodeExecution({
          nodes: [],
          edges: [],
          input: {},
        }),
      ).rejects.toThrow()
    })

    it('should handle 500 server error', async () => {
      mock.onPost(API_ENDPOINTS.EXECUTION.INLINE_RUN).reply(500, {
        success: false,
        message: 'Server error',
      })
      await expect(
        tasksApi.testNodeExecution({
          nodes: [],
          edges: [],
          input: {},
        }),
      ).rejects.toThrow('Server error')
    })
  })
})
