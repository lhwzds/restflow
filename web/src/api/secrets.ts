import { requestTyped } from './http-client'
import type { Secret } from '@/types/generated/Secret'

export async function listSecrets(): Promise<Secret[]> {
  return requestTyped<Secret[]>({ type: 'ListSecrets' })
}

export async function createSecret(
  key: string,
  value: string,
  description?: string,
): Promise<Secret> {
  await requestTyped({
    type: 'CreateSecret',
    data: {
      key,
      value,
      description: description ?? null,
    },
  })

  const secrets = await listSecrets()
  const secret = secrets.find((entry) => entry.key === key)

  return {
    key,
    value: secret?.value ?? '',
    description: secret?.description ?? description ?? null,
    created_at: secret?.created_at ?? Date.now(),
    updated_at: secret?.updated_at ?? Date.now(),
  }
}

export async function updateSecret(
  key: string,
  value: string,
  description?: string,
): Promise<void> {
  await requestTyped({
    type: 'UpdateSecret',
    data: {
      key,
      value,
      description: description ?? null,
    },
  })
}

export async function deleteSecret(key: string): Promise<void> {
  await requestTyped({ type: 'DeleteSecret', data: { key } })
}
