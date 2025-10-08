import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient } from '@/api/config'
import * as secretsApi from '@/api/secrets'
import type { Secret } from '@/types/generated/Secret'
import { API_ENDPOINTS } from '@/constants'

vi.mock('@/api/utils', () => ({
  isTauri: () => false,
  invokeCommand: vi.fn()
}))

describe('Secrets API', () => {
  let mock: MockAdapter

  beforeEach(() => {
    mock = new MockAdapter(apiClient)
  })

  afterEach(() => {
    mock.reset()
  })

  const createMockSecret = (key: string): Secret => ({
    key,
    value: '',
    description: `Description for ${key}`,
    created_at: Date.now(),
    updated_at: Date.now()
  })

  describe('listSecrets', () => {
    it('should fetch and return secrets list', async () => {
      const mockSecrets = [
        createMockSecret('API_KEY_1'),
        createMockSecret('API_KEY_2')
      ]

      mock.onGet(API_ENDPOINTS.SECRET.LIST).reply(200, {
        success: true,
        data: mockSecrets
      })

      const result = await secretsApi.listSecrets()
      expect(result).toEqual(mockSecrets)
    })
  })

  describe('createSecret', () => {
    it('should create secret and return it', async () => {
      const mockSecret = createMockSecret('NEW_API_KEY')

      mock.onPost(API_ENDPOINTS.SECRET.CREATE).reply(200, {
        success: true,
        data: mockSecret
      })

      const result = await secretsApi.createSecret(
        'NEW_API_KEY',
        'secret-value',
        'Test description'
      )
      expect(result).toEqual(mockSecret)
    })

    it('should create secret without description', async () => {
      const mockSecret = createMockSecret('SIMPLE_KEY')

      mock.onPost(API_ENDPOINTS.SECRET.CREATE).reply(200, {
        success: true,
        data: mockSecret
      })

      const result = await secretsApi.createSecret('SIMPLE_KEY', 'value')
      expect(result.key).toBe('SIMPLE_KEY')
    })
  })

  describe('updateSecret', () => {
    it('should update secret', async () => {
      mock.onPut(API_ENDPOINTS.SECRET.UPDATE('EXISTING_KEY')).reply(200, {
        success: true
      })

      await expect(
        secretsApi.updateSecret('EXISTING_KEY', 'new-value', 'Updated desc')
      ).resolves.toBeUndefined()
    })

    it('should update secret without description', async () => {
      mock.onPut(API_ENDPOINTS.SECRET.UPDATE('EXISTING_KEY')).reply(200, {
        success: true
      })

      await expect(
        secretsApi.updateSecret('EXISTING_KEY', 'new-value')
      ).resolves.toBeUndefined()
    })
  })

  describe('deleteSecret', () => {
    it('should delete secret', async () => {
      mock.onDelete(API_ENDPOINTS.SECRET.DELETE('OLD_KEY')).reply(200, {
        success: true
      })

      await expect(secretsApi.deleteSecret('OLD_KEY')).resolves.toBeUndefined()
    })
  })

  describe('Error Handling', () => {
    it('should handle network timeout', async () => {
      mock.onGet(API_ENDPOINTS.SECRET.LIST).timeout()
      await expect(secretsApi.listSecrets()).rejects.toThrow()
    })

    it('should handle 404 secret not found', async () => {
      mock.onPut(API_ENDPOINTS.SECRET.UPDATE('MISSING_KEY')).reply(404, {
        success: false,
        message: 'Secret not found'
      })
      await expect(secretsApi.updateSecret('MISSING_KEY', 'value')).rejects.toThrow('Secret not found')
    })

    it('should handle duplicate secret key', async () => {
      mock.onPost(API_ENDPOINTS.SECRET.CREATE).reply(200, {
        success: false,
        message: 'Secret already exists'
      })
      await expect(secretsApi.createSecret('DUPLICATE_KEY', 'value')).rejects.toThrow('Secret already exists')
    })

    it('should handle network error', async () => {
      mock.onGet(API_ENDPOINTS.SECRET.LIST).networkError()
      await expect(secretsApi.listSecrets()).rejects.toThrow()
    })
  })
})
