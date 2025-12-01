import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient, isApiResponse } from '@/api/config'
import type { ApiResponse } from '@/types/api'

describe('Axios Response Interceptor', () => {
  let mock: MockAdapter

  beforeEach(() => {
    mock = new MockAdapter(apiClient)
  })

  afterEach(() => {
    mock.reset()
  })

  describe('isApiResponse type guard logic', () => {
    it('should identify valid ApiResponse with success and data', () => {
      const data: ApiResponse<string> = {
        success: true,
        data: 'test-data',
      }

      expect(isApiResponse(data)).toBe(true)
    })

    it('should identify valid ApiResponse with success only', () => {
      const data: ApiResponse<never> = {
        success: true,
      }

      expect(isApiResponse(data)).toBe(true)
    })

    it('should identify valid ApiResponse with error message', () => {
      const data: ApiResponse<never> = {
        success: false,
        message: 'Error occurred',
      }

      expect(isApiResponse(data)).toBe(true)
    })

    it('should reject business data with extra fields', () => {
      const businessData = {
        success: true,
        payload: { id: 1 },
        count: 42,
      }

      expect(isApiResponse(businessData)).toBe(false)
    })

    it('should reject object without success field', () => {
      const data = {
        data: 'test',
        message: 'hello',
      }

      expect(isApiResponse(data)).toBe(false)
    })

    it('should reject non-boolean success field', () => {
      const data = {
        success: 'true',
        data: 'test',
      }

      expect(isApiResponse(data)).toBe(false)
    })

    it('should reject null', () => {
      expect(isApiResponse(null)).toBe(false)
    })

    it('should reject undefined', () => {
      expect(isApiResponse(undefined)).toBe(false)
    })

    it('should reject primitive values', () => {
      expect(isApiResponse('string')).toBe(false)
      expect(isApiResponse(123)).toBe(false)
      expect(isApiResponse(true)).toBe(false)
    })

    it('should reject array', () => {
      expect(isApiResponse([{ success: true }])).toBe(false)
    })
  })

  describe('Response interceptor behavior', () => {
    it('should unwrap successful ApiResponse', async () => {
      const apiResponse: ApiResponse<{ id: number; name: string }> = {
        success: true,
        data: { id: 1, name: 'test' },
      }

      mock.onGet('/test').reply(200, apiResponse)

      const response = await apiClient.get('/test')
      expect(response.data).toEqual({ id: 1, name: 'test' })
    })

    it('should reject failed ApiResponse', async () => {
      const apiResponse: ApiResponse<never> = {
        success: false,
        message: 'Operation failed',
      }

      mock.onGet('/test').reply(200, apiResponse)

      await expect(apiClient.get('/test')).rejects.toThrow('Operation failed')
    })

    it('should pass through non-ApiResponse data unchanged', async () => {
      const businessData = {
        success: true,
        payload: { id: 1 },
        count: 42,
      }

      mock.onGet('/test').reply(200, businessData)

      const response = await apiClient.get('/test')
      expect(response.data).toEqual(businessData)
    })

    it('should handle ApiResponse with no data field', async () => {
      const apiResponse: ApiResponse<never> = {
        success: true,
      }

      mock.onGet('/test').reply(200, apiResponse)

      const response = await apiClient.get('/test')
      expect(response.data).toBeUndefined()
    })

    it('should handle failed ApiResponse with default message', async () => {
      const apiResponse: ApiResponse<never> = {
        success: false,
      }

      mock.onGet('/test').reply(200, apiResponse)

      await expect(apiClient.get('/test')).rejects.toThrow('Request failed')
    })
  })
})
