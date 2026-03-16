import { requestOptional, requestTyped } from './http-client'
import type { ModelMetadataDTO } from '@/types/generated/ModelMetadataDTO'

export type SystemConfig = Record<string, unknown>

export interface ToolDefinition {
  name: string
  description: string | null
}

/** Fetch runtime system configuration from backend. */
export async function getSystemConfig(): Promise<SystemConfig> {
  return requestTyped<SystemConfig>({ type: 'GetConfig' })
}

/** Persist runtime system configuration to backend. */
export async function updateSystemConfig(config: SystemConfig): Promise<SystemConfig> {
  await requestTyped({ type: 'SetConfig', data: { config } })
  return config
}

/** Check whether a secret exists by key. */
export async function hasSecretKey(key: string): Promise<boolean> {
  const secret = await requestOptional<{ value: string | null }>({
    type: 'GetSecret',
    data: { key },
  })
  return Boolean(secret?.value)
}

/** List models advertised by the daemon runtime. */
export async function getAvailableModels(): Promise<ModelMetadataDTO[]> {
  return requestTyped<ModelMetadataDTO[]>({ type: 'GetAvailableModels' })
}

/** List available tool definitions advertised by the daemon runtime. */
export async function getAvailableTools(): Promise<ToolDefinition[]> {
  return requestTyped<ToolDefinition[]>({ type: 'GetAvailableToolDefinitions' })
}
