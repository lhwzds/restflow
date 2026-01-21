import { apiClient, isTauri, tauriInvoke } from './config'
import type { Secret } from '@/types/generated/Secret'
import { API_ENDPOINTS } from '@/constants'

// Tauri returns SecretInfo without value for security
interface TauriSecretInfo {
  key: string
  description: string | null
  created_at: number
  updated_at: number
}

// List all secrets (returns keys only, not values)
export async function listSecrets(): Promise<Secret[]> {
  if (isTauri()) {
    const secrets = await tauriInvoke<TauriSecretInfo[]>('list_secrets')
    // Convert TauriSecretInfo to Secret format (value is empty for security)
    return secrets.map((s) => ({
      key: s.key,
      value: '', // Value is not returned for security
      description: s.description,
      created_at: s.created_at,
      updated_at: s.updated_at,
    }))
  }
  const response = await apiClient.get<Secret[]>(API_ENDPOINTS.SECRET.LIST)
  return response.data
}

export async function createSecret(
  key: string,
  value: string,
  description?: string,
): Promise<Secret> {
  if (isTauri()) {
    const result = await tauriInvoke<TauriSecretInfo>('create_secret', {
      request: { key, value, description: description || null },
    })
    return {
      key: result.key,
      value: '', // Value is not returned for security
      description: result.description,
      created_at: result.created_at,
      updated_at: result.updated_at,
    }
  }
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
  if (isTauri()) {
    await tauriInvoke<TauriSecretInfo>('update_secret', {
      key,
      request: { value, description: description || null },
    })
    return
  }
  await apiClient.put(API_ENDPOINTS.SECRET.UPDATE(key), {
    value,
    description,
  })
}

export async function deleteSecret(key: string): Promise<void> {
  if (isTauri()) {
    return tauriInvoke<void>('delete_secret', { key })
  }
  await apiClient.delete(API_ENDPOINTS.SECRET.DELETE(key))
}
