import { tauriInvoke } from './tauri-client'
import type { Secret } from '@/types/generated/Secret'

// Tauri returns SecretInfo without value for security
interface TauriSecretInfo {
  key: string
  description: string | null
  created_at: number
  updated_at: number
}

// List all secrets (returns keys only, not values)
export async function listSecrets(): Promise<Secret[]> {
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

export async function createSecret(
  key: string,
  value: string,
  description?: string,
): Promise<Secret> {
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

export async function updateSecret(
  key: string,
  value: string,
  description?: string,
): Promise<void> {
  await tauriInvoke<TauriSecretInfo>('update_secret', {
    key,
    request: { value, description: description || null },
  })
}

export async function deleteSecret(key: string): Promise<void> {
  return tauriInvoke<void>('delete_secret', { key })
}
