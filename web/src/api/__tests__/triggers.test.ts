import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient } from '@/api/config'
import * as triggersApi from '@/api/triggers'
import { API_ENDPOINTS } from '@/constants'

vi.mock('@/api/utils', () => ({
  isTauri: () => false,
  invokeCommand: vi.fn()
}))

describe('Triggers API', () => {
  let mock: MockAdapter

  beforeEach(() => {
    mock = new MockAdapter(apiClient)
  })

  afterEach(() => {
    mock.reset()
  })

  describe('activateWorkflow', () => {
    it('should activate workflow trigger', async () => {
      mock.onPut(API_ENDPOINTS.TRIGGER.ACTIVATE('wf1')).reply(200, {
        success: true
      })

      await expect(triggersApi.activateWorkflow('wf1')).resolves.toBeUndefined()
    })
  })

  describe('deactivateWorkflow', () => {
    it('should deactivate workflow trigger', async () => {
      mock.onPut(API_ENDPOINTS.TRIGGER.DEACTIVATE('wf1')).reply(200, {
        success: true
      })

      await expect(triggersApi.deactivateWorkflow('wf1')).resolves.toBeUndefined()
    })
  })

  describe('getTriggerStatus', () => {
    it('should get trigger status', async () => {
      const mockStatus = {
        is_active: true,
        trigger_config: {
          type: 'webhook' as const,
          path: '/webhook/test',
          method: 'POST',
          auth: null,
          response_mode: 'sync' as const
        },
        webhook_url: '/api/triggers/webhook/wf1',
        trigger_count: 0,
        last_triggered_at: null,
        activated_at: Date.now()
      }

      mock.onGet(API_ENDPOINTS.TRIGGER.STATUS('wf1')).reply(200, {
        success: true,
        data: mockStatus
      })

      const result = await triggersApi.getTriggerStatus('wf1')
      expect(result).toMatchObject({
        is_active: true,
        trigger_config: {
          type: 'webhook',
          path: '/webhook/test'
        }
      })
    })

    it('should return null for no trigger', async () => {
      mock.onGet(API_ENDPOINTS.TRIGGER.STATUS('wf1')).reply(200, {
        success: true,
        data: null
      })

      const result = await triggersApi.getTriggerStatus('wf1')
      expect(result).toBeNull()
    })

    it('should handle manual trigger status', async () => {
      const mockStatus = {
        is_active: true,
        trigger_config: {
          type: 'manual' as const
        },
        webhook_url: null,
        trigger_count: 5,
        last_triggered_at: Date.now(),
        activated_at: Date.now()
      }

      mock.onGet(API_ENDPOINTS.TRIGGER.STATUS('wf2')).reply(200, {
        success: true,
        data: mockStatus
      })

      const result = await triggersApi.getTriggerStatus('wf2')
      expect(result).toMatchObject({
        is_active: true,
        trigger_config: { type: 'manual' }
      })
    })
  })

  describe('testWorkflow', () => {
    it('should test workflow with data', async () => {
      const testData = { key: 'value' }
      const mockResponse = { result: 'success' }

      mock.onPost(API_ENDPOINTS.TRIGGER.TEST('wf1')).reply(200, {
        success: true,
        data: mockResponse
      })

      const result = await triggersApi.testWorkflow('wf1', testData)
      expect(result).toEqual(mockResponse)
    })

    it('should test workflow without data', async () => {
      const mockResponse = { result: 'success' }

      mock.onPost(API_ENDPOINTS.TRIGGER.TEST('wf1')).reply(200, {
        success: true,
        data: mockResponse
      })

      const result = await triggersApi.testWorkflow('wf1')
      expect(result).toEqual(mockResponse)
    })
  })

  describe('getWebhookUrl', () => {
    it('should generate webhook URL', () => {
      const url = triggersApi.getWebhookUrl('wf1')
      expect(url).toContain('/api/triggers/webhook/wf1')
    })
  })

  describe('Error Handling', () => {
    it('should handle network timeout on activate', async () => {
      mock.onPut(API_ENDPOINTS.TRIGGER.ACTIVATE('wf1')).timeout()
      await expect(triggersApi.activateWorkflow('wf1')).rejects.toThrow()
    })

    it('should handle 404 workflow not found', async () => {
      mock.onPut(API_ENDPOINTS.TRIGGER.ACTIVATE('missing')).reply(404, {
        success: false,
        message: 'Workflow not found'
      })
      await expect(triggersApi.activateWorkflow('missing')).rejects.toThrow('Workflow not found')
    })

    it('should handle failed activation', async () => {
      mock.onPut(API_ENDPOINTS.TRIGGER.ACTIVATE('wf1')).reply(200, {
        success: false,
        message: 'No trigger configured'
      })
      await expect(triggersApi.activateWorkflow('wf1')).rejects.toThrow('No trigger configured')
    })

    it('should handle network error', async () => {
      mock.onGet(API_ENDPOINTS.TRIGGER.STATUS('wf1')).networkError()
      await expect(triggersApi.getTriggerStatus('wf1')).rejects.toThrow()
    })
  })
})
