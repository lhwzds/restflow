import axios from 'axios'
import type { ApiResponse } from '@/types/api'

// Re-export Tauri utilities for use in API modules
export { isTauri, tauriInvoke } from './tauri-client'

export const API_BASE_URL = import.meta.env.VITE_API_URL || ''

export const apiClient = axios.create({
  baseURL: API_BASE_URL,
  headers: {
    'Content-Type': 'application/json',
  },
})

// Request interceptor
apiClient.interceptors.request.use(
  (config) => {
    return config
  },
  (error) => {
    return Promise.reject(error)
  },
)

export const isApiResponse = (obj: any): obj is ApiResponse<any> => {
  if (!obj || typeof obj !== 'object') return false
  if (typeof obj.success !== 'boolean') return false

  const keys = Object.keys(obj)
  const allowedKeys = new Set(['success', 'data', 'message'])

  return keys.length > 0 && keys.includes('success') && keys.every((k) => allowedKeys.has(k))
}

apiClient.interceptors.response.use(
  (response) => {
    const data = response.data

    if (isApiResponse(data)) {
      if (!data.success) {
        return Promise.reject(new Error(data.message || 'Request failed'))
      }

      response.data = data.data
    }

    return response
  },
  (error) => {
    if (error.response?.data && isApiResponse(error.response.data)) {
      const apiError = error.response.data
      console.error('API Error:', apiError.message || error.message)
      return Promise.reject(new Error(apiError.message || error.message))
    }

    console.error('API Error:', error)
    return Promise.reject(error)
  },
)
