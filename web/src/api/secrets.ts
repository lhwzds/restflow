import { apiClient } from './config'
import type { Secret } from '@/types/generated/Secret'
import { API_ENDPOINTS } from '@/constants'

// List all secrets (returns keys only, not values)
export async function listSecrets(): Promise<Secret[]> {
  const response = await apiClient.get<Secret[]>(API_ENDPOINTS.SECRET.LIST)
  return response.data
}

export async function createSecret(
  key: string,
  value: string,
  description?: string,
): Promise<Secret> {
  const response = await apiClient.post<Secret>(API_ENDPOINTS.SECRET.CREATE, {
    key,
    value,
    description,
  })
  return response.data
}

export async function updateSecret(
  key: string,
  value: string,
  description?: string,
): Promise<void> {
  await apiClient.put(API_ENDPOINTS.SECRET.UPDATE(key), {
    value,
    description,
  })
}

export async function deleteSecret(key: string): Promise<void> {
  await apiClient.delete(API_ENDPOINTS.SECRET.DELETE(key))
}
